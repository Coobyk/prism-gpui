//! Metadata parsing: Prism component packs and Mojang version JSON.
//! Mirrors: launcher/meta/*, launcher/minecraft/MojangVersionFormat, OneSixVersionFormat.

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct ComponentPack {
    #[serde(default)]
    pub components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
pub struct Component {
    pub uid: String,
    pub version: String,
    #[serde(default, rename = "cachedName")]
    pub cached_name: String,
    #[serde(default, rename = "dependencyOnly")]
    pub dependency_only: bool,
}

pub fn read_component_pack(inst_dir: &Path) -> Option<ComponentPack> {
    let text = std::fs::read_to_string(inst_dir.join("mmc-pack.json")).ok()?;
    serde_json::from_str(&text).ok()
}

#[derive(Debug, Deserialize, Default)]
pub struct VersionJson {
    #[serde(default, rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// Artifacts that must exist locally (e.g. a Forge installer) but are not
    /// added to the classpath.
    #[serde(default, rename = "mavenFiles")]
    pub maven_files: Vec<Library>,
    #[serde(default, rename = "assetIndex")]
    pub asset_index: Option<AssetIndex>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default, rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, rename = "type")]
    pub version_type: String,
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "javaVersion")]
    pub java_version: Option<JavaVersion>,
    /// Prism component metadata: extra JVM/game arguments, which may be plain
    /// strings or rule-based objects.
    #[serde(default, rename = "+jvmArgs")]
    pub extra_jvm_args: Vec<serde_json::Value>,
    #[serde(default, rename = "+gameArgs")]
    pub extra_game_args: Vec<serde_json::Value>,
    #[serde(default, rename = "+traits")]
    pub traits: Vec<String>,
    #[serde(default, rename = "+agents")]
    pub agents: Vec<Agent>,
    /// Mojang's `downloads` block (client/server jars).
    #[serde(default)]
    pub downloads: Option<MojangDownloads>,
    #[serde(default)]
    pub order: i32,
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub conflicts: Vec<Requirement>,
    #[serde(default, rename = "name")]
    pub component_name: String,
    #[serde(default, rename = "uid")]
    pub component_uid: String,
    #[serde(default)]
    pub version: String,
}

/// A Prism component dependency/conflict declaration.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Requirement {
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub equals: String,
    #[serde(default)]
    pub suggests: String,
}

/// A Java agent declared by a component (`+agents`).
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Agent {
    #[serde(default)]
    pub argument: String,
    #[serde(default)]
    pub library: Option<Library>,
    #[serde(default)]
    pub main_class: String,
}

impl VersionJson {
    /// Does this component have something to contribute to the launch?
    pub fn is_empty_meta(&self) -> bool {
        self.main_class.is_empty() && self.libraries.is_empty()
    }

    /// Whether this component is `net.minecraft` (the base game).
    pub fn is_minecraft(&self) -> bool {
        self.component_uid == "net.minecraft"
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct JavaVersion {
    #[serde(default, rename = "majorVersion")]
    pub major_version: u32,
    #[serde(default, rename = "component")]
    pub component: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AssetIndex {
    pub id: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<Downloads>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub clientreq: Option<bool>,
    /// Mojang rule objects; when present the library only applies if the rules
    /// resolve to `allow` for the current platform.
    #[serde(default)]
    pub rules: Vec<serde_json::Value>,
    /// Legacy natives map (`{"linux": "natives-linux"}`).
    #[serde(default)]
    pub natives: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub extract: Option<Extract>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Downloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub classifiers: std::collections::BTreeMap<String, Artifact>,
}

/// Mojang's top-level `downloads` block: the vanilla client and server jars.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MojangDownloads {
    #[serde(default)]
    pub client: Option<Artifact>,
    #[serde(default)]
    pub server: Option<Artifact>,
}

/// The current platform's Mojang OS name.
pub fn os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

/// The current platform's Mojang architecture name.
pub fn os_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => other,
    }
}

