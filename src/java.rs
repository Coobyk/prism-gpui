//! Java detection. Mirrors launcher/java/JavaInstall, JavaInstallList, JavaVersion.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub version: String,
    pub vendor: String,
    pub arch: String,
}

impl JavaInstall {
    pub fn probe(java_bin: &Path) -> Option<Self> {
        let out = Command::new(java_bin)
            .arg("-XshowSettings:properties")
            .arg("-version")
            .output()
            .ok()?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut version = String::new();
        let mut vendor = String::new();
        let mut arch = String::new();
        for line in stderr.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "java.version" => version = value.trim().to_string(),
                "java.vendor" => vendor = value.trim().to_string(),
                "os.arch" => arch = value.trim().to_string(),
                _ => {}
            }
        }
        // fallback: parse "java version \"X\"" line
        if version.is_empty() {
            for line in stderr.lines() {
                if line.contains("version") {
                    if let Some(s) = line.split('"').nth(1) {
                        version = s.to_string();
                        break;
                    }
                }
            }
        }
        Some(JavaInstall {
            path: java_bin.to_path_buf(),
            version,
            vendor,
            arch,
        })
    }
}

/// Scan `<data_root>/java` for java installations (Prism stores them there).
pub fn discover_java(data_root: &Path) -> Vec<JavaInstall> {
    let mut out = Vec::new();
    let java_dir = data_root.join("java");
    if let Ok(entries) = std::fs::read_dir(&java_dir) {
        for e in entries.flatten() {
            let bin = e.path().join("bin").join("java");
            if bin.is_file() {
                if let Some(inst) = JavaInstall::probe(&bin) {
                    out.push(inst);
                }
            }
        }
    }
    // Also check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(':') {
            let bin = PathBuf::from(dir).join("java");
            if bin.is_file() {
                if let Some(inst) = JavaInstall::probe(&bin) {
                    if !out.iter().any(|i| i.path == inst.path) {
                        out.push(inst);
                    }
                }
            }
        }
    }
    out
}

// ---- Automatic Java runtime download (Mojang java-runtime CDN) ----

const RUNTIMES_INDEX: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

fn os_key() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else if cfg!(target_arch = "aarch64") {
        "linux"
    } else {
        "linux"
    }
}

/// Download a Mojang Java runtime matching `major` into `<data_root>/java/<name>`.
/// Returns the path to the `java` binary.
pub fn download_runtime(
    data_root: &Path,
    component: &str,
    major: &str,
) -> Result<PathBuf, String> {
    let body = crate::net::get_string(RUNTIMES_INDEX)?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let entry = json
        .get(component)
        .and_then(|c| c.get(os_key()))
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.iter().find(|r| r.get("version").and_then(|v| v.get("name")).and_then(|n| n.as_str()) == Some(major)))
        .or_else(|| {
            json.get(component)
                .and_then(|c| c.get(os_key()))
                .and_then(|a| a.as_array())
                .and_then(|arr| arr.first())
        })
        .ok_or_else(|| format!("no runtime for {component} {major} on {}", os_key()))?;

    let manifest_url = entry
        .get("manifest")
        .and_then(|m| m.get("url"))
        .and_then(|u| u.as_str())
        .ok_or("runtime has no manifest")?;

    let dest_root = data_root.join("java").join(component);
    let manifest = crate::net::get_string(manifest_url)?;
    let mf: serde_json::Value = serde_json::from_str(&manifest).map_err(|e| e.to_string())?;
    let files = mf
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or("runtime manifest has no files")?;

    for (rel, info) in files {
        if info.get("type").and_then(|t| t.as_str()) == Some("directory") {
            std::fs::create_dir_all(dest_root.join(rel)).map_err(|e| e.to_string())?;
            continue;
        }
        let Some(download) = info.get("downloads").and_then(|d| d.get("raw")) else {
            continue;
        };
        let url = download.get("url").and_then(|u| u.as_str()).unwrap_or("");
        if url.is_empty() {
            continue;
        }
        let sha1 = download.get("sha1").and_then(|s| s.as_str());
        let dest = crate::fs::safe_join(&dest_root, rel)
            .ok_or_else(|| format!("unsafe path in runtime manifest: {rel}"))?;
        crate::net::download_verified(url, &dest, sha1)?;
    }

    let bin = dest_root.join("bin").join("java");
    if bin.is_file() {
        // Ensure the extracted binary is executable on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&bin) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&bin, perms);
            }
        }
        Ok(bin)
    } else {
        Err(format!("runtime downloaded but java binary missing: {}", bin.display()))
    }
}

/// Ensure a Java runtime for `major` exists (download if needed).
pub fn ensure_runtime(data_root: &Path, component: &str, major: &str) -> Result<PathBuf, String> {
    let bin = data_root.join("java").join(component).join("bin").join("java");
    if bin.is_file() {
        return Ok(bin);
    }
    download_runtime(data_root, component, major)
}
