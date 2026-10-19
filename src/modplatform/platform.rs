//! Unified content-platform facade.
//!
//! Prism talks to several mod platforms through per-platform classes. Here the
//! shared operations the UI and CLI need — search, list versions, install — are
//! exposed behind one enum, so the content browser and CLI stay platform
//! agnostic.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// A mod/content platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Platform {
    Modrinth,
    CurseForge,
    Ftb,
    Technic,
    AtLauncher,
    Packwiz,
}

impl Platform {
    pub const ALL: [Platform; 6] = [
        Platform::Modrinth,
        Platform::CurseForge,
        Platform::Ftb,
        Platform::Technic,
        Platform::AtLauncher,
        Platform::Packwiz,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Platform::Modrinth => "Modrinth",
            Platform::CurseForge => "CurseForge",
            Platform::Ftb => "FTB",
            Platform::Technic => "Technic",
            Platform::AtLauncher => "ATLauncher",
            Platform::Packwiz => "Packwiz",
        }
    }

    /// Platforms that host individual mods (with a mod-loader filter).
    pub fn supports_mods(&self) -> bool {
        matches!(self, Platform::Modrinth | Platform::CurseForge)
    }

    /// Platforms that host resource packs / shaders.
    pub fn supports_assets(&self) -> bool {
        matches!(self, Platform::Modrinth | Platform::CurseForge)
    }

    /// Platforms that host modpacks.
    pub fn supports_packs(&self) -> bool {
        !matches!(self, Platform::Packwiz)
    }

    /// Whether the platform needs a user-provided API key before it can be
    /// used. Reported by the UI so the settings page can be linked.
    pub fn needs_api_key(&self) -> bool {
        matches!(self, Platform::CurseForge)
    }

    /// Parse a platform name (case-insensitive, allows aliases).
    pub fn from_name(name: &str) -> Option<Platform> {
        match name.trim().to_ascii_lowercase().as_str() {
            "modrinth" | "mr" => Some(Platform::Modrinth),
            "curseforge" | "cf" | "flame" => Some(Platform::CurseForge),
            "ftb" => Some(Platform::Ftb),
            "technic" => Some(Platform::Technic),
            "atlauncher" | "atl" => Some(Platform::AtLauncher),
            "packwiz" | "pw" => Some(Platform::Packwiz),
            _ => None,
        }
    }

    /// Platforms available for a content kind.
    pub fn for_kind(kind: ContentKind) -> Vec<Platform> {
        Platform::ALL
            .iter()
            .copied()
            .filter(|p| match kind {
                ContentKind::Mod => p.supports_mods(),
                ContentKind::Modpack => p.supports_packs(),
                ContentKind::ResourcePack | ContentKind::Shader => p.supports_assets(),
            })
            .collect()
    }
}

/// What is being browsed/installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentKind {
    Mod,
    Modpack,
    ResourcePack,
    Shader,
}

impl ContentKind {
    pub fn label(&self) -> &'static str {
        match self {
            ContentKind::Mod => "Mods",
            ContentKind::Modpack => "Modpacks",
            ContentKind::ResourcePack => "Resource Packs",
            ContentKind::Shader => "Shader Packs",
        }
    }

    /// Modrinth `project_type` value.
    pub fn modrinth_type(&self) -> &'static str {
        match self {
            ContentKind::Mod => "mod",
            ContentKind::Modpack => "modpack",
            ContentKind::ResourcePack => "resourcepack",
            ContentKind::Shader => "shader",
        }
    }

    /// CurseForge class id.
    pub fn curseforge_class(&self) -> u32 {
        use crate::modplatform::curseforge as cf;
        match self {
            ContentKind::Mod => cf::CLASS_MOD,
            ContentKind::Modpack => cf::CLASS_MODPACK,
            ContentKind::ResourcePack => cf::CLASS_RESOURCEPACK,
            ContentKind::Shader => cf::CLASS_SHADER,
        }
    }
}

/// A platform-independent search hit.
#[derive(Debug, Clone)]
pub struct PlatformHit {
    pub platform: Platform,
    pub kind: ContentKind,
    /// Platform project id (used for installs).
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
    /// Extra install context, platform specific (see `extra` docs at call
    /// sites). Examples: FTB pack id + version, ATLauncher safe name + version.
    pub extra: String,
}

/// Search filters.
#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }

    pub fn with_game_version(mut self, v: Option<String>) -> Self {
        self.game_version = v.filter(|s| !s.is_empty());
        self
    }

    pub fn with_loader(mut self, l: Option<String>) -> Self {
        self.loader = l.filter(|s| !s.is_empty());
        self
    }
}

