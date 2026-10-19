//! CurseForge API client. Mirrors launcher/modplatform/flame.
//! Requires CF_API_KEY (env) for the API; downloads use the CDN.

use crate::modplatform::ModHit;
use serde::Deserialize;
use std::sync::OnceLock;

const BASE: &str = "https://api.curseforge.com/v1";
pub const GAME_MINECRAFT: u32 = 432;

static API_KEY: OnceLock<Option<String>> = OnceLock::new();

/// Install the CurseForge API key from settings (or `CF_API_KEY`). Only the
/// first call takes effect.
pub fn set_api_key(key: Option<String>) {
    let _ = API_KEY.set(key);
}

fn api_key() -> Result<String, String> {
    if let Some(Some(k)) = API_KEY.get() {
        return Ok(k.clone());
    }
    std::env::var("CF_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "CurseForge API key not set (Settings → APIs, or CF_API_KEY)".to_string())
}

/// Whether a CurseForge API key is available.
pub fn has_api_key() -> bool {
    api_key().is_ok()
}

// ---- Search (shared with the generic `ModProvider` entry point) ----

/// Mod-loader type ids used by the CurseForge API.
pub const LOADER_FORGE: u32 = 1;
pub const LOADER_FABRIC: u32 = 4;
pub const LOADER_QUILT: u32 = 5;
pub const LOADER_NEOFORGE: u32 = 6;

/// Map a loader name to its CurseForge `modLoaderType`.
pub fn loader_type(name: &str) -> Option<u32> {
    match name.to_ascii_lowercase().as_str() {
        "forge" => Some(LOADER_FORGE),
        "fabric" => Some(LOADER_FABRIC),
        "quilt" => Some(LOADER_QUILT),
        "neoforge" => Some(LOADER_NEOFORGE),
        _ => None,
    }
}

/// CurseForge class ids: 6 = mods, 4471 = modpacks, 12 = resource packs,
/// 6552 = shaders.
pub const CLASS_MOD: u32 = 6;
pub const CLASS_MODPACK: u32 = 4471;
pub const CLASS_RESOURCEPACK: u32 = 12;
pub const CLASS_SHADER: u32 = 6552;

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<Mod>,
}

#[derive(Deserialize)]
struct Mod {
    id: u64,
    slug: String,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: u64,
}

#[derive(Deserialize)]
struct FilesResponse {
    data: Vec<CfFile>,
}

#[derive(Deserialize, Clone)]
pub struct CfFile {
    pub id: u64,
    #[serde(default, rename = "modId")]
    pub mod_id: u64,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub file_date: String,
    #[serde(default, rename = "gameVersions")]
    pub game_versions: Vec<String>,
}

impl CfFile {
    /// Best-effort download URL: the API-provided one when present, otherwise
    /// the public CDN layout (`mediafilez.forgecdn.net/files/<id/1000>/<id%1000>/<name>`).
    pub fn resolved_url(&self) -> Option<String> {
        if let Some(u) = &self.download_url {
            if !u.is_empty() {
                return Some(u.clone());
            }
        }
        if self.file_name.is_empty() {
            return None;
        }
        Some(format!(
            "https://mediafilez.forgecdn.net/files/{}/{}/{}",
            self.id / 1000,
            self.id % 1000,
            self.file_name
        ))
    }
}

pub struct CurseForge;

