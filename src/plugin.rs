//! Plugin host. Mirrors launcher/plugin (content sources, custom indexes) and
//! adds an optional native-extension ABI.
//!
//! Two kinds of plugin are supported:
//!
//! * **Manifest plugins** — a directory under `<data_root>/plugins/<name>/`
//!   containing `plugin.toml`. These declare extra content sources (community
//!   APIs described by URL templates) and extra component indexes (custom
//!   Maven repositories for loader forks), so new sources can be added without
//!   recompiling the launcher.
//! * **Native plugins** — a shared library exporting the C ABI below. The
//!   launcher calls `prism_plugin_api_version` and, when it matches, calls
//!   `prism_plugin_init` with the data root.
//!
//! ```c
//! uint32_t prism_plugin_api_version(void);
//! int32_t  prism_plugin_init(const char *data_root_utf8);
//! ```

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// API version a native plugin must report to be loaded.
pub const PLUGIN_API_VERSION: u32 = 1;

/// A plugin manifest (`plugin.toml`).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct PluginManifest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Plugin API version the manifest targets.
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    #[serde(default)]
    pub source: Vec<ContentSource>,
    #[serde(default)]
    pub component_index: Vec<ComponentIndex>,
}

fn default_api_version() -> u32 {
    PLUGIN_API_VERSION
}

/// A content source described purely by URL templates.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ContentSource {
    pub name: String,
    /// Search endpoint. `{query}` is percent-encoded and substituted.
    pub search_url: String,
    /// JSON pointer to the array of results (dotted path, e.g. `data.hits`).
    #[serde(default)]
    pub results_path: String,
    /// Field names inside each result object.
    #[serde(default = "default_id_field")]
    pub id_field: String,
    #[serde(default = "default_title_field")]
    pub title_field: String,
    #[serde(default)]
    pub description_field: String,
    #[serde(default)]
    pub downloads_field: String,
    /// Download template used when installing; `{id}` is substituted.
    #[serde(default)]
    pub download_url: String,
}

fn default_id_field() -> String {
    "id".to_string()
}

fn default_title_field() -> String {
    "title".to_string()
}

/// A custom component (loader) index, e.g. a fork's Maven repository.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ComponentIndex {
    /// Component uid this index provides, e.g. `net.minecraftforge`.
    pub uid: String,
    /// Version listing URL (a Maven `maven-metadata.xml` or JSON array).
    #[serde(default)]
    pub versions_url: String,
}

/// A `plugin.toml` plus the directory it was found in.
#[derive(Debug, Clone)]
pub struct ManifestPlugin {
    pub dir: PathBuf,
    pub manifest: PluginManifest,
}

/// A loaded native plugin.
pub struct NativePlugin {
    pub path: PathBuf,
    pub api_version: u32,
    #[allow(dead_code)]
    library: libloading::Library,
}

impl std::fmt::Debug for NativePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePlugin")
            .field("path", &self.path)
            .field("api_version", &self.api_version)
            .finish()
    }
}

impl NativePlugin {
    /// Load a shared library and verify its API version.
    pub fn load(path: &Path) -> Result<NativePlugin, String> {
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|e| format!("cannot load {}: {e}", path.display()))?;
        let api_version: u32 = unsafe {
            let f: libloading::Symbol<extern "C" fn() -> u32> = library
                .get(b"prism_plugin_api_version")
                .map_err(|e| format!("missing prism_plugin_api_version: {e}"))?;
            f()
        };
        if api_version != PLUGIN_API_VERSION {
            return Err(format!(
                "{} targets plugin API {api_version}, launcher provides {PLUGIN_API_VERSION}",
                path.display()
            ));
        }
        Ok(NativePlugin {
            path: path.to_path_buf(),
            api_version,
            library,
        })
    }

    /// Call the plugin's init entry point with the launcher data root.
    pub fn init(&self, data_root: &Path) -> Result<(), String> {
        let c_path = std::ffi::CString::new(data_root.to_string_lossy().as_bytes())
            .map_err(|e| e.to_string())?;
        unsafe {
            let f: libloading::Symbol<extern "C" fn(*const std::ffi::c_char) -> i32> = self
                .library
                .get(b"prism_plugin_init")
                .map_err(|e| format!("missing prism_plugin_init: {e}"))?;
            let rc = f(c_path.as_ptr());
            if rc != 0 {
                return Err(format!("{} init returned {rc}", self.path.display()));
            }
        }
        Ok(())
    }
}