/// Search one platform.
pub fn search(
    platform: Platform,
    kind: ContentKind,
    query: &SearchQuery,
) -> Result<Vec<PlatformHit>, String> {
    match platform {
        Platform::Modrinth => {
            let hits = crate::modplatform::modrinth::Modrinth.search_typed(
                &query.text,
                Some(kind.modrinth_type()),
                query.loader.as_deref(),
                query.game_version.as_deref(),
            )?;
            Ok(hits
                .into_iter()
                .map(|h| PlatformHit {
                    platform,
                    kind,
                    id: h.id,
                    slug: h.slug,
                    title: h.title,
                    description: h.description,
                    downloads: h.downloads,
                    extra: String::new(),
                })
                .collect())
        }
        Platform::CurseForge => {
            use crate::modplatform::curseforge as cf;
            let loader = query
                .loader
                .as_deref()
                .and_then(cf::loader_type)
                .filter(|_| kind == ContentKind::Mod);
            let hits = cf::CurseForge.search_filtered(
                &query.text,
                kind.curseforge_class(),
                query.game_version.as_deref(),
                loader,
                0,
            )?;
            Ok(hits
                .into_iter()
                .map(|h| PlatformHit {
                    platform,
                    kind,
                    id: h.id,
                    slug: h.slug,
                    title: h.title,
                    description: h.description,
                    downloads: h.downloads,
                    extra: String::new(),
                })
                .collect())
        }
        Platform::Ftb => {
            let packs = crate::modplatform::ftb::Ftb.search(&query.text)?;
            Ok(packs
                .into_iter()
                .map(|p| {
                    let version = p.latest_version().map(|v| v.id).unwrap_or(0);
                    PlatformHit {
                        platform,
                        kind: ContentKind::Modpack,
                        id: p.id.to_string(),
                        slug: p.id.to_string(),
                        title: p.name.clone(),
                        description: p.synopsis.clone(),
                        downloads: p.installs,
                        extra: version.to_string(),
                    }
                })
                .collect())
        }
        Platform::Technic => {
            let packs = crate::modplatform::technic::Technic.search(&query.text)?;
            Ok(packs
                .into_iter()
                .map(|p| PlatformHit {
                    platform,
                    kind: ContentKind::Modpack,
                    id: p.slug().to_string(),
                    slug: p.slug().to_string(),
                    title: p.title().to_string(),
                    description: p.minecraft.clone(),
                    downloads: p.installs,
                    extra: p.minecraft.clone(),
                })
                .collect())
        }
        Platform::AtLauncher => {
            let packs = crate::modplatform::atlauncher::AtLauncher.search(&query.text)?;
            Ok(packs
                .into_iter()
                .map(|p| {
                    let version = p
                        .latest_version()
                        .map(|v| v.version.clone())
                        .unwrap_or_default();
                    let mc = p
                        .latest_version()
                        .map(|v| v.minecraft.clone())
                        .unwrap_or_default();
                    PlatformHit {
                        platform,
                        kind: ContentKind::Modpack,
                        id: p.safe_name(),
                        slug: p.safe_name(),
                        title: p.name.clone(),
                        description: p.description.clone(),
                        downloads: 0,
                        extra: format!("{version}|{mc}"),
                    }
                })
                .collect())
        }
        Platform::Packwiz => Err(
            "Packwiz packs are installed from a URL or file (use `install-packwiz <url>`)".into(),
        ),
    }
}

/// Result of an install: a summary plus any loader components the pack
/// implies (so the caller can populate an instance's component pack).
#[derive(Debug, Clone, Default)]
pub struct InstallOutcome {
    pub message: String,
    pub components: Vec<(String, String)>,
}

