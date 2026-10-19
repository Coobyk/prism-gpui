//! Translations. Mirrors launcher/translations (Qt .qm) with a JSON catalog.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Translator {
    lang: String,
    catalog: BTreeMap<String, String>,
}

impl Translator {
    /// Load `<data_root>/translations/<lang>.json` (flat key → value map).
    pub fn load(data_root: &Path, lang: &str) -> Self {
        let mut catalog = BTreeMap::new();
        let path: PathBuf = data_root.join("translations").join(format!("{lang}.json"));
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        catalog.insert(k, s.to_string());
                    }
                }
            }
        }
        Translator {
            lang: lang.to_string(),
            catalog,
        }
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// Translate a key, falling back to the key itself.
    pub fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.catalog.get(key).map(|s| s.as_str()).unwrap_or(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn falls_back_to_key() {
        let td = tempfile::tempdir().unwrap();
        let t = Translator::load(td.path(), "en");
        assert_eq!(t.tr("Launch"), "Launch");
    }
    #[test]
    fn loads_catalog() {
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(td.path().join("translations")).unwrap();
        std::fs::write(
            td.path().join("translations/de.json"),
            r#"{"Launch":"Starten"}"#,
        )
        .unwrap();
        let t = Translator::load(td.path(), "de");
        assert_eq!(t.tr("Launch"), "Starten");
    }
}