/// Whether a single Mojang rule object matches the current platform.
fn rule_matches(rule: &serde_json::Value) -> bool {
    if let Some(features) = rule.get("features").and_then(|f| f.as_object()) {
        // The launcher enables no optional features, so any rule that requires
        // one (is_demo_user, has_custom_resolution, …) never matches.
        if !features.is_empty() {
            return false;
        }
    }
    if let Some(os) = rule.get("os") {
        if let Some(name) = os.get("name").and_then(|v| v.as_str()) {
            if name != os_name() {
                return false;
            }
        }
        if let Some(arch) = os.get("arch").and_then(|v| v.as_str()) {
            if arch != os_arch() {
                return false;
            }
        }
        // `os.version` is a regex over the OS version; matching it requires the
        // running kernel version, which we do not query, so an os.version rule
        // is treated as not matching (this matches the common case where such
        // rules refine newer-OS-only behaviour).
        if os.get("version").is_some() {
            return false;
        }
    }
    true
}

/// Evaluate a Mojang rules array. An empty array means "always".
pub fn rules_allow(rules: &[serde_json::Value]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.get("action").and_then(|a| a.as_str()) == Some("allow");
        }
    }
    allowed
}

impl Library {
    /// Whether this library applies to the current platform.
    pub fn applies(&self) -> bool {
        rules_allow(&self.rules)
    }

    /// The legacy natives classifier for the current platform, if declared.
    pub fn native_classifier(&self) -> Option<String> {
        let key = os_name();
        self.natives
            .get(key)
            .map(|c| c.replace("${arch}", if cfg!(target_pointer_width = "64") { "64" } else { "32" }))
    }

    /// A modern (`:natives-<os>`) native jar.
    pub fn is_modern_native(&self) -> bool {
        self.name.contains(":natives-") && self.applies()
    }

    /// The artifact holding this library's natives, if any.
    pub fn native_artifact(&self) -> Option<Artifact> {
        if let Some(classifier) = self.native_classifier() {
            if let Some(a) = self
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.get(&classifier))
            {
                return Some(a.clone());
            }
        }
        if self.is_modern_native() {
            return self.downloads.as_ref().and_then(|d| d.artifact.clone());
        }
        None
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct Artifact {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub url: String,
}

impl Library {
    /// Maven path, e.g. `org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar`.
    ///
    /// Handles classifiers and extensions
    /// (`group:artifact:version:classifier@ext`), which Prism's `mavenFiles`
    /// use for installer jars such as `net.minecraftforge:forge:<v>:installer`.
    pub fn maven_path(&self) -> Option<String> {
        if let Some(path) = self
            .downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.path.clone())
            .filter(|p| !p.is_empty())
        {
            return Some(path);
        }
        maven_path_of(&self.name)
    }

    /// The classifier carried in the maven coordinate, if any.
    pub fn classifier(&self) -> Option<&str> {
        let (rest, _ext) = split_extension(&self.name);
        let parts: Vec<&str> = rest.split(':').collect();
        parts.get(3).copied().filter(|s| !s.is_empty())
    }

    /// The file extension from the maven coordinate (defaults to `jar`).
    pub fn extension(&self) -> &str {
        let (_rest, ext) = split_extension(&self.name);
        ext
    }
}

/// Split `group:a:v:classifier@ext` into (`group:a:v:classifier`, `ext`).
fn split_extension(name: &str) -> (&str, &str) {
    match name.split_once('@') {
        Some((rest, ext)) if !ext.is_empty() => (rest, ext),
        _ => (name, "jar"),
    }
}

/// Build a maven path from a coordinate, honouring classifiers/extensions.
pub fn maven_path_of(name: &str) -> Option<String> {
    let (rest, ext) = split_extension(name);
    let parts: Vec<&str> = rest.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let classifier = parts.get(3).copied().filter(|s| !s.is_empty());
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{}/{}/{}",
        group.replace('.', "/"),
        artifact,
        version,
        file
    ))
}

pub fn read_version_json(path: &Path) -> Option<VersionJson> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Mojang's version manifest (all Minecraft versions and their metadata URLs).
pub const MOJANG_VERSION_MANIFEST: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Deserialize, Default, Clone)]
pub struct VersionManifest {
    #[serde(default)]
    pub latest: LatestVersions,
    #[serde(default)]
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct LatestVersions {
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub snapshot: String,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ManifestVersion {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "type")]
    pub version_type: String,
    #[serde(default)]
    pub release_time: String,
}

