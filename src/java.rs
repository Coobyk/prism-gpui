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
