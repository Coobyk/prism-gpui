//! Modrinth API v2 client (skeleton).

use crate::modplatform::ModHit;
use serde::Deserialize;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    downloads: u64,
}

pub struct Modrinth;

impl crate::modplatform::ModProvider for Modrinth {
    fn name(&self) -> &'static str {
        "Modrinth"
    }
    fn search(&self, query: &str) -> Vec<ModHit> {
        // Synchronous blocking fetch; replace with async in production.
        let url = format!(
            "https://api.modrinth.com/v2/search?query={}&limit=20",
            urlencoding(query)
        );
        let resp = match ureq_get(&url) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let parsed: SearchResponse = match serde_json::from_str(&resp) {
            Ok(p) => p,
            Err(_) => return vec![],
        };
        parsed
            .hits
            .into_iter()
            .map(|h| ModHit {
                id: h.project_id,
                slug: h.slug,
                title: h.title,
                description: h.description,
                downloads: h.downloads,
            })
            .collect()
    }
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

fn ureq_get(url: &str) -> Result<String, ()> {
    let resp = ureq::get(url)
        .set("User-Agent", "prism-gpui/0.1 (github.com/Coobyk/prism-gpui)")
        .call()
        .map_err(|_| ())?;
    resp.into_string().map_err(|_| ())
}

#[derive(Deserialize)]
pub struct VersionInfo {
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
}

#[derive(Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
}

impl Modrinth {
    /// List versions for a project, optionally filtered by loader and game version.
    pub fn versions(
        &self,
        slug: &str,
        loader: Option<&str>,
        game_version: Option<&str>,
    ) -> Result<Vec<VersionInfo>, String> {
        let mut url = format!("https://api.modrinth.com/v2/project/{slug}/version");
        let mut params = Vec::new();
        if let Some(l) = loader {
            params.push(format!("loaders=[\"{l}\"]"));
        }
        if let Some(g) = game_version {
            params.push(format!("game_versions=[\"{g}\"]"));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        let body = ureq::get(&url)
            .set("User-Agent", "prism-gpui/0.1 (github.com/Coobyk/prism-gpui)")
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Download a version's primary file into `dest_dir`; returns the written path.
    pub fn download_version_file(
        &self,
        version: &VersionInfo,
        dest_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        let file = version
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| version.files.first())
            .ok_or("version has no files")?;
        std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
        let dest = dest_dir.join(&file.filename);
        let resp = ureq::get(&file.url)
            .set("User-Agent", "prism-gpui/0.1 (github.com/Coobyk/prism-gpui)")
            .call()
            .map_err(|e| e.to_string())?;
        let mut reader = resp.into_reader();
        let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut reader, &mut out).map_err(|e| e.to_string())?;
        Ok(dest)
    }
}
