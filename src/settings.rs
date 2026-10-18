//! Global Prism settings (prismlauncher.cfg) parser.
//! Mirrors: launcher/settings/INISettingsObject, SettingsObject.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct GlobalSettings {
    pub data_root: PathBuf,
    pub theme: String,
    pub instance_dir: String,
    pub raw: BTreeMap<String, String>,
}

impl GlobalSettings {
    pub fn load(data_root: &Path) -> Self {
        let cfg_path = data_root.join("prismlauncher.cfg");
        let text = fs::read_to_string(&cfg_path).unwrap_or_default();
        let mut raw = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                raw.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        GlobalSettings {
            data_root: data_root.to_path_buf(),
            theme: raw.get("ApplicationTheme").cloned().unwrap_or_default(),
            instance_dir: raw
                .get("InstanceDir")
                .cloned()
                .unwrap_or_else(|| "instances".to_string()),
            raw,
        }
    }
}