impl VersionManifest {
    pub fn find(&self, id: &str) -> Option<&ManifestVersion> {
        self.versions.iter().find(|v| v.id == id)
    }
}

/// Fetch (and cache) Mojang's version manifest.
pub fn version_manifest(data_root: &Path) -> Result<VersionManifest, String> {
    let cache = data_root.join("meta").join("mojang_version_manifest.json");
    if let Ok(text) = std::fs::read_to_string(&cache) {
        if let Ok(m) = serde_json::from_str::<VersionManifest>(&text) {
            if !m.versions.is_empty() {
                return Ok(m);
            }
        }
    }
    let body = crate::net::get_string(MOJANG_VERSION_MANIFEST)?;
    let manifest: VersionManifest =
        serde_json::from_str(&body).map_err(|e| format!("malformed version manifest: {e}"))?;
    if let Some(dir) = cache.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&cache, body.as_bytes());
    Ok(manifest)
}

/// Path of the cached `net.minecraft` metadata for a version.
pub fn minecraft_metadata_path(data_root: &Path, version: &str) -> std::path::PathBuf {
    data_root
        .join("meta")
        .join("net.minecraft")
        .join(format!("{version}.json"))
}

/// Ensure `meta/net.minecraft/<version>.json` exists, downloading it from
/// Mojang along with its libraries and asset index.
pub fn ensure_minecraft_metadata(data_root: &Path, version: &str) -> Result<VersionJson, String> {
    let dest = minecraft_metadata_path(data_root, version);
    let json = if let Some(v) = read_version_json(&dest) {
        v
    } else {
        let manifest = version_manifest(data_root)?;
        let entry = manifest.find(version).ok_or_else(|| {
            format!("Minecraft {version} is not in Mojang's version manifest")
        })?;
        let body = crate::net::get_string(&entry.url)?;
        let parsed: VersionJson = serde_json::from_str(&body)
            .map_err(|e| format!("Minecraft {version} metadata was malformed: {e}"))?;
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, body.as_bytes()).map_err(|e| e.to_string())?;
        parsed
    };
    Ok(json)
}

/// Ensure the Minecraft version's metadata, libraries and asset index are all
/// present. Returns the number of files downloaded.
pub fn ensure_minecraft(data_root: &Path, version: &str) -> Result<usize, String> {
    let json = ensure_minecraft_metadata(data_root, version)?;
    let mut n = ensure_libraries(data_root, &json)?;
    if let Some(index) = ensure_asset_index(data_root, &json)? {
        // Only fetch the index itself here; objects are fetched on demand.
        if index.is_file() {
            n += 1;
        }
    }
    // The client jar lives with the other Mojang libraries so every instance of
    // the same version shares it (Prism uses the same download).
    if ensure_vanilla_client(data_root, &json)? {
        n += 1;
    }
    Ok(n)
}

/// Path of the shared vanilla client jar for a version.
pub fn vanilla_client_jar(data_root: &Path, version: &str) -> std::path::PathBuf {
    data_root
        .join("libraries")
        .join("com")
        .join("mojang")
        .join("minecraft")
        .join(version)
        .join(format!("minecraft-{version}-client.jar"))
}

/// Download the vanilla client jar if it is missing. Returns `true` when a
/// download happened.
pub fn ensure_vanilla_client(data_root: &Path, version: &VersionJson) -> Result<bool, String> {
    let Some(client) = version.downloads.as_ref().and_then(|d| d.client.as_ref()) else {
        return Ok(false);
    };
    if client.url.is_empty() {
        return Ok(false);
    }
    let dest = vanilla_client_jar(data_root, &version.id);
    if dest.is_file() {
        return Ok(false);
    }
    crate::net::download_verified(&client.url, &dest, Some(&client.sha1))?;
    Ok(true)
}

/// Ensure every asset object referenced by a version's asset index is present.
pub fn ensure_all_assets(data_root: &Path, version: &str) -> Result<usize, String> {
    let path = data_root.join("meta").join("net.minecraft").join(format!("{version}.json"));
    let json = read_version_json(&path).ok_or_else(|| format!("no metadata for {version}"))?;
    let Some(index) = ensure_asset_index(data_root, &json)? else {
        return Ok(0);
    };
    ensure_assets(data_root, &index, None)
}

