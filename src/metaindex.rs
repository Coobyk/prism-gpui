//! Prism metadata index client (`https://meta.prismlauncher.org/v1`).
//!
//! Mirrors launcher/meta/ (`MetaIndex`, `Version`, `VersionList`). This is the
//! authoritative source for loader components (`net.minecraftforge`,
//! `net.neoforged`, `net.fabricmc.fabric-loader`, `org.quiltmc.quilt-loader`,
//! …). Every component version JSON is a superset of the Mojang version format
//! and additionally carries `requires`/`conflicts`, `mavenFiles`, `+jvmArgs`,
//! `+gameArgs` and `+traits`.
//!
//! Indexes and version files are cached under `<data_root>/meta/<uid>/` so the
//! launcher keeps working offline after the first resolve.

use crate::meta::{Library, Requirement, VersionJson};
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub const META_BASE: &str = "https://meta.prismlauncher.org/v1";

/// One entry in a component's version list.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MetaVersion {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub release_time: String,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub sha256: String,
}

impl MetaVersion {
    /// Whether this version targets the given `net.minecraft` version.
    pub fn requires_minecraft(&self) -> Option<&str> {
        self.requires
            .iter()
            .find(|r| r.uid == "net.minecraft")
            .map(|r| r.equals.as_str())
            .filter(|s| !s.is_empty())
    }
}

/// A component's version list.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MetaIndex {
    #[serde(default, rename = "formatVersion")]
    pub format_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub versions: Vec<MetaVersion>,
}

impl MetaIndex {
    /// Versions that target `mc_version`, newest first (index order).
    pub fn for_minecraft(&self, mc_version: &str) -> Vec<&MetaVersion> {
        self.versions
            .iter()
            .filter(|v| v.requires_minecraft() == Some(mc_version))
            .collect()
    }

    /// The recommended version for `mc_version`, falling back to the newest.
    pub fn recommended_for(&self, mc_version: &str) -> Option<&MetaVersion> {
        let matching = self.for_minecraft(mc_version);
        matching
            .iter()
            .find(|v| v.recommended)
            .or_else(|| matching.first())
            .copied()
    }

    /// The newest version for `mc_version`.
    pub fn newest_for(&self, mc_version: &str) -> Option<&MetaVersion> {
        self.for_minecraft(mc_version).first().copied()
    }

    pub fn find(&self, version: &str) -> Option<&MetaVersion> {
        self.versions.iter().find(|v| v.version == version)
    }
}

/// Directory holding cached metadata for a uid.
pub fn meta_dir(data_root: &Path, uid: &str) -> PathBuf {
    data_root.join("meta").join(uid)
}

/// Whether a uid is served by the Prism meta index (i.e. not `net.minecraft`,
/// which comes from Mojang).
pub fn is_indexed_component(uid: &str) -> bool {
    uid != "net.minecraft"
}

/// Load a cached component index, if present.
pub fn cached_index(data_root: &Path, uid: &str) -> Option<MetaIndex> {
    let text = std::fs::read_to_string(meta_dir(data_root, uid).join("index.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Load a cached component version file, if present.
pub fn cached_component(data_root: &Path, uid: &str, version: &str) -> Option<VersionJson> {
    let path = meta_dir(data_root, uid).join(format!("{version}.json"));
    crate::meta::read_version_json(&path)
}

/// Fetch (and cache) a component index.
pub fn fetch_index(data_root: &Path, uid: &str) -> Result<MetaIndex, String> {
    let url = format!("{META_BASE}/{uid}/index.json");
    let body = match crate::net::get_string(&url) {
        Ok(b) => b,
        Err(e) => {
            // Fall back to the cache when the network is unavailable.
            if let Some(cached) = cached_index(data_root, uid) {
                return Ok(cached);
            }
            return Err(format!("could not fetch index for {uid}: {e}"));
        }
    };
    let index: MetaIndex =
        serde_json::from_str(&body).map_err(|e| format!("index for {uid} was malformed: {e}"))?;
    let dir = meta_dir(data_root, uid);
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(dir.join("index.json"), body.as_bytes());
    }
    Ok(index)
}

/// Fetch a component index, preferring the on-disk cache.
pub fn index(data_root: &Path, uid: &str) -> Result<MetaIndex, String> {
    if let Some(cached) = cached_index(data_root, uid) {
        return Ok(cached);
    }
    fetch_index(data_root, uid)
}

/// Fetch (and cache) one component version, along with all of its libraries.
pub fn ensure_component(data_root: &Path, uid: &str, version: &str) -> Result<VersionJson, String> {
    let path = meta_dir(data_root, uid).join(format!("{version}.json"));
    let component = if let Some(cached) = crate::meta::read_version_json(&path) {
        cached
    } else {
        let url = format!("{META_BASE}/{uid}/{version}.json");
        let body = crate::net::get_string(&url)?;
        let parsed: VersionJson = serde_json::from_str(&body)
            .map_err(|e| format!("component {uid}/{version} was malformed: {e}"))?;
        if let Some(dir) = path.parent()
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = std::fs::write(&path, body.as_bytes());
        }
        parsed
    };
    crate::meta::ensure_libraries(data_root, &component)?;
    Ok(component)
}

/// One resolved component in a dependency chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedComponent {
    pub uid: String,
    pub version: String,
}

