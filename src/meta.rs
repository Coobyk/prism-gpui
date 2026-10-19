//! Metadata parsing: Prism component packs and Mojang version JSON.
//! Mirrors: launcher/meta/*, launcher/minecraft/MojangVersionFormat, OneSixVersionFormat.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct ComponentPack {
    #[serde(default)]
    pub components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    pub uid: String,
    pub version: String,
    #[serde(default, rename = "cachedName")]
    pub cached_name: String,
    #[serde(default, rename = "dependencyOnly")]
    pub dependency_only: bool,
}

pub fn read_component_pack(inst_dir: &Path) -> Option<ComponentPack> {
    let text = std::fs::read_to_string(inst_dir.join("mmc-pack.json")).ok()?;
    serde_json::from_str(&text).ok()
}

#[derive(Debug, Deserialize, Default)]
pub struct VersionJson {
    #[serde(default, rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, rename = "type")]
    pub version_type: String,
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AssetIndex {
    pub id: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub clientreq: Option<bool>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Downloads {
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Artifact {
    pub path: String,
    pub sha1: String,
    pub size: u64,
    #[serde(default)]
    pub url: String,
}

impl Library {
    /// Maven path, e.g. org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar
    pub fn maven_path(&self) -> Option<String> {
        if let Some(path) = self
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.path.clone())
        {
            return Some(path);
        }
        // derive from group:artifact:version
        let parts: Vec<&str> = self.name.split(':').collect();
        if parts.len() < 3 {
            return None;
        }
        let (group, artifact, version) = (parts[0], parts[1], parts[2]);
        Some(format!(
            "{}/{}/{}/{}-{}.jar",
            group.replace('.', "/"),
            artifact,
            version,
            artifact,
            version
        ))
    }
}

pub fn read_version_json(path: &Path) -> Option<VersionJson> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Resolve the classpath jars for a version from `<libraries>/<maven path>`.
pub fn classpath(libs_dir: &Path, version: &VersionJson) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for lib in &version.libraries {
        if let Some(p) = lib.maven_path() {
            let full = libs_dir.join(p);
            if full.is_file() {
                out.push(full);
            }
        }
    }
    out
}

impl Library {
    /// Resolve the download URL for this library's artifact.
    pub fn artifact_url(&self) -> Option<String> {
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            if !a.url.is_empty() {
                return Some(a.url.clone());
            }
        }
        let path = self.maven_path()?;
        let base = self
            .url
            .clone()
            .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
        Some(format!("{}{}", base, path))
    }

    pub fn artifact_sha1(&self) -> Option<&str> {
        self.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.sha1.as_str())
            .filter(|s| !s.is_empty())
    }
}

/// Ensure all of a version's libraries are present under `<data_root>/libraries`.
/// Returns the number of files downloaded.
pub fn ensure_libraries(
    data_root: &Path,
    version: &VersionJson,
) -> Result<usize, String> {
    let libs_dir = data_root.join("libraries");
    let mut downloaded = 0;
    for lib in &version.libraries {
        let Some(rel) = lib.maven_path() else { continue };
        let Some(url) = lib.artifact_url() else { continue };
        let dest = libs_dir.join(&rel);
        let before = dest.is_file();
        crate::net::download_verified(&url, &dest, lib.artifact_sha1())?;
        if !before && dest.is_file() {
            downloaded += 1;
        }
    }
    Ok(downloaded)
}

/// Ensure the asset index file is present; returns its path.
pub fn ensure_asset_index(data_root: &Path, version: &VersionJson) -> Result<Option<std::path::PathBuf>, String> {
    let Some(ai) = &version.asset_index else {
        return Ok(None);
    };
    if ai.url.is_empty() {
        return Ok(None);
    }
    let dest = data_root
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", ai.id));
    crate::net::download_verified(&ai.url, &dest, None)?;
    Ok(Some(dest))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maven_path_download() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1".into(),
            downloads: Some(Downloads {
                artifact: Some(Artifact {
                    path: "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar".into(),
                    ..Default::default()
                }),
            }),
            ..Default::default()
        };
        assert_eq!(
            lib.maven_path().unwrap(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar"
        );
    }
    #[test]
    fn artifact_url_derived() {
        let lib = Library {
            name: "org.ow2.asm:asm:9.10.1".into(),
            url: Some("https://maven.fabricmc.net/".into()),
            ..Default::default()
        };
        assert_eq!(
            lib.artifact_url().unwrap(),
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
    }

    #[test]
    fn maven_path_derived() {
        let lib = Library {
            name: "org.ow2.asm:asm:9.10.1".into(),
            ..Default::default()
        };
        assert_eq!(
            lib.maven_path().unwrap(),
            "org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
    }
}