/// Path of the vanilla client jar used on an instance's classpath.
///
/// Prism keeps the jar in the shared library cache; older MultiMC layouts kept
/// a copy inside the instance. Both work, so use the instance copy when present
/// and fall back to the shared one.
pub fn client_jar_for(inst_dir: &Path, data_root: &Path, version: &str) -> std::path::PathBuf {
    let local = inst_dir.join("minecraft").join("client.jar");
    if local.is_file() {
        local
    } else {
        vanilla_client_jar(data_root, version)
    }
}

/// List Minecraft versions from Mojang's manifest (newest first).
pub fn minecraft_versions(data_root: &Path) -> Result<Vec<ManifestVersion>, String> {
    version_manifest(data_root).map(|m| m.versions)
}

/// Java major version required by an instance's Minecraft version. Prefers the
/// `javaVersion.majorVersion` field of the Mojang version JSON and falls back
/// to a version-string heuristic when that JSON has not been downloaded yet.
pub fn required_java_major(inst_dir: &Path, data_root: &Path) -> u32 {
    if let Some(pack) = read_component_pack(inst_dir) {
        for comp in &pack.components {
            if comp.uid == "net.minecraft" {
                let candidate = data_root
                    .join("meta")
                    .join("net.minecraft")
                    .join(format!("{}.json", comp.version));
                if let Some(v) = read_version_json(&candidate) {
                    if let Some(j) = v.java_version {
                        if j.major_version > 0 {
                            return j.major_version;
                        }
                    }
                }
                return crate::java::heuristic_major(&comp.version);
            }
        }
    }
    8
}

/// Resolve the classpath jars for a version from `<libraries>/<maven path>`,
/// including only libraries that apply to the current platform.
pub fn classpath(libs_dir: &Path, version: &VersionJson) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for lib in &version.libraries {
        if !lib.applies() {
            continue;
        }
        let Some(p) = lib.maven_path() else { continue };
        let full = libs_dir.join(p);
        if full.is_file() {
            out.push(full);
        }
    }
    out
}

/// Natives jars that must be extracted for the current platform.
pub fn natives_artifacts(libs_dir: &Path, version: &VersionJson) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for lib in &version.libraries {
        if !lib.applies() {
            continue;
        }
        let Some(artifact) = lib.native_artifact() else {
            continue;
        };
        let rel = if !artifact.path.is_empty() {
            artifact.path.clone()
        } else if let Some(p) = lib.maven_path() {
            p
        } else {
            continue;
        };
        let full = libs_dir.join(rel);
        if full.is_file() {
            out.push(full);
        }
    }
    out
}

impl Library {
    /// Resolve the download URL for this library's artifact.
    pub fn artifact_url(&self) -> Option<String> {
        if let Some(a) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            if !a.url.is_empty() {
                return Some(a.url.clone());
            }
        }
        let path = self.maven_path()?;
        let base = self
            .url
            .clone()
            .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
        Some(format!(
            "{}/{}",
            base.trim_end_matches('/'),
            path
        ))
    }

    pub fn artifact_sha1(&self) -> Option<&str> {
        self.downloads
            .as_ref()
            .and_then(|d| d.artifact.as_ref())
            .map(|a| a.sha1.as_str())
            .filter(|s| !s.is_empty())
    }
}