/// Resolve `uid` for a Minecraft version, following its dependency chain.
///
/// Loader components often delegate the Minecraft version constraint to a
/// dependency (Fabric Loader → Fabric Intermediary → `net.minecraft`), so a
/// plain index scan is not enough. The returned vector is dependency-first and
/// deduplicated, ending with the requested component.
pub fn resolve_chain(
    data_root: &Path,
    uid: &str,
    mc_version: &str,
    prefer_recommended: bool,
) -> Result<Vec<ResolvedComponent>, String> {
    let mut visiting: Vec<String> = Vec::new();
    let mut chain: Vec<ResolvedComponent> = Vec::new();
    let version = resolve_inner(
        data_root,
        uid,
        mc_version,
        prefer_recommended,
        &mut visiting,
        &mut chain,
    )?;
    chain.push(ResolvedComponent {
        uid: uid.to_string(),
        version,
    });
    // Deduplicate, keeping the last occurrence of each uid.
    let mut out: Vec<ResolvedComponent> = Vec::new();
    for c in chain {
        match out.iter_mut().find(|x| x.uid == c.uid) {
            Some(existing) => existing.version = c.version,
            None => out.push(c),
        }
    }
    Ok(out)
}

fn resolve_inner(
    data_root: &Path,
    uid: &str,
    mc_version: &str,
    prefer_recommended: bool,
    visiting: &mut Vec<String>,
    chain: &mut Vec<ResolvedComponent>,
) -> Result<String, String> {
    if uid == "net.minecraft" {
        return Ok(mc_version.to_string());
    }
    // Already resolved in this run?
    if let Some(existing) = chain.iter().find(|c| c.uid == uid) {
        return Ok(existing.version.clone());
    }
    if visiting.iter().any(|v| v == uid) {
        return Err(format!("dependency cycle through {uid}"));
    }
    visiting.push(uid.to_string());

    let index = index(data_root, uid)?;
    let result = (|| -> Result<String, String> {
        // Candidates targeting this Minecraft version come first; then any
        // version that declares no Minecraft constraint (loader-style entries
        // that inherit it from a dependency).
        let mut ordered: Vec<&MetaVersion> = index.for_minecraft(mc_version);
        let direct: Vec<&str> = ordered.iter().map(|v| v.version.as_str()).collect();
        for v in &index.versions {
            if v.requires_minecraft().is_none() && !direct.contains(&v.version.as_str()) {
                ordered.push(v);
            }
        }
        if prefer_recommended {
            ordered.sort_by_key(|v| !v.recommended);
        }

        let mut last_err = format!("no {uid} version is available for Minecraft {mc_version}");
        for cand in ordered {
            let mut staged: Vec<ResolvedComponent> = Vec::new();
            let mut satisfied = true;
            for req in &cand.requires {
                if req.uid == "net.minecraft" {
                    if !req.equals.is_empty() && req.equals != mc_version {
                        satisfied = false;
                        break;
                    }
                    continue;
                }
                match resolve_inner(
                    data_root,
                    &req.uid,
                    mc_version,
                    prefer_recommended,
                    visiting,
                    chain,
                ) {
                    Ok(v) => {
                        if !req.equals.is_empty() && v != req.equals {
                            satisfied = false;
                            break;
                        }
                        staged.push(ResolvedComponent {
                            uid: req.uid.clone(),
                            version: v,
                        });
                    }
                    Err(e) => {
                        last_err = e;
                        satisfied = false;
                        break;
                    }
                }
            }
            if satisfied {
                chain.extend(staged);
                return Ok(cand.version.clone());
            }
        }
        Err(last_err)
    })();

    visiting.retain(|v| v != uid);
    result
}

