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

/// Scan the configured java directory and PATH for Java installations. The
/// `JavaPath` setting (an explicit binary) is preferred when set.
pub fn discover_java(data_root: &Path) -> Vec<JavaInstall> {
    let mut out = Vec::new();
    let settings = crate::settings::GlobalSettings::load(data_root);
    let explicit = settings.java_path().trim().to_string();
    if !explicit.is_empty() {
        let p = PathBuf::from(&explicit);
        if p.is_file() {
            if let Some(inst) = JavaInstall::probe(&p) {
                out.push(inst);
            }
        }
    }
    let java_dir = settings.java_dir();
    if let Ok(entries) = std::fs::read_dir(&java_dir) {
        for e in entries.flatten() {
            let bin = e.path().join("bin").join("java");
            if bin.is_file() {
                if let Some(inst) = JavaInstall::probe(&bin) {
                    if !out.iter().any(|i| i.path == inst.path) {
                        out.push(inst);
                    }
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

/// Parse a `java.version` string into a major version number.
pub fn major_version(version: &str) -> u32 {
    let v = version.trim();
    if let Some(rest) = v.strip_prefix("1.") {
        rest.split(['.', '_', '-'])
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    } else {
        v.split(['.', '_', '-'])
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

/// Best-effort mapping from a Minecraft version to its required Java major.
pub fn heuristic_major(mc_version: &str) -> u32 {
    let mut parts = mc_version.split(['.', '-', '+']);
    let a = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let b = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let c = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    if a != 1 {
        // New-style versions (e.g. 26.1) track modern Java.
        return 21;
    }
    match (b, c) {
        (0..=16, _) => 8,
        (17, 0) => 16,
        (17, _) => 17,
        (18..=20, 0..=4) => 17,
        (20, 5..) => 21,
        (21.., _) => 21,
        _ => 17,
    }
}

/// Choose the best installed Java for the given required major version:
/// an exact match first, then the oldest runtime that is new enough, then any.
pub fn select_java(data_root: &Path, required_major: u32) -> Option<JavaInstall> {
    let all = discover_java(data_root);
    if all.is_empty() {
        return None;
    }
    if let Some(exact) = all
        .iter()
        .find(|j| major_version(&j.version) == required_major)
    {
        return Some(exact.clone());
    }
    let mut enough: Vec<&JavaInstall> = all
        .iter()
        .filter(|j| major_version(&j.version) >= required_major)
        .collect();
    enough.sort_by_key(|j| major_version(&j.version));
    if let Some(j) = enough.first() {
        return Some((*j).clone());
    }
    all.into_iter().next()
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
    let platform = json
        .get(os_key())
        .and_then(|p| p.as_object())
        .ok_or_else(|| format!("runtime index has no platform {}", os_key()))?;
    let candidates = platform
        .get(component)
        .and_then(|a| a.as_array())
        .ok_or_else(|| format!("no runtime {component} for {}", os_key()))?;
    let entry = candidates
        .iter()
        .find(|r| {
            r.get("version")
                .and_then(|v| v.get("name"))
                .and_then(|n| n.as_str())
                .map(|n| n == major || n.starts_with(&format!("{major}.")))
                .unwrap_or(false)
        })
        .or_else(|| candidates.first())
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

/// Mojang runtime component candidates for a required Java major, newest
/// launcher component naming first.
pub fn runtime_components(required_major: u32) -> Vec<&'static str> {
    match required_major {
        0..=8 => vec!["jre-legacy"],
        9..=16 => vec!["java-runtime-alpha", "java-runtime-beta"],
        17 => vec!["java-runtime-gamma", "java-runtime-beta"],
        18..=20 => vec!["java-runtime-gamma"],
        21 => vec!["java-runtime-delta", "java-runtime-gamma"],
        _ => vec!["java-runtime-delta", "java-runtime-gamma"],
    }
}

/// Resolve a Java binary for an instance: use a matching installed runtime,
/// otherwise download the appropriate Mojang runtime (when enabled in
/// settings), otherwise fall back to any installed runtime.
pub fn ensure_for_instance(data_root: &Path, inst_dir: &Path) -> Result<PathBuf, String> {
    let required = crate::meta::required_java_major(inst_dir, data_root);
    if let Some(j) = select_java(data_root, required) {
        return Ok(j.path);
    }
    let settings = crate::settings::GlobalSettings::load(data_root);
    if settings.get_bool("AutomaticJavaDownload") {
        let mut last = String::new();
        for component in runtime_components(required) {
            match ensure_runtime(data_root, component, &required.to_string()) {
                Ok(p) => return Ok(p),
                Err(e) => last = e,
            }
        }
        return Err(format!(
            "no Java {required} runtime installed and download failed: {last}"
        ));
    }
    Err(format!(
        "no Java {required} runtime installed (enable Automatic Java Download in settings)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_major_versions() {
        assert_eq!(major_version("1.8.0_402"), 8);
        assert_eq!(major_version("17.0.9"), 17);
        assert_eq!(major_version("21"), 21);
        assert_eq!(major_version("25.0.1"), 25);
    }

    #[test]
    fn heuristic_matches_minecraft_versions() {
        assert_eq!(heuristic_major("1.8.9"), 8);
        assert_eq!(heuristic_major("1.16.5"), 8);
        assert_eq!(heuristic_major("1.17"), 16);
        assert_eq!(heuristic_major("1.17.1"), 17);
        assert_eq!(heuristic_major("1.18.2"), 17);
        assert_eq!(heuristic_major("1.20.4"), 17);
        assert_eq!(heuristic_major("1.20.5"), 21);
        assert_eq!(heuristic_major("1.21.11"), 21);
        assert_eq!(heuristic_major("26.1.2"), 21);
    }

    #[test]
    fn runtime_component_mapping() {
        assert_eq!(runtime_components(8)[0], "jre-legacy");
        assert_eq!(runtime_components(16)[0], "java-runtime-alpha");
        assert_eq!(runtime_components(17)[0], "java-runtime-gamma");
        assert_eq!(runtime_components(21)[0], "java-runtime-delta");
    }
}
