//! ATLauncher pack API client. Mirrors launcher/modplatform/atlauncher.
//!
//! ATLauncher's public pack data lives on its CDN:
//! * pack index: `launcher/json/packsnew.json`
//! * pack version manifest: `packs/{SafeName}/versions/{version}/Configs.json`
//!
//! The version manifest lists mods, libraries and config files. Downloads are
//! either relative to the CDN (`download: "server"`), absolute
//! (`download: "direct"`), or browser-only (`download: "browser"`), which
//! cannot be automated and is reported as blocked.

use serde::{Deserialize, Deserializer};

pub const CDN_BASE: &str = "https://download.nodecdn.net/containers/atl/";

/// Deserialize a possibly-null value as its `Default`.
fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AtPack {
    pub id: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, deserialize_with = "null_default")]
    pub description: String,
    #[serde(default)]
    pub versions: Vec<AtVersion>,
    #[serde(default)]
    pub dev_versions: Vec<AtVersion>,
    #[serde(default, rename = "type", deserialize_with = "null_default")]
    pub pack_type: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AtVersion {
    #[serde(default, deserialize_with = "null_default")]
    pub version: String,
    #[serde(default, deserialize_with = "null_default")]
    pub minecraft: String,
    #[serde(default, rename = "isRecommended")]
    pub is_recommended: bool,
}

impl AtPack {
    /// ATLauncher "safe name": alphanumeric characters of the pack name,
    /// case preserved.
    pub fn safe_name(&self) -> String {
        safe_name(&self.name)
    }

    /// The newest version, preferring ATLauncher's recommended build.
    pub fn latest_version(&self) -> Option<&AtVersion> {
        self.versions
            .iter()
            .find(|v| v.is_recommended)
            .or_else(|| self.versions.first())
    }
}

