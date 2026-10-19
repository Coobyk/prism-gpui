//! FTB modpack API client. Mirrors launcher/modplatform/ftb.
//!
//! The public API (`api.modpacks.ch/public`) exposes:
//! * search → a list of pack *ids*
//! * `/modpack/{id}` → pack details + versions
//! * `/modpack/{id}/{version}` → full file/loader manifest

use serde::Deserialize;

pub const BASE: &str = "https://api.modpacks.ch/public";

#[derive(Deserialize, Debug, Clone)]
pub struct PackVersion {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub release_type: String,
    #[serde(default)]
    pub targets: Vec<Target>,
    #[serde(default)]
    pub updated: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Pack {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub synopsis: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(default)]
    pub plays: u64,
    #[serde(default)]
    pub versions: Vec<PackVersion>,
    #[serde(default)]
    pub art: Vec<Art>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Art {
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "type")]
    pub art_type: String,
}

impl Pack {
    /// Newest version, preferring a stable `release`.
    pub fn latest_version(&self) -> Option<&PackVersion> {
        self.versions
            .iter()
            .find(|v| v.release_type.eq_ignore_ascii_case("release"))
            .or_else(|| self.versions.first())
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct PackManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub targets: Vec<Target>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Target {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, rename = "type")]
    pub type_: String,
}

impl PackManifest {
    /// Loader components implied by the manifest's `targets`, as
    /// `(prism uid, version)` pairs.
    pub fn components(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for t in &self.targets {
            let uid = match t.name.to_ascii_lowercase().as_str() {
                "minecraft" => "net.minecraft",
                "forge" => "net.minecraftforge",
                "fabric" => "net.fabricmc.fabric-loader",
                "neoforge" => "net.neoforged",
                "quilt" => "org.quiltmc.quilt-loader",
                _ => continue,
            };
            out.push((uid.to_string(), t.version.clone()));
        }
        out
    }

    /// The Minecraft version declared by the manifest, if any.
    pub fn minecraft_version(&self) -> Option<&str> {
        self.targets
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case("minecraft"))
            .map(|t| t.version.as_str())
    }
}

/// Normalize an FTB manifest path (strip a leading `./`).
fn clean_path(raw: &str) -> String {
    raw.trim_start_matches("./").to_string()
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ManifestFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub clientonly: bool,
    #[serde(default)]
    pub serveronly: bool,
}

pub struct Ftb;

impl Ftb {
    /// Search for packs. The API returns ids only, so each hit is resolved to
    /// its pack details (bounded to keep requests reasonable).
    pub fn search(&self, query: &str) -> Result<Vec<Pack>, String> {
        let url = format!("{BASE}/modpack/search/8?term={}", urlencode(query));
        let body = crate::net::get_string(&url)?;
        let ids: SearchIds = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for id in ids.packs.into_iter().take(10) {
            if let Ok(p) = self.pack_info(id) {
                out.push(p);
            }
        }
        Ok(out)
    }

    /// Pack details (name, versions, art).
    pub fn pack_info(&self, id: u64) -> Result<Pack, String> {
        let body = crate::net::get_string(&format!("{BASE}/modpack/{id}"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Full manifest for a version, or the pack's newest release when `version`
    /// is `None`.
    pub fn manifest(&self, id: u64, version: Option<u64>) -> Result<PackManifest, String> {
        let version = match version {
            Some(v) => v,
            None => self
                .pack_info(id)?
                .latest_version()
                .map(|v| v.id)
                .ok_or("pack has no versions")?,
        };
        let body = crate::net::get_string(&format!("{BASE}/modpack/{id}/{version}"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Download every client file in a manifest into `game_dir`, verifying
    /// sha1 when provided. Returns the number of files written.
    pub fn install_files(
        &self,
        manifest: &PackManifest,
        game_dir: &std::path::Path,
    ) -> Result<usize, String> {
        let mut count = 0;
        for f in &manifest.files {
            if f.url.is_empty() || f.serveronly {
                continue;
            }
            let rel = clean_path(if f.path.is_empty() { &f.name } else { &f.path });
            if rel.is_empty() {
                continue;
            }
            let dest = crate::fs::safe_join(game_dir, &rel)
                .ok_or_else(|| format!("unsafe path in FTB manifest: {rel}"))?;
            let sha = if f.sha1.is_empty() {
                None
            } else {
                Some(f.sha1.as_str())
            };
            crate::net::download_verified(&f.url, &dest, sha)?;
            count += 1;
        }
        Ok(count)
    }

    /// Install a pack version into `game_dir` and return the manifest.
    pub fn install(
        &self,
        id: u64,
        version: Option<u64>,
        game_dir: &std::path::Path,
    ) -> Result<PackManifest, String> {
        let manifest = self.manifest(id, version)?;
        std::fs::create_dir_all(game_dir).map_err(|e| e.to_string())?;
        self.install_files(&manifest, game_dir)?;
        Ok(manifest)
    }
}

#[derive(Deserialize)]
struct SearchIds {
    #[serde(default)]
    packs: Vec<u64>,
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pack_info() {
        let p: Pack = serde_json::from_str(
            r#"{"id":14,"name":"FTB Presents Direwolf20","synopsis":"s","installs":10,
                "versions":[{"id":104,"name":"1.10.0","type":"release",
                  "targets":[{"name":"minecraft","version":"1.7.10","type":"game"}]}]}"#,
        )
        .unwrap();
        assert_eq!(p.id, 14);
        assert_eq!(p.latest_version().unwrap().id, 104);
    }

    #[test]
    fn manifest_components() {
        let m: PackManifest = serde_json::from_str(
            r#"{"name":"x","version":"1","files":[],
                "targets":[{"name":"minecraft","version":"1.20.1"},
                           {"name":"forge","version":"47.2.0"},
                           {"name":"java","version":"17"}]}"#,
        )
        .unwrap();
        assert_eq!(m.minecraft_version(), Some("1.20.1"));
        let c = m.components();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].0, "net.minecraft");
        assert_eq!(c[1].0, "net.minecraftforge");
    }
}