/// All plugins discovered under a data root.
#[derive(Debug, Default)]
pub struct PluginHost {
    pub manifests: Vec<ManifestPlugin>,
    pub natives: Vec<NativePlugin>,
    pub warnings: Vec<String>,
}

/// Directory scanned for plugins.
pub fn plugins_dir(data_root: &Path) -> PathBuf {
    data_root.join("plugins")
}

impl PluginHost {
    /// Discover plugins; never fails — problems are recorded as warnings.
    pub fn discover(data_root: &Path) -> PluginHost {
        let mut host = PluginHost::default();
        let root = plugins_dir(data_root);
        let Ok(entries) = std::fs::read_dir(&root) else {
            return host;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("plugin.toml");
                match std::fs::read_to_string(&manifest_path) {
                    Ok(text) => match toml::from_str::<PluginManifest>(&text) {
                        Ok(manifest) => {
                            if manifest.api_version != PLUGIN_API_VERSION {
                                host.warnings.push(format!(
                                    "{} targets API {}, launcher provides {PLUGIN_API_VERSION}",
                                    path.display(),
                                    manifest.api_version
                                ));
                                continue;
                            }
                            host.manifests.push(ManifestPlugin {
                                dir: path,
                                manifest,
                            });
                        }
                        Err(e) => host
                            .warnings
                            .push(format!("{}: {e}", manifest_path.display())),
                    },
                    Err(_) => {} // directory without a manifest: ignore
                }
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if matches!(ext, "so" | "dll" | "dylib") {
                    match NativePlugin::load(&path) {
                        Ok(p) => host.natives.push(p),
                        Err(e) => host.warnings.push(e),
                    }
                }
            }
        }
        host.manifests.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        host
    }

    /// Every declared content source across all manifest plugins.
    pub fn sources(&self) -> Vec<(&str, &ContentSource)> {
        self.manifests
            .iter()
            .flat_map(|p| p.manifest.source.iter().map(|s| (p.manifest.name.as_str(), s)))
            .collect()
    }

    /// Every declared component index.
    pub fn component_indexes(&self) -> Vec<&ComponentIndex> {
        self.manifests
            .iter()
            .flat_map(|p| p.manifest.component_index.iter())
            .collect()
    }

    /// Call init on every native plugin.
    pub fn init_natives(&self, data_root: &Path) -> Vec<String> {
        let mut errors = Vec::new();
        for p in &self.natives {
            if let Err(e) = p.init(data_root) {
                errors.push(e);
            }
        }
        errors
    }

    pub fn print(&self) {
        if self.manifests.is_empty() && self.natives.is_empty() {
            println!(
                "(no plugins in {})",
                plugins_dir(Path::new("")).display()
            );
        }
        for p in &self.manifests {
            println!(
                "{:<24} {:<10} api={} {}",
                p.manifest.name, p.manifest.version, p.manifest.api_version, p.manifest.description
            );
            for s in &p.manifest.source {
                println!("    source: {}", s.name);
            }
            for c in &p.manifest.component_index {
                println!("    component index: {}", c.uid);
            }
        }
        for p in &self.natives {
            println!("{:<24} native (api {})", p.path.display(), p.api_version);
        }
        for w in &self.warnings {
            println!("warning: {w}");
        }
    }
}

/// Percent-encode a query string for URL templates.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Expand a URL template's `{query}` / `{id}` placeholders.
pub fn expand(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), &encode(v));
    }
    out
}