/// Ensure all of a version's libraries (and maven-only artifacts) are present
/// under `<data_root>/libraries`. Only libraries that apply to this platform
/// are fetched. Returns the number of files downloaded.
pub fn ensure_libraries(data_root: &Path, version: &VersionJson) -> Result<usize, String> {
    let libs_dir = data_root.join("libraries");
    let mut downloaded = 0;
    for lib in version.libraries.iter().chain(version.maven_files.iter()) {
        if !lib.rules.is_empty() && !lib.applies() {
            continue;
        }
        // A library with a platform-specific natives map only contributes the
        // matching classifier.
        if !lib.natives.is_empty() {
            if let Some(artifact) = lib.native_artifact() {
                let Some(rel) = (if !artifact.path.is_empty() {
                    Some(artifact.path.clone())
                } else {
                    lib.maven_path()
                }) else {
                    continue;
                };
                let url = if !artifact.url.is_empty() {
                    artifact.url.clone()
                } else {
                    let base = lib
                        .url
                        .clone()
                        .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
                    format!("{}/{}", base.trim_end_matches('/'), rel)
                };
                let dest = libs_dir.join(&rel);
                let before = dest.is_file();
                let sha = (!artifact.sha1.is_empty()).then_some(artifact.sha1.as_str());
                crate::net::download_verified(&url, &dest, sha)?;
                if !before && dest.is_file() {
                    downloaded += 1;
                }
            }
            continue;
        }
        let Some(rel) = lib.maven_path() else { continue };
        let Some(url) = lib.artifact_url() else { continue };
        let dest = libs_dir.join(&rel);
        let before = dest.is_file();
        crate::net::download_verified(&url, &dest, lib.artifact_sha1())?;
        if !before && dest.is_file() {
            downloaded += 1;
        }
    }
    Ok(downloaded)
}

/// Ensure the asset index file is present; returns its path.
pub fn ensure_asset_index(data_root: &Path, version: &VersionJson) -> Result<Option<std::path::PathBuf>, String> {
    let Some(ai) = &version.asset_index else {
        return Ok(None);
    };
    if ai.url.is_empty() {
        return Ok(None);
    }
    let dest = data_root
        .join("assets")
        .join("indexes")
        .join(format!("{}.json", ai.id));
    crate::net::download_verified(&ai.url, &dest, None)?;
    Ok(Some(dest))
}

