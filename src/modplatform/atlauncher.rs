//! ATLauncher API client. Mirrors launcher/modplatform/atlauncher.

use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct AtPack {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub safe_name: String,
    #[serde(default)]
    pub versions: Vec<AtVersion>,
}

#[derive(Deserialize, Debug)]
pub struct AtVersion {
    pub version: String,
    #[serde(default)]
    pub minecraft: String,
}

pub struct AtLauncher;

impl AtLauncher {
    pub fn search(&self) -> Result<Vec<AtPack>, String> {
        let body = crate::net::get_string("https://api.atlauncher.com/v1/packs")?;
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            data: Vec<AtPack>,
        }
        serde_json::from_str::<R>(&body)
            .map(|r| r.data)
            .map_err(|e| e.to_string())
    }
}