/// A generic search hit from a manifest plugin source.
#[derive(Debug, Clone)]
pub struct SourceHit {
    pub id: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
}

/// Search a manifest-declared content source.
pub fn search_source(source: &ContentSource, query: &str) -> Result<Vec<SourceHit>, String> {
    let url = expand(&source.search_url, &[("query", query)]);
    let body = crate::net::get_string(&url)?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let node = if source.results_path.is_empty() {
        &json
    } else {
        let mut cur = &json;
        for part in source.results_path.split('.') {
            cur = cur
                .get(part)
                .ok_or_else(|| format!("results path '{}' not found", source.results_path))?;
        }
        cur
    };
    let arr = node
        .as_array()
        .ok_or_else(|| format!("results path '{}' is not an array", source.results_path))?;
    let get_str = |obj: &serde_json::Value, field: &str| -> String {
        if field.is_empty() {
            return String::new();
        }
        match obj.get(field) {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(v) if v.is_number() => v.to_string(),
            _ => String::new(),
        }
    };
    Ok(arr
        .iter()
        .map(|item| SourceHit {
            id: get_str(item, &source.id_field),
            title: get_str(item, &source.title_field),
            description: get_str(item, &source.description_field),
            downloads: item
                .get(&source.downloads_field)
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_reports_sources() {
        let td = tempfile::tempdir().unwrap();
        let dir = plugins_dir(td.path()).join("community");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.toml"),
            r#"
name = "Community"
version = "1.0.0"
description = "Extra sources"
api-version = 1

[[source]]
name = "Example"
search-url = "https://api.example.com/search?q={query}"
results-path = "data.hits"
id-field = "project_id"
title-field = "title"
description-field = "description"
downloads-field = "downloads"

[[component-index]]
uid = "net.minecraftforge"
versions-url = "https://maven.example.com/forge/maven-metadata.xml"
"#,
        )
        .unwrap();

        let host = PluginHost::discover(td.path());
        assert_eq!(host.manifests.len(), 1);
        assert_eq!(host.manifests[0].manifest.name, "Community");
        assert_eq!(host.sources().len(), 1);
        assert_eq!(host.component_indexes().len(), 1);
        assert!(host.warnings.is_empty());
    }

    #[test]
    fn rejects_wrong_api_version() {
        let td = tempfile::tempdir().unwrap();
        let dir = plugins_dir(td.path()).join("old");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.toml"),
            "name=\"Old\"\napi-version = 99\n",
        )
        .unwrap();
        let host = PluginHost::discover(td.path());
        assert!(host.manifests.is_empty());
        assert_eq!(host.warnings.len(), 1);
    }

    #[test]
    fn expands_templates() {
        let url = expand(
            "https://api.example.com/search?q={query}&type=mod",
            &[("query", "hello world/a")],
        );
        assert_eq!(
            url,
            "https://api.example.com/search?q=hello%20world%2Fa&type=mod"
        );
    }

    #[test]
    fn parses_source_results() {
        let source = ContentSource {
            name: "Example".into(),
            search_url: "https://e".into(),
            results_path: "data.hits".into(),
            id_field: "project_id".into(),
            title_field: "title".into(),
            description_field: "description".into(),
            downloads_field: "downloads".into(),
            download_url: String::new(),
        };
        let json = r#"{"data":{"hits":[
            {"project_id":"abc","title":"Foo","description":"d","downloads":12}
        ]}}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        // Mirror the lookup logic without network.
        let mut cur = &value;
        for part in source.results_path.split('.') {
            cur = cur.get(part).unwrap();
        }
        let arr = cur.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].get(&source.id_field).unwrap(), "abc");
    }

    #[test]
    fn native_plugin_load_fails_cleanly() {
        let td = tempfile::tempdir().unwrap();
        let fake = td.path().join("not-a-library.so");
        std::fs::write(&fake, b"nope").unwrap();
        assert!(NativePlugin::load(&fake).is_err());
    }
}
