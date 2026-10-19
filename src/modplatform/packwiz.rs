//! Packwiz pack support (pack.toml + index.toml). Mirrors launcher/modplatform/packwiz.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Deserialize, Debug, Default)]
pub struct PackToml {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pack_format: Option<String>,
    #[serde(default)]
    pub versions: BTreeMap<String, String>,
}

#[derive(Deserialize, Debug, Default)]
pub struct IndexToml {
    #[serde(default)]
    pub files: Vec<IndexFile>,
}

#[derive(Deserialize, Debug, Default)]
pub struct IndexFile {
    pub file: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub hash_format: String,
    #[serde(default)]
    pub metafile: bool,
}

pub fn read_pack(dir: &Path) -> Result<PackToml, String> {
    let text = std::fs::read_to_string(dir.join("pack.toml")).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

pub fn read_index(dir: &Path) -> Result<IndexToml, String> {
    let text = std::fs::read_to_string(dir.join("index.toml")).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_pack_toml() {
        let text = r#"
name = "My Pack"
version = "1.0.0"
pack-format = "packwiz:1.1.0"

[versions]
minecraft = "1.20.1"
fabric = "0.15.0"
"#;
        let p: PackToml = toml::from_str(text).unwrap();
        assert_eq!(p.name, "My Pack");
        assert_eq!(p.versions.get("minecraft").unwrap(), "1.20.1");
    }
}