#[derive(Debug, Deserialize, Default)]
pub struct AssetIndexFile {
    #[serde(default)]
    pub objects: std::collections::BTreeMap<String, AssetObject>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// Ensure all assets referenced by the index are present under
/// `<data_root>/assets/objects`. `limit` caps downloads (None = all).
pub fn ensure_assets(
    data_root: &Path,
    index_path: &Path,
    limit: Option<usize>,
) -> Result<usize, String> {
    // Fetch several objects at once, like Prism's NetJob with
    // `NumberOfConcurrentDownloads` workers.
    let workers = crate::settings::GlobalSettings::load(data_root)
        .get_int("NumberOfConcurrentDownloads")
        .clamp(1, 16) as usize;
    ensure_assets_with_workers(data_root, index_path, limit, workers)
}

/// Concurrent variant of [`ensure_assets`], used by tests and callers that
/// already know how many workers they want.
pub fn ensure_assets_with_workers(
    data_root: &Path,
    index_path: &Path,
    limit: Option<usize>,
    workers: usize,
) -> Result<usize, String> {
    let text = std::fs::read_to_string(index_path).map_err(|e| e.to_string())?;
    let index: AssetIndexFile = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let objects_dir = data_root.join("assets").join("objects");
    let todo: Vec<String> = index
        .objects
        .values()
        .map(|o| o.hash.clone())
        .filter(|h| {
            h.len() > 2 && !objects_dir.join(&h[..2]).join(h).is_file()
        })
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    if todo.is_empty() {
        return Ok(0);
    }

    let workers = workers.clamp(1, 16).min(todo.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let downloaded = std::sync::atomic::AtomicUsize::new(0);
    let error: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(hash) = todo.get(i) else { break };
                if error.lock().map(|e| e.is_some()).unwrap_or(true) {
                    break;
                }
                let sub = &hash[..2];
                let dest = objects_dir.join(sub).join(hash);
                let url = format!("https://resources.download.minecraft.net/{sub}/{hash}");
                match crate::net::download_verified(&url, &dest, Some(hash)) {
                    Ok(()) => {
                        downloaded.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => {
                        if let Ok(mut slot) = error.lock() {
                            if slot.is_none() {
                                *slot = Some(e);
                            }
                        }
                        break;
                    }
                }
            });
        }
    });

    if let Some(e) = error.into_inner().unwrap_or(None) {
        return Err(e);
    }
    Ok(downloaded.load(std::sync::atomic::Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maven_path_download() {
        let lib = Library {
            name: "org.lwjgl:lwjgl:3.3.1".into(),
            downloads: Some(Downloads {
                artifact: Some(Artifact {
                    path: "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar".into(),
                    ..Default::default()
                }),
                classifiers: Default::default(),
            }),
            ..Default::default()
        };
        assert_eq!(
            lib.maven_path().unwrap(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1.jar"
        );
    }
    #[test]
    fn parses_asset_index() {
        let t = r#"{"objects":{"minecraft/sounds/a.ogg":{"hash":"aabbcc","size":10}}}"#;
        let idx: AssetIndexFile = serde_json::from_str(t).unwrap();
        let obj = idx.objects.get("minecraft/sounds/a.ogg").unwrap();
        assert_eq!(obj.hash, "aabbcc");
        assert_eq!(obj.size, 10);
    }

    #[test]
    fn asset_download_skips_present_objects() {
        let td = tempfile::tempdir().unwrap();
        let index_path = td.path().join("index.json");
        let hashes = [
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ];
        let objects = td.path().join("assets").join("objects");
        for h in hashes {
            let dir = objects.join(&h[..2]);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(h), b"x").unwrap();
        }
        std::fs::write(
            &index_path,
            r#"{"objects":{"a":{"hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","size":1},"b":{"hash":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","size":1}}}"#,
        )
        .unwrap();
        // Everything is already present, so no network access happens.
        assert_eq!(
            ensure_assets_with_workers(td.path(), &index_path, None, 4).unwrap(),
            0
        );
    }

    #[test]
    fn vanilla_client_jar_path_matches_prism_layout() {
        let p = vanilla_client_jar(Path::new("/data"), "1.21.1");
        assert!(p.ends_with("libraries/com/mojang/minecraft/1.21.1/minecraft-1.21.1-client.jar"));
    }

    #[test]
    fn parses_mojang_downloads_block() {
        let json = r#"{"id":"1.21.1","mainClass":"a.B","downloads":{
            "client":{"path":"x.jar","sha1":"abc","size":5,"url":"https://example.invalid/x.jar"},
            "server":{"path":"s.jar","sha1":"def","size":6,"url":"https://example.invalid/s.jar"}}}"#;
        let v: VersionJson = serde_json::from_str(json).unwrap();
        let d = v.downloads.unwrap();
        assert_eq!(d.client.unwrap().sha1, "abc");
        assert_eq!(d.server.unwrap().size, 6);
    }

    #[test]
    fn artifact_url_derived() {
        let lib = Library {
            name: "org.ow2.asm:asm:9.10.1".into(),
            url: Some("https://maven.fabricmc.net/".into()),
            ..Default::default()
        };
        assert_eq!(
            lib.artifact_url().unwrap(),
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
    }

    #[test]
    fn evaluates_mojang_rules() {
        use serde_json::json;
        // No rules: always applies.
        assert!(rules_allow(&[]));
        // OS-specific allow.
        let linux_only = vec![json!({"action":"allow","os":{"name":"linux"}})];
        let mac_only = vec![json!({"action":"allow","os":{"name":"osx"}})];
        assert_eq!(rules_allow(&linux_only), cfg!(target_os = "linux"));
        assert_eq!(rules_allow(&mac_only), cfg!(target_os = "macos"));
        // An allow for another OS followed by a blanket disallow.
        let rules = vec![
            json!({"action":"allow","os":{"name":"windows"}}),
            json!({"action":"disallow"}),
        ];
        assert!(!rules_allow(&rules));
        // Feature-gated rules never match.
        let demo = vec![json!({"action":"allow","features":{"is_demo_user":true}})];
        assert!(!rules_allow(&demo));
    }

    #[test]
    fn library_applicability_and_natives() {
        use serde_json::json;
        let lib: Library = serde_json::from_value(json!({
            "name": "ca.weblite:java-objc-bridge:1.1",
            "rules": [{"action":"allow","os":{"name":"osx"}}]
        }))
        .unwrap();
        assert_eq!(lib.applies(), cfg!(target_os = "macos"));

        let modern: Library = serde_json::from_value(json!({
            "name": "org.lwjgl:lwjgl-glfw:3.3.1:natives-linux",
            "downloads": {"artifact": {
                "path": "org/lwjgl/lwjgl-glfw/3.3.1/lwjgl-glfw-3.3.1-natives-linux.jar"
            }},
            "rules": [{"action":"allow","os":{"name":"linux"}}]
        }))
        .unwrap();
        assert_eq!(modern.is_modern_native(), cfg!(target_os = "linux"));
        assert_eq!(
            modern.native_artifact().map(|a| a.path),
            cfg!(target_os = "linux").then(|| "org/lwjgl/lwjgl-glfw/3.3.1/lwjgl-glfw-3.3.1-natives-linux.jar".to_string())
        );
    }

    #[test]
    fn legacy_natives_classifier_selected_for_os() {
        let lib: Library = serde_json::from_str(&format!(
            r#"{{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
                 "natives":{{"{}":"natives-{}"}},
                 "downloads":{{"classifiers":{{"natives-{}":{{"path":"org/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4-natives-{}.jar"}}}}}}}}"#,
            os_name(), os_name(), os_name(), os_name()
        ))
        .unwrap();
        assert_eq!(
            lib.native_classifier().as_deref(),
            Some(format!("natives-{}", os_name()).as_str())
        );
        assert!(lib.native_artifact().is_some());
    }

    #[test]
    fn classpath_skips_inapplicable_libraries() {
        use serde_json::json;
        let td = tempfile::tempdir().unwrap();
        let libs = td.path();
        std::fs::create_dir_all(libs.join("a/b/1")).unwrap();
        std::fs::create_dir_all(libs.join("c/d/2")).unwrap();
        std::fs::write(libs.join("a/b/1/b-1.jar"), b"x").unwrap();
        std::fs::write(libs.join("c/d/2/d-2.jar"), b"x").unwrap();
        let version: VersionJson = serde_json::from_value(json!({
            "libraries": [
                {"name":"a.b:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar"}}},
                {"name":"c.d:d:2","downloads":{"artifact":{"path":"c/d/2/d-2.jar"}},
                 "rules":[{"action":"allow","os":{"name":"plan9"}}]}
            ]
        }))
        .unwrap();
        let cp = classpath(libs, &version);
        assert_eq!(cp.len(), 1);
        assert!(cp[0].ends_with("a/b/1/b-1.jar"));
    }

    #[test]
    fn maven_path_derived() {
        let lib = Library {
            name: "org.ow2.asm:asm:9.10.1".into(),
            ..Default::default()
        };
        assert_eq!(
            lib.maven_path().unwrap(),
            "org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct MetaIndex {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub uid: String,
    #[serde(default)]
    pub versions: Vec<MetaVersion>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MetaVersion {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub recommended: bool,
    #[serde(rename = "type", default)]
    pub release_type: String,
}

/// List available versions for a component from `<data_root>/meta/<uid>/index.json`.
pub fn list_versions(data_root: &Path, uid: &str) -> Option<MetaIndex> {
    let path = data_root.join("meta").join(uid).join("index.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Set (or add) a component version in an instance's mmc-pack.json.
pub fn set_component(inst_dir: &Path, uid: &str, version: &str) -> Result<(), String> {
    let path = inst_dir.join("mmc-pack.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        r#"{"formatVersion":1,"components":[]}"#.to_string()
    });
    let mut value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let components = value
        .get_mut("components")
        .and_then(|c| c.as_array_mut())
        .ok_or("mmc-pack.json has no components array")?;
    let mut found = false;
    for c in components.iter_mut() {
        if c.get("uid").and_then(|u| u.as_str()) == Some(uid) {
            c["version"] = serde_json::Value::String(version.to_string());
            if let Some(name) = c.get("cachedName").and_then(|n| n.as_str()) {
                let _ = name;
            }
            found = true;
        }
    }
    if !found {
        components.push(serde_json::json!({
            "uid": uid,
            "version": version,
            "cachedName": uid
        }));
    }
    let out = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod component_tests {
    use super::*;
    #[test]
    fn set_and_update_component() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path();
        set_component(dir, "net.minecraft", "1.20.1").unwrap();
        let pack = read_component_pack(dir).unwrap();
        assert_eq!(pack.components[0].version, "1.20.1");
        set_component(dir, "net.minecraft", "1.21.1").unwrap();
        let pack = read_component_pack(dir).unwrap();
        assert_eq!(pack.components[0].version, "1.21.1");
        assert_eq!(pack.components.len(), 1);
    }
}
