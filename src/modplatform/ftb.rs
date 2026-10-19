//! FTB modpack API client. Mirrors launcher/modplatform/ftb.

use serde::Deserialize;

const BASE: &str = "https://api.modpacks.ch/public";

#[derive(Deserialize, Debug)]
pub struct PackVersion {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type", default)]
    pub release_type: String,
}

#[derive(Deserialize, Debug)]
pub struct Pack {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub versions: Vec<PackVersion>,
}

#[derive(Deserialize, Debug)]
pub struct PackManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub targets: Vec<Target>,
}

#[derive(Deserialize, Debug)]
pub struct Target {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub type_: String,
}

#[derive(Deserialize, Debug)]
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
}

pub struct Ftb;

impl Ftb {
    pub fn search(&self, query: &str) -> Result<Vec<Pack>, String> {
        let url = format!("{BASE}/modpack/search/8?term={}", urlencode(query));
        let body = crate::net::get_string(&url)?;
        serde_json::from_str::<SearchResponse>(&body)
            .map(|r| r.packs)
            .map_err(|e| e.to_string())
    }

    pub fn pack(&self, id: u64) -> Result<PackManifest, String> {
        let body = crate::net::get_string(&format!("{BASE}/modpack/{id}"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    pub fn version(&self, id: u64, version: u64) -> Result<PackManifest, String> {
        let body = crate::net::get_string(&format!("{BASE}/modpack/{id}/{version}"))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    packs: Vec<Pack>,
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
