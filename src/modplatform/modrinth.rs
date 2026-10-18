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
    // Use std::process? Simpler: shell out to curl if available.
    let out = std::process::Command::new("curl")
        .arg("-sS")
        .arg(url)
        .output()
        .map_err(|_| ())?;
    if !out.status.success() {
        return Err(());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}
