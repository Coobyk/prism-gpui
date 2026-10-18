//! Instance discovery and parsing for Prism Launcher's on-disk format.
//!
//! Mirrors: launcher/BaseInstance, launcher/MinecraftInstance, launcher/InstanceList.
//! Prism stores instances under `<data>/instances/<name>/instance.cfg` (INI).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub name: String,
    pub dir: PathBuf,
    pub instance_type: String, // OneSix, Legacy, etc.
    pub icon_key: String,
    pub java_path: String,
    pub min_mem: u32,
    pub max_mem: u32,
    pub settings: BTreeMap<String, String>,
}

impl Instance {
    pub fn load(dir: &Path) -> Option<Self> {
        let cfg_path = dir.join("instance.cfg");
        let text = fs::read_to_string(&cfg_path).ok()?;
        let settings = parse_ini(&text);
        let name = dir
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Some(Instance {
            name,
            dir: dir.to_path_buf(),
            instance_type: settings
                .get("InstanceType")
                .cloned()
                .unwrap_or_default(),
            icon_key: settings.get("IconKey").cloned().unwrap_or_default(),
            java_path: settings.get("JavaPath").cloned().unwrap_or_default(),
            min_mem: settings
                .get("MinMemAlloc")
                .and_then(|v| v.parse().ok())
                .unwrap_or(512),
            max_mem: settings
                .get("MaxMemAlloc")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4096),
            settings,
        })
    }
}

/// Discover instances under `<data_root>/instances`.
pub fn discover_instances(data_root: &Path) -> Vec<Instance> {
    let instances_dir = data_root.join("instances");
    let mut out = Vec::new();
    let entries = match fs::read_dir(&instances_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("instance.cfg").is_file() {
            if let Some(inst) = Instance::load(&path) {
                out.push(inst);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Minimal INI parser: reads `[General]` section (and any) into flat key=value.
fn parse_ini(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('[') || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_sample() {
        let t = "[General]\nInstanceType=OneSix\nMinMemAlloc=512\nMaxMemAlloc=8192\n";
        let m = parse_ini(t);
        assert_eq!(m.get("InstanceType").unwrap(), "OneSix");
        assert_eq!(m.get("MinMemAlloc").unwrap(), "512");
    }

    #[test]
    fn discover_temp() {
        use tempfile::tempdir;
        let td = tempdir().unwrap();
        let root = td.path();
        let inst = root.join("instances").join("TestInst");
        std::fs::create_dir_all(&inst).unwrap();
        std::fs::write(
            inst.join("instance.cfg"),
            "[General]\nInstanceType=OneSix\nMinMemAlloc=256\nMaxMemAlloc=1024\n",
        )
        .unwrap();
        let found = discover_instances(root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "TestInst");
        assert_eq!(found[0].min_mem, 256);
        assert_eq!(found[0].max_mem, 1024);
    }
}
