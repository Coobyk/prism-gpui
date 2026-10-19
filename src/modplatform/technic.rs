//! Technic Platform API client. Mirrors launcher/modplatform/technic.

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct SolderPack {
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub recommended: String,
    #[serde(default)]
    pub latest: String,
}

pub struct Technic;

impl Technic {
    /// Search the Technic platform.
    pub fn search(&self, query: &str) -> Result<Vec<SolderPack>, String> {
        let url = format!("https://api.technicpack.net/search?q={}", urlencode(query));
        let body = crate::net::get_string(&url)?;
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            modpacks: Vec<SolderPack>,
        }
        serde_json::from_str::<R>(&body)
            .map(|r| r.modpacks)
            .map_err(|e| e.to_string())
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
