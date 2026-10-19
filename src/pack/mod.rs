//! Modpack import/export. Mirrors launcher/InstanceImportTask, InstanceCopyTask,
//! launcher/minecraft/PackProfile and the various modplatform pack formats.

pub mod curseforge;
pub mod mmc;
pub mod mrpack;

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PackDependencies {
    pub minecraft: Option<String>,
    pub fabric_loader: Option<String>,
    pub forge: Option<String>,
    pub quilt_loader: Option<String>,
    pub neo_forge: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PackFile {
    pub path: String,
    pub sha1: Option<String>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct PackManifest {
    pub name: String,
    pub version: String,
    pub dependencies: PackDependencies,
    pub files: Vec<PackFile>,
}

/// Guess the pack format from a file name/extension.
pub fn detect_format(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.ends_with(".mrpack") {
        "mrpack"
    } else if name.ends_with(".zip") {
        "zip"
    } else {
        "unknown"
    }
}