impl CurseForge {
    fn get(&self, url: &str) -> Result<String, String> {
        let key = api_key()?;
        crate::net::agent()
            .get(url)
            .set("x-api-key", &key)
            .set("Accept", "application/json")
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())
    }

    pub fn search_paged(&self, query: &str, index: u32) -> Result<Vec<ModHit>, String> {
        let url = format!(
            "{BASE}/mods/search?gameId={GAME_MINECRAFT}&searchFilter={}&index={index}&pageSize=20&sortField=2",
            urlencode(query)
        );
        let body = self.get(&url)?;
        let resp: SearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp
            .data
            .into_iter()
            .map(|m| ModHit {
                id: m.id.to_string(),
                slug: m.slug,
                title: m.name,
                description: m.summary,
                downloads: m.download_count,
            })
            .collect())
    }

    /// Latest files for a mod, filtered by game version / loader.
    pub fn files(&self, mod_id: &str, game_version: &str, loader_type: u32) -> Result<Vec<CfFile>, String> {
        let url = format!(
            "{BASE}/mods/{mod_id}/files?gameVersion={game_version}&modLoaderType={loader_type}&pageSize=20"
        );
        let body = self.get(&url)?;
        let resp: FilesResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp.data)
    }

    /// Download a file into `dest_dir`; returns the path.
    pub fn download_file(
        &self,
        mod_id: &str,
        file_id: u64,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let url = format!("{BASE}/mods/{mod_id}/files/{file_id}/download-url");
        match self.get(&url) {
            Ok(body) => {
                #[derive(Deserialize)]
                struct R {
                    data: String,
                }
                let r: R = serde_json::from_str(&body).map_err(|e| e.to_string())?;
                std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
                let filename = r
                    .data
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("mod.jar")
                    .to_string();
                let dest = dest_dir.join(filename);
                crate::net::download_verified(&r.data, &dest, None)?;
                Ok(dest)
            }
            // Some API keys cannot use the download-url endpoint (or a mod
            // opts out of the API). Fall back to file metadata + the public
            // CDN layout.
            Err(api_err) => {
                let file = self.file(mod_id, file_id)?;
                let url = file
                    .resolved_url()
                    .ok_or_else(|| format!("{api_err}; file has no public download"))?;
                std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
                let name = if file.file_name.is_empty() {
                    format!("cf-{file_id}.jar")
                } else {
                    file.file_name.clone()
                };
                let dest = dest_dir.join(name);
                crate::net::download_verified(&url, &dest, None)?;
                Ok(dest)
            }
        }
    }

    /// Metadata for one file of a mod.
    pub fn file(&self, mod_id: &str, file_id: u64) -> Result<CfFile, String> {
        let body = self.get(&format!("{BASE}/mods/{mod_id}/files/{file_id}"))?;
        #[derive(Deserialize)]
        struct R {
            data: CfFile,
        }
        let r: R = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(r.data)
    }

    /// The latest file for a project, with no version filtering.
    pub fn latest_any(&self, mod_id: &str) -> Result<Option<CfFile>, String> {
        self.latest_file(mod_id, None, None)
    }
}

impl CurseForge {
    /// Search restricted to a CurseForge class id (mods/modpacks/packs/shaders)
    /// with optional game-version and loader filters.
    pub fn search_filtered(
        &self,
        query: &str,
        class_id: u32,
        game_version: Option<&str>,
        loader: Option<u32>,
        index: u32,
    ) -> Result<Vec<ModHit>, String> {
        let mut url = format!(
            "{BASE}/mods/search?gameId={GAME_MINECRAFT}&classId={class_id}&searchFilter={}&index={index}&pageSize=20&sortField=2",
            urlencode(query)
        );
        if let Some(g) = game_version {
            url.push_str(&format!("&gameVersion={}", urlencode(g)));
        }
        if let Some(l) = loader {
            url.push_str(&format!("&modLoaderType={l}"));
        }
        let body = self.get(&url)?;
        let resp: SearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp
            .data
            .into_iter()
            .map(|m| ModHit {
                id: m.id.to_string(),
                slug: m.slug,
                title: m.name,
                description: m.summary,
                downloads: m.download_count,
            })
            .collect())
    }

    /// The newest file for a mod matching the given filters.
    pub fn latest_file(
        &self,
        mod_id: &str,
        game_version: Option<&str>,
        loader: Option<u32>,
    ) -> Result<Option<CfFile>, String> {
        let mut url = format!("{BASE}/mods/{mod_id}/files?pageSize=20");
        if let Some(g) = game_version {
            url.push_str(&format!("&gameVersion={}", urlencode(g)));
        }
        if let Some(l) = loader {
            url.push_str(&format!("&modLoaderType={l}"));
        }
        let body = self.get(&url)?;
        let resp: FilesResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp.data.into_iter().next())
    }

    /// Download a specific file into `dest_dir`.
    pub fn install_file(&self, file: &CfFile, dest_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
        let url = file
            .resolved_url()
            .ok_or_else(|| format!("file {} has no download URL", file.display_name))?;
        std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
        let name = if file.file_name.is_empty() {
            format!("curseforge-{}.jar", file.id)
        } else {
            file.file_name.clone()
        };
        let dest = dest_dir.join(name);
        crate::net::download_verified(&url, &dest, None)?;
        Ok(dest)
    }

    /// Download the latest matching file of a mod into `dest_dir`.
    pub fn install_mod(
        &self,
        mod_id: &str,
        game_version: Option<&str>,
        loader: Option<u32>,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let file = self
            .latest_file(mod_id, game_version, loader)?
            .ok_or("no matching files for this mod")?;
        self.install_file(&file, dest_dir)
    }
}

impl crate::modplatform::ModProvider for CurseForge {
    fn name(&self) -> &'static str {
        "CurseForge"
    }
    fn search(&self, query: &str) -> Vec<ModHit> {
        self.search_paged(query, 0).unwrap_or_default()
    }
}

fn urlencode(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}