pub fn safe_name(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AtManifest {
    #[serde(default, deserialize_with = "null_default")]
    pub version: String,
    #[serde(default, deserialize_with = "null_default")]
    pub minecraft: String,
    #[serde(default)]
    pub mods: Vec<AtFile>,
    #[serde(default)]
    pub libraries: Vec<AtFile>,
    #[serde(default)]
    pub configs: AtConfigs,
    #[serde(default)]
    pub messages: AtMessages,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AtMessages {
    #[serde(default)]
    pub install: String,
    #[serde(default)]
    pub update: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AtConfigs {
    #[serde(default)]
    pub files: Vec<AtFile>,
    #[serde(default)]
    pub folders: Vec<AtFolder>,
    #[serde(default)]
    pub no_configs: bool,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct AtFolder {
    #[serde(default, deserialize_with = "null_default")]
    pub path: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AtFile {
    #[serde(default, deserialize_with = "null_default")]
    pub name: String,
    #[serde(default, rename = "type", deserialize_with = "null_default")]
    pub file_type: String,
    /// `server`, `direct`, or `browser`.
    #[serde(default, deserialize_with = "null_default")]
    pub download: String,
    #[serde(default, deserialize_with = "null_default")]
    pub url: String,
    /// Destination file name.
    #[serde(default, deserialize_with = "null_default")]
    pub file: String,
    #[serde(default, deserialize_with = "null_default")]
    pub md5: String,
    #[serde(default, deserialize_with = "null_default")]
    pub version: String,
    /// Maven-style server path for libraries.
    #[serde(default, deserialize_with = "null_default")]
    pub server: String,
    #[serde(default, deserialize_with = "null_default")]
    pub path: String,
}

impl AtFile {
    /// Absolute download URL, when the file can be downloaded non-interactively.
    pub fn download_url(&self) -> Option<String> {
        match self.download.as_str() {
            "server" => Some(format!("{CDN_BASE}{}", self.url.trim_start_matches('/'))),
            "direct" => Some(self.url.clone()),
            _ => None,
        }
    }

    /// Destination sub-directory inside the game dir, following Prism's
    /// ATLauncher `modTypeToDir` mapping.
    pub fn target_dir(&self, minecraft: &str) -> Option<String> {
        let kind = self.file_type.to_ascii_lowercase();
        Some(match kind.as_str() {
            "root" | "extract" | "decomp" | "texturepackextract" | "resourcepackextract"
            | "mcpc" => return None,
            "forge" | "jar" | "jarmods" => "mods".to_string(),
            "mods" | "mod" => "mods".to_string(),
            "flan" => "Flan".to_string(),
            "dependency" => format!("mods/{minecraft}"),
            "ic2lib" => "mods/ic2".to_string(),
            "denlib" => "mods/denlib".to_string(),
            "coremods" | "coremod" => "coremods".to_string(),
            "plugins" => "plugins".to_string(),
            "texturepack" => "texturepacks".to_string(),
            "resourcepack" => "resourcepacks".to_string(),
            "shaderpack" => "shaderpacks".to_string(),
            "millenaire" => "millenaire".to_string(),
            _ => return None,
        })
    }
}

pub struct AtLauncher;

impl AtLauncher {
    /// Download the full public pack index.
    pub fn packs(&self) -> Result<Vec<AtPack>, String> {
        let body = crate::net::get_string(&format!("{CDN_BASE}launcher/json/packsnew.json"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Search the pack index by name/description (the API has no server-side
    /// search endpoint).
    pub fn search(&self, query: &str) -> Result<Vec<AtPack>, String> {
        let q = query.to_ascii_lowercase();
        let mut packs = self.packs()?;
        if !q.trim().is_empty() {
            packs.retain(|p| {
                p.name.to_ascii_lowercase().contains(&q)
                    || p.description.to_ascii_lowercase().contains(&q)
            });
        }
        packs.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
        Ok(packs)
    }

    /// Fetch a pack version manifest.
    pub fn manifest(&self, safe_name: &str, version: &str) -> Result<AtManifest, String> {
        let url = format!(
            "{CDN_BASE}packs/{}/{}/Configs.json",
            urlencode(safe_name),
            urlencode(version)
        );
        let body = crate::net::get_string(&url)?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Install a pack version into `game_dir`.
    ///
    /// Returns the number of files downloaded and the list of mods that can
    /// only be fetched through a browser.
    pub fn install(
        &self,
        safe_name: &str,
        version: &str,
        game_dir: &std::path::Path,
    ) -> Result<(usize, Vec<String>), String> {
        let manifest = self.manifest(safe_name, version)?;
        std::fs::create_dir_all(game_dir).map_err(|e| e.to_string())?;
        let mut n = 0usize;
        let mut blocked = Vec::new();

        for file in manifest.mods.iter().chain(manifest.configs.files.iter()) {
            let Some(dir) = file.target_dir(&manifest.minecraft) else {
                continue;
            };
            let Some(url) = file.download_url() else {
                if !file.name.is_empty() {
                    blocked.push(file.name.clone());
                }
                continue;
            };
            let name = if !file.file.is_empty() {
                file.file.clone()
            } else if !file.path.is_empty() {
                file.path.clone()
            } else if !file.name.is_empty() {
                file.name.clone()
            } else {
                continue;
            };
            let dest = crate::fs::safe_join(game_dir, &format!("{dir}/{name}"))
                .ok_or_else(|| format!("unsafe path in ATLauncher manifest: {dir}/{name}"))?;
            let md5 = if file.md5.is_empty() { None } else { Some(file.md5.as_str()) };
            crate::net::download_verified_md5(&url, &dest, md5)
                .map_err(|e| format!("{}: {e}", file.name))?;
            n += 1;
        }

        for lib in &manifest.libraries {
            let Some(url) = lib.download_url() else {
                continue;
            };
            let rel = if !lib.server.is_empty() {
                lib.server.clone()
            } else if !lib.file.is_empty() {
                lib.file.clone()
            } else {
                continue;
            };
            let dest = crate::fs::safe_join(game_dir, &format!("libraries/{rel}"))
                .ok_or_else(|| format!("unsafe library path: {rel}"))?;
            let md5 = if lib.md5.is_empty() { None } else { Some(lib.md5.as_str()) };
            crate::net::download_verified_md5(&url, &dest, md5)?;
            n += 1;
        }

        // Config archives are published as a single zip when the manifest does
        // not enumerate config files.
        if manifest.configs.files.is_empty() && !manifest.configs.no_configs {
            let url = format!(
                "{CDN_BASE}packs/{}/{}/Configs.zip",
                urlencode(safe_name),
                urlencode(version)
            );
            let tmp = std::env::temp_dir().join(format!(
                "prism-atl-{}-{}.zip",
                std::process::id(),
                safe_name
            ));
            if crate::net::download_verified(&url, &tmp, None).is_ok() {
                let _ = crate::fs::extract_zip(&tmp, game_dir, false);
                let _ = std::fs::remove_file(&tmp);
                n += 1;
            }
        }

        Ok((n, blocked))
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_safe_names() {
        assert_eq!(safe_name("MoonQuest"), "MoonQuest");
        assert_eq!(safe_name("FTB Presents: Direwolf20"), "FTBPresentsDirewolf20");
        assert_eq!(safe_name("A B-C_D!"), "ABCD");
    }

    #[test]
    fn resolves_download_urls() {
        let mut f = AtFile {
            download: "server".into(),
            url: "packs/X/files/a.jar".into(),
            ..Default::default()
        };
        assert_eq!(
            f.download_url().as_deref(),
            Some("https://download.nodecdn.net/containers/atl/packs/X/files/a.jar")
        );
        f.download = "direct".into();
        f.url = "https://example.com/a.jar".into();
        assert_eq!(f.download_url().as_deref(), Some("https://example.com/a.jar"));
        f.download = "browser".into();
        assert_eq!(f.download_url(), None);
    }

    #[test]
    fn maps_mod_types_to_directories() {
        let mk = |t: &str| AtFile {
            file_type: t.into(),
            ..Default::default()
        };
        assert_eq!(mk("mods").target_dir("1.12.2").as_deref(), Some("mods"));
        assert_eq!(mk("forge").target_dir("1.12.2").as_deref(), Some("mods"));
        assert_eq!(
            mk("dependency").target_dir("1.12.2").as_deref(),
            Some("mods/1.12.2")
        );
        assert_eq!(mk("root").target_dir("1.12.2"), None);
    }

    #[test]
    fn parses_pack_index_and_manifest() {
        let idx: Vec<AtPack> = serde_json::from_str(
            r#"[{"id":1,"name":"MoonQuest","type":"public","description":"d",
                 "versions":[{"version":"0.15.1","minecraft":"1.6.4","isRecommended":true}]}]"#,
        )
        .unwrap();
        assert_eq!(idx[0].safe_name(), "MoonQuest");
        assert_eq!(idx[0].latest_version().unwrap().version, "0.15.1");

        let m: AtManifest = serde_json::from_str(
            r#"{"version":"1","minecraft":"1.12.2",
                "mods":[{"name":"a","type":"mods","download":"server","url":"packs/X/files/a.jar","file":"a.jar","md5":"abc"}],
                "libraries":[{"url":"packs/X/files/l.jar","server":"g/l/1/l.jar","download":"server"}],
                "configs":{"files":[],"folders":[]}}"#,
        )
        .unwrap();
        assert_eq!(m.mods.len(), 1);
        assert_eq!(m.mods[0].file, "a.jar");
        assert_eq!(m.libraries[0].server, "g/l/1/l.jar");
    }
}
