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
    #[serde(default)]
    project_type: String,
}

pub struct Modrinth;

/// A search hit enriched with its project type.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    pub project_type: String,
}

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
    crate::net::agent()
        .get(url)
        .call()
        .map_err(|_| ())?
        .into_string()
        .map_err(|_| ())
}

#[derive(Deserialize)]
pub struct VersionInfo {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub project_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<VersionDependency>,
}

#[derive(Deserialize)]
pub struct VersionDependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default, rename = "dependency_type")]
    pub dependency_type: String,
}

#[derive(Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub hashes: Hashes,
}

#[derive(Deserialize, Default)]
pub struct Hashes {
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub sha512: String,
}

impl VersionInfo {
    /// The primary file, falling back to the first listed.
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

impl Modrinth {
    /// Fetch a version by id.
    pub fn version_by_id(&self, id: &str) -> Result<VersionInfo, String> {
        let body = crate::net::get_string(&format!("https://api.modrinth.com/v2/version/{id}"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }
}

impl Modrinth {
    /// Search with a project-type facet (mod, modpack, resourcepack, shader, datapack).
    pub fn search_typed(
        &self,
        query: &str,
        project_type: Option<&str>,
        loader: Option<&str>,
        game_version: Option<&str>,
    ) -> Result<Vec<SearchHit>, String> {
        let mut facets: Vec<String> = Vec::new();
        if let Some(t) = project_type {
            facets.push(format!("[\"project_type:{t}\"]"));
        }
        if let Some(l) = loader {
            facets.push(format!("[\"categories:{l}\"]"));
        }
        if let Some(g) = game_version {
            facets.push(format!("[\"versions:{g}\"]"));
        }
        let mut url = format!(
            "https://api.modrinth.com/v2/search?query={}&limit=20",
            urlencoding(query)
        );
        if !facets.is_empty() {
            url.push_str(&format!("&facets=[{}]", facets.join(",")));
        }
        let body = crate::net::get_string(&url)?;
        let parsed: SearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(parsed
            .hits
            .into_iter()
            .map(|h| SearchHit {
                id: h.project_id,
                slug: h.slug,
                title: h.title,
                description: h.description,
                downloads: h.downloads,
                project_type: h.project_type,
            })
            .collect())
    }

    /// Look up a project + version from a local file's sha1 hash.
    pub fn version_from_hash(&self, sha1: &str) -> Result<VersionInfo, String> {
        let url = format!("https://api.modrinth.com/v2/version_file/{sha1}?algorithm=sha1");
        let body = crate::net::get_string(&url)?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

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
        let body = crate::net::get_string(&url)?;
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
        let sha = if file.hashes.sha1.is_empty() {
            None
        } else {
            Some(file.hashes.sha1.as_str())
        };
        crate::net::download_verified(&file.url, &dest, sha)?;
        Ok(dest)
    }
}