/// Resolve the best single component version for a Minecraft version.
pub fn resolve(
    data_root: &Path,
    uid: &str,
    mc_version: &str,
    prefer_recommended: bool,
) -> Result<String, String> {
    let chain = resolve_chain(data_root, uid, mc_version, prefer_recommended)?;
    chain
        .into_iter()
        .find(|c| c.uid == uid)
        .map(|c| c.version)
        .ok_or_else(|| format!("{uid} was not resolved"))
}

/// Whether a candidate version's dependencies can all be satisfied for
/// `mc_version` (used by both resolution and version listing).
fn candidate_ok(
    data_root: &Path,
    cand: &MetaVersion,
    mc_version: &str,
    visiting: &mut Vec<String>,
) -> bool {
    for req in &cand.requires {
        if req.uid == "net.minecraft" {
            if !req.equals.is_empty() && req.equals != mc_version {
                return false;
            }
            continue;
        }
        let mut scratch = Vec::new();
        match resolve_inner(
            data_root,
            &req.uid,
            mc_version,
            false,
            visiting,
            &mut scratch,
        ) {
            Ok(v) => {
                if !req.equals.is_empty() && v != req.equals {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    true
}

/// A component version offered by the meta index, for UI listings.
#[derive(Debug, Clone)]
pub struct VersionChoice {
    pub version: String,
    pub recommended: bool,
    pub minecraft: Option<String>,
    /// The resolved Minecraft version this entry works with when the
    /// constraint is delegated to a dependency.
    pub derived_minecraft: Option<String>,
}

/// Versions of `uid` usable with the given Minecraft versions, newest first.
///
/// A component may declare its Minecraft constraint on a dependency rather
/// than directly (Fabric Loader → Fabric Intermediary), so candidates are
/// validated by resolving their dependency chain when a filter is supplied.
pub fn versions_for(
    data_root: &Path,
    uid: &str,
    minecraft: &[String],
) -> Result<Vec<VersionChoice>, String> {
    let index = index(data_root, uid)?;
    if minecraft.is_empty() {
        return Ok(index
            .versions
            .iter()
            .map(|v| VersionChoice {
                version: v.version.clone(),
                recommended: v.recommended,
                minecraft: v.requires_minecraft().map(|s| s.to_string()),
                derived_minecraft: None,
            })
            .collect());
    }
    let mut out = Vec::new();
    for mc in minecraft {
        let mut visiting = Vec::new();
        for v in &index.versions {
            let direct = v.requires_minecraft();
            let ok = match direct {
                Some(d) => d == mc,
                None => candidate_ok(data_root, v, mc, &mut visiting),
            };
            if ok {
                out.push(VersionChoice {
                    version: v.version.clone(),
                    recommended: v.recommended,
                    minecraft: direct.map(|s| s.to_string()).or_else(|| Some(mc.clone())),
                    derived_minecraft: direct.is_none().then(|| mc.clone()),
                });
            }
        }
    }
    Ok(out)
}

/// Ensure every component of an instance pack is downloaded (metadata +
/// libraries), so a launch can proceed.
///
/// Returns the number of components materialised and any component that could
/// not be resolved.
pub fn ensure_instance_components(
    data_root: &Path,
    pack: &crate::meta::ComponentPack,
) -> (usize, Vec<String>) {
    let mut ok = 0usize;
    let mut failed = Vec::new();
    for comp in &pack.components {
        if !is_indexed_component(&comp.uid) {
            continue; // net.minecraft is handled by Mojang metadata
        }
        match ensure_component(data_root, &comp.uid, &comp.version) {
            Ok(_) => ok += 1,
            Err(e) => failed.push(format!("{}: {e}", comp.uid)),
        }
    }
    (ok, failed)
}

/// Loader components that the meta index can resolve, in install order.
pub const LOADER_COMPONENTS: [&str; 6] = [
    "net.fabricmc.fabric-loader",
    "org.quiltmc.quilt-loader",
    "net.minecraftforge",
    "net.neoforged",
    "com.mumfrey.liteloader",
    "optifine",
];

/// Human-readable name for a component uid.
pub fn component_label(uid: &str) -> String {
    match uid {
        "net.minecraft" => "Minecraft".to_string(),
        "net.fabricmc.fabric-loader" => "Fabric Loader".to_string(),
        "net.fabricmc.intermediary" => "Fabric Intermediary".to_string(),
        "org.quiltmc.quilt-loader" => "Quilt Loader".to_string(),
        "net.minecraftforge" => "Forge".to_string(),
        "net.neoforged" => "NeoForge".to_string(),
        "com.mumfrey.liteloader" => "LiteLoader".to_string(),
        "optifine" => "OptiFine".to_string(),
        other => other.to_string(),
    }
}

/// Libraries implied by a component that must not be shown to the user.
pub fn is_hidden_library(name: &str) -> bool {
    name.starts_with("io.github.zekerzhayard:ForgeWrapper")
}

/// Convenience: a component's libraries filtered for display.
pub fn visible_libraries(component: &VersionJson) -> Vec<&Library> {
    component
        .libraries
        .iter()
        .filter(|l| !is_hidden_library(&l.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_index_and_resolves_versions() {
        let json = r#"{
            "formatVersion": 1,
            "name": "Forge",
            "uid": "net.minecraftforge",
            "versions": [
                {"version": "50.2.14", "recommended": false, "releaseTime": "2026-10-15T03:34:14+00:00",
                 "requires": [{"equals": "1.20.6", "uid": "net.minecraft"}]},
                {"version": "47.2.0", "recommended": true, "releaseTime": "2023-09-21T17:42:34+00:00",
                 "requires": [{"equals": "1.20.1", "uid": "net.minecraft"}]},
                {"version": "47.3.0", "recommended": false, "releaseTime": "2024-01-01T00:00:00+00:00",
                 "requires": [{"equals": "1.20.1", "uid": "net.minecraft"}]}
            ]
        }"#;
        let index: MetaIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.versions.len(), 3);
        assert_eq!(index.recommended_for("1.20.1").unwrap().version, "47.2.0");
        assert_eq!(index.newest_for("1.20.1").unwrap().version, "47.2.0");
        assert_eq!(index.for_minecraft("1.20.1").len(), 2);
        assert!(index.recommended_for("1.19.2").is_none());
        assert_eq!(
            index.find("50.2.14").unwrap().requires_minecraft(),
            Some("1.20.6")
        );
    }

    #[test]
    fn order_is_preserved_for_newest() {
        // The real index lists newest first; `newest_for` returns the first match.
        let json = r#"{"uid":"net.neoforged","versions":[
            {"version":"21.1.90","requires":[{"equals":"1.21.1","uid":"net.minecraft"}]},
            {"version":"21.1.80","requires":[{"equals":"1.21.1","uid":"net.minecraft"}]}
        ]}"#;
        let index: MetaIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.newest_for("1.21.1").unwrap().version, "21.1.90");
    }

    #[test]
    fn caches_and_reads_component_metadata() {
        let td = tempfile::tempdir().unwrap();
        let dir = meta_dir(td.path(), "net.minecraftforge");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("47.2.0.json"),
            r#"{"uid":"net.minecraftforge","version":"47.2.0","name":"Forge",
                "mainClass":"io.github.zekerzhayard.forgewrapper.installer.Main",
                "minecraftArguments":"--launchTarget forgeclient",
                "libraries":[{"name":"io.github.zekerzhayard:ForgeWrapper:prism-2026-08-01"}],
                "mavenFiles":[{"name":"net.minecraftforge:forge:1.20.1-47.2.0:installer"}]}"#,
        )
        .unwrap();
        let comp = cached_component(td.path(), "net.minecraftforge", "47.2.0").unwrap();
        assert_eq!(comp.component_uid, "net.minecraftforge");
        assert!(comp.main_class.contains("forgewrapper"));
        assert_eq!(comp.libraries.len(), 1);
        assert_eq!(comp.maven_files.len(), 1);
        assert_eq!(visible_libraries(&comp).len(), 0);
    }

    #[test]
    fn classifies_maven_files() {
        let lib = Library {
            name: "net.minecraftforge:forge:1.20.1-47.2.0:installer".into(),
            ..Default::default()
        };
        assert_eq!(
            lib.maven_path().unwrap(),
            "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-installer.jar"
        );
        assert_eq!(lib.classifier(), Some("installer"));
    }

    #[test]
    fn labels_components() {
        assert_eq!(component_label("net.minecraftforge"), "Forge");
        assert_eq!(component_label("net.neoforged"), "NeoForge");
        assert_eq!(component_label("mystery"), "mystery");
    }

    #[test]
    fn resolves_loader_with_dependency_chain() {
        // Fabric Loader has no `net.minecraft` requirement of its own; the
        // Minecraft version lives on its intermediary dependency.
        let td = tempfile::tempdir().unwrap();
        let loader = meta_dir(td.path(), "net.fabricmc.fabric-loader");
        std::fs::create_dir_all(&loader).unwrap();
        std::fs::write(
            loader.join("index.json"),
            r#"{"uid":"net.fabricmc.fabric-loader","versions":[
                {"version":"0.20.0","requires":[{"uid":"net.fabricmc.intermediary"}]},
                {"version":"0.19.5","recommended":true,"requires":[{"uid":"net.fabricmc.intermediary"}]}]}"#,
        )
        .unwrap();
        let inter = meta_dir(td.path(), "net.fabricmc.intermediary");
        std::fs::create_dir_all(&inter).unwrap();
        std::fs::write(
            inter.join("index.json"),
            r#"{"uid":"net.fabricmc.intermediary","versions":[
                {"version":"26.4-snapshot-4","requires":[{"equals":"26.4-snapshot-4","uid":"net.minecraft"}]},
                {"version":"1.21.1","recommended":true,"requires":[{"equals":"1.21.1","uid":"net.minecraft"}]}]}"#,
        )
        .unwrap();

        let chain = resolve_chain(td.path(), "net.fabricmc.fabric-loader", "1.21.1", true).unwrap();
        assert_eq!(
            chain,
            vec![
                ResolvedComponent {
                    uid: "net.fabricmc.intermediary".into(),
                    version: "1.21.1".into()
                },
                ResolvedComponent {
                    uid: "net.fabricmc.fabric-loader".into(),
                    version: "0.19.5".into()
                },
            ]
        );
        // Without a recommendation preference the newest loader still wins
        // because both candidates resolve.
        let chain =
            resolve_chain(td.path(), "net.fabricmc.fabric-loader", "1.21.1", false).unwrap();
        assert_eq!(chain.last().unwrap().version, "0.20.0");
        // An unsupported Minecraft version fails cleanly.
        assert!(resolve_chain(td.path(), "net.fabricmc.fabric-loader", "1.7.10", true).is_err());
    }

    #[test]
    fn offline_resolve_uses_cache() {
        let td = tempfile::tempdir().unwrap();
        let dir = meta_dir(td.path(), "org.quiltmc.quilt-loader");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("index.json"),
            r#"{"uid":"org.quiltmc.quilt-loader","versions":[
                {"version":"0.27.1","recommended":true,
                 "requires":[{"equals":"1.21.1","uid":"net.minecraft"}]}]}"#,
        )
        .unwrap();
        // No network access needed: `index` reads the cache first.
        let v = resolve(td.path(), "org.quiltmc.quilt-loader", "1.21.1", true).unwrap();
        assert_eq!(v, "0.27.1");
        let err = resolve(td.path(), "org.quiltmc.quilt-loader", "1.20.1", true).unwrap_err();
        assert!(err.contains("no org.quiltmc.quilt-loader version"));
    }
}
