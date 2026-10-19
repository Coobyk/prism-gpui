//! CurseForge API client. Mirrors launcher/modplatform/flame.
//! Requires CF_API_KEY (env) for the API; downloads use the CDN.

use crate::modplatform::ModHit;
use serde::Deserialize;

const BASE: &str = "https://api.curseforge.com/v1";
const GAME_MINECRAFT: u32 = 432;

fn api_key() -> Result<String, String> {
    std::env::var("CF_API_KEY").map_err(|_| "CF_API_KEY not set".to_string())
}

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

#[derive(Deserialize)]
struct CfFile {
    id: u64,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    download_url: Option<String>,
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
        let body = self.get(&url)?;
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
