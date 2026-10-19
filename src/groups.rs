//! Instance groups. Mirrors launcher/InstanceList group handling
//! (`instgroups.json`).

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub const GROUPS_FILE: &str = "instgroups.json";

#[derive(Debug, Clone, Default)]
pub struct Group {
    pub name: String,
    pub hidden: bool,
    /// Instance *ids* (directory names) in this group.
    pub instances: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GroupStore {
    pub groups: BTreeMap<String, Group>,
    pub ungrouped_hidden: bool,
}

impl GroupStore {
    pub fn load(data_root: &Path) -> Self {
        let path = data_root.join(GROUPS_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Self::default();
        };
        let mut groups = BTreeMap::new();
        if let Some(obj) = value.get("groups").and_then(|g| g.as_object()) {
            for (name, g) in obj {
                groups.insert(
                    name.clone(),
                    Group {
                        name: name.clone(),
                        hidden: g.get("hidden").and_then(|h| h.as_bool()).unwrap_or(false),
                        instances: g
                            .get("instances")
                            .and_then(|i| i.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    },
                );
            }
        }
        let ungrouped_hidden = value
            .get("ungrouped")
            .and_then(|u| u.get("hidden"))
            .and_then(|h| h.as_bool())
            .unwrap_or(false);
        Self {
            groups,
            ungrouped_hidden,
        }
    }

    pub fn save(&self, data_root: &Path) -> Result<(), String> {
        let mut groups = serde_json::Map::new();
        for (name, g) in &self.groups {
            groups.insert(
                name.clone(),
                serde_json::json!({
                    "hidden": g.hidden,
                    "instances": g.instances,
                }),
            );
        }
        let value = serde_json::json!({
            "formatVersion": "1",
            "groups": groups,
            "ungrouped": { "hidden": self.ungrouped_hidden },
        });
        let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
        std::fs::write(data_root.join(GROUPS_FILE), text).map_err(|e| e.to_string())
    }

    /// Which group an instance id belongs to (empty = ungrouped).
    pub fn group_of(&self, instance_id: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|(_, g)| g.instances.iter().any(|i| i == instance_id))
            .map(|(name, _)| name.as_str())
    }

    /// Assign an instance to a group (removing it from any previous group).
    pub fn set_group(&mut self, instance_id: &str, group: Option<&str>) {
        for g in self.groups.values_mut() {
            g.instances.retain(|i| i != instance_id);
        }
        if let Some(name) = group {
            let entry = self
                .groups
                .entry(name.to_string())
                .or_insert_with(|| Group {
                    name: name.to_string(),
                    ..Default::default()
                });
            entry.instances.push(instance_id.to_string());
        }
        self.groups.retain(|_, g| !g.instances.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn set_group_moves_instances() {
        let td = tempfile::tempdir().unwrap();
        let mut store = GroupStore::default();
        store.set_group("A", Some("modded"));
        store.set_group("B", Some("modded"));
        store.set_group("A", Some("vanilla"));
        store.save(td.path()).unwrap();

        let loaded = GroupStore::load(td.path());
        assert_eq!(loaded.group_of("A"), Some("vanilla"));
        assert_eq!(loaded.group_of("B"), Some("modded"));
        assert_eq!(loaded.group_of("C"), None);

        let mut loaded = loaded;
        loaded.set_group("B", None);
        assert_eq!(loaded.group_of("B"), None);
    }
}