/// Install a search hit.
///
/// * mods / resource packs / shaders → `dest` is the directory to place the file
/// * modpacks → `dest` is the instance directory to populate
///
/// Returns a human-readable summary.
pub fn install(
    hit: &PlatformHit,
    dest: &Path,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<InstallOutcome, String> {
    match hit.platform {
        Platform::Modrinth => {
            let mr = crate::modplatform::modrinth::Modrinth;
            let versions = mr.versions(&hit.slug, loader, game_version)?;
            let version = versions.first().ok_or("no version matches this instance")?;
            if hit.kind == ContentKind::Modpack {
                let tmp = tmp_path("mrpack", &hit.slug);
                let file = mr.download_version_file(version, tmp.parent().unwrap())?;
                let manifest = crate::pack::mrpack::install(&file, dest)?;
                let _ = std::fs::remove_file(&file);
                return Ok(InstallOutcome {
                    message: format!(
                        "installed Modrinth pack {} ({} files)",
                        manifest.name,
                        manifest.files.len()
                    ),
                    components: pack_components(&manifest.dependencies),
                });
            }
            let path = mr.download_version_file(version, dest)?;
            Ok(InstallOutcome {
                message: format!("installed {}", path.display()),
                components: vec![],
            })
        }
        Platform::CurseForge => {
            let cf = crate::modplatform::curseforge::CurseForge;
            let loader_type = loader.and_then(crate::modplatform::curseforge::loader_type);
            if hit.kind == ContentKind::Modpack {
                let dir = tmp_path("cfpack", &hit.slug);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let file = cf
                    .latest_file(&hit.id, game_version, None)?
                    .ok_or("no files available")?;
                let url = file
                    .resolved_url()
                    .ok_or("pack file has no download URL")?;
                let pack = dir.join(&file.file_name);
                crate::net::download_verified(&url, &pack, None)?;
                let manifest = crate::pack::curseforge::install(&pack, dest)?;
                let _ = std::fs::remove_dir_all(&dir);
                return Ok(InstallOutcome {
                    message: format!(
                        "installed CurseForge pack {} ({} files)",
                        manifest.name,
                        manifest.files.len()
                    ),
                    components: pack_components(&manifest.dependencies),
                });
            }
            let path = cf.install_mod(&hit.id, game_version, loader_type, dest)?;
            Ok(InstallOutcome {
                message: format!("installed {}", path.display()),
                components: vec![],
            })
        }
        Platform::Ftb => {
            let version = hit.extra.parse::<u64>().ok();
            let manifest =
                crate::modplatform::ftb::Ftb.install(hit.id.parse().unwrap_or(0), version, dest)?;
            Ok(InstallOutcome {
                message: format!(
                    "installed FTB pack {} ({} files)",
                    manifest.name,
                    manifest.files.len()
                ),
                components: manifest.components(),
            })
        }
        Platform::Technic => {
            let game_dir = dest.join("minecraft");
            let (message, components) =
                crate::modplatform::technic::Technic.install(&hit.id, &game_dir)?;
            Ok(InstallOutcome {
                message,
                components,
            })
        }
        Platform::AtLauncher => {
            let version = hit
                .extra
                .split('|')
                .next()
                .filter(|s| !s.is_empty())
                .ok_or("pack version unknown")?;
            let manifest = crate::modplatform::atlauncher::AtLauncher
                .manifest(&hit.id, version)?;
            let components = manifest.components();
            let game_dir = dest.join("minecraft");
            let (n, blocked, failed) =
                crate::modplatform::atlauncher::AtLauncher.install(&hit.id, version, &game_dir)?;
            let mut summary = format!("installed ATLauncher pack {} ({} files)", hit.title, n);
            if !blocked.is_empty() {
                summary.push_str(&format!(
                    "; {} mods need manual download: {}",
                    blocked.len(),
                    blocked.join(", ")
                ));
            }
            if !failed.is_empty() {
                summary.push_str(&format!("; {} files failed to download", failed.len()));
            }
            Ok(InstallOutcome {
                message: summary,
                components,
            })
        }
        Platform::Packwiz => Err("use `install-packwiz <url>` for Packwiz packs".into()),
    }
}

/// Convert pack dependencies into `(prism uid, version)` components.
pub fn pack_components(deps: &crate::pack::PackDependencies) -> Vec<(String, String)> {
    let mut v = Vec::new();
    if let Some(mc) = &deps.minecraft {
        v.push(("net.minecraft".to_string(), mc.clone()));
    }
    if let Some(f) = &deps.fabric_loader {
        v.push(("net.fabricmc.fabric-loader".to_string(), f.clone()));
    }
    if let Some(f) = &deps.forge {
        v.push(("net.minecraftforge".to_string(), f.clone()));
    }
    if let Some(f) = &deps.neo_forge {
        v.push(("net.neoforged".to_string(), f.clone()));
    }
    if let Some(f) = &deps.quilt_loader {
        v.push(("org.quiltmc.quilt-loader".to_string(), f.clone()));
    }
    v
}

fn tmp_path(kind: &str, slug: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("prism-{kind}-{}-{slug}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_labels_and_aliases() {
        assert_eq!(Platform::from_name("cf"), Some(Platform::CurseForge));
        assert_eq!(Platform::from_name("Modrinth"), Some(Platform::Modrinth));
        assert_eq!(Platform::from_name("nope"), None);
        assert_eq!(Platform::CurseForge.label(), "CurseForge");
    }

    #[test]
    fn availability_by_kind() {
        let mods = Platform::for_kind(ContentKind::Mod);
        assert!(mods.contains(&Platform::Modrinth));
        assert!(mods.contains(&Platform::CurseForge));
        assert!(!mods.contains(&Platform::Technic));

        let packs = Platform::for_kind(ContentKind::Modpack);
        assert!(packs.contains(&Platform::Technic));
        assert!(packs.contains(&Platform::AtLauncher));
    }

    #[test]
    fn kind_maps_to_platform_types() {
        assert_eq!(ContentKind::Modpack.modrinth_type(), "modpack");
        assert_eq!(
            ContentKind::Modpack.curseforge_class(),
            crate::modplatform::curseforge::CLASS_MODPACK
        );
    }

    #[test]
    fn search_query_filters_empty_values() {
        let q = SearchQuery::new("jei")
            .with_game_version(Some("".into()))
            .with_loader(Some("fabric".into()));
        assert_eq!(q.game_version, None);
        assert_eq!(q.loader.as_deref(), Some("fabric"));
    }
}
