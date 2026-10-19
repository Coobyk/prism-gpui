//! Packwiz pack support (`pack.toml` + `index.toml` + `.pw.toml` metafiles).
//! Mirrors launcher/modplatform/packwiz.
//!
//! A packwiz pack is a directory tree whose `index.toml` lists every file.
//! Entries marked `metafile = true` point at a `.pw.toml` file describing how
//! to obtain the real file; other entries are raw includes relative to the
//! pack's source.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct PackToml {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pack_format: Option<String>,
    #[serde(default)]
    pub index: Option<String>,
    #[serde(default)]
    pub versions: BTreeMap<String, String>,
}

impl PackToml {
    pub fn minecraft(&self) -> Option<&str> {
        self.versions.get("minecraft").map(|s| s.as_str())
    }

    /// Loader components as `(prism uid, version)`.
    pub fn components(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (k, v) in &self.versions {
            let uid = match k.to_ascii_lowercase().as_str() {
                "minecraft" => "net.minecraft",
                "forge" => "net.minecraftforge",
                "fabric" => "net.fabricmc.fabric-loader",
                "neoforge" => "net.neoforged",
                "quilt" => "org.quiltmc.quilt-loader",
                _ => continue,
            };
            out.push((uid.to_string(), v.clone()));
        }
        out
    }
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct IndexToml {
    #[serde(default)]
    pub hash_format: String,
    #[serde(default)]
    pub files: Vec<IndexFile>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct IndexFile {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub hash_format: String,
    #[serde(default)]
    pub metafile: bool,
    #[serde(default)]
    pub preserve: bool,
    #[serde(default)]
    pub alias: Option<String>,
}

/// A `.pw.toml` metafile.
#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct MetaFile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub side: String,
    #[serde(default)]
    pub download: Option<Download>,
    #[serde(default)]
    pub update: Option<Update>,
    #[serde(default)]
    pub option: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct Download {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub hash_format: String,
    #[serde(default)]
    pub mode: String,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct Update {
    #[serde(default)]
    pub modrinth: Option<UpdateSource>,
    #[serde(default)]
    pub curseforge: Option<UpdateSource>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateSource {
    #[serde(default)]
    pub mod_id: String,
    #[serde(default)]
    pub version: String,
}

/// A file resolved and ready to download.
#[derive(Debug, Clone)]
pub struct ResolvedFile {
    /// Path relative to the pack's game directory.
    pub dest: String,
    pub url: String,
    pub hash: String,
    pub hash_format: String,
}

pub fn read_pack(dir: &Path) -> Result<PackToml, String> {
    let text = std::fs::read_to_string(dir.join("pack.toml")).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

pub fn read_index(dir: &Path, index_name: &str) -> Result<IndexToml, String> {
    let text =
        std::fs::read_to_string(dir.join(index_name)).map_err(|e| e.to_string())?;
    toml::from_str(&text).map_err(|e| e.to_string())
}

/// Directory a metafile path (e.g. `mods/foo.pw.toml`) refers to, and the
/// base file name to use when the metafile names no filename.
pub fn dest_for_metafile(metafile_path: &str, metafile: &MetaFile) -> Option<String> {
    let dir = Path::new(metafile_path).parent()?.to_string_lossy().to_string();
    let name = if !metafile.filename.is_empty() {
        metafile.filename.clone()
    } else if !metafile.name.is_empty() {
        metafile.name.clone()
    } else {
        Path::new(metafile_path)
            .file_name()?
            .to_string_lossy()
            .trim_end_matches(".pw.toml")
            .to_string()
    };
    if dir.is_empty() {
        Some(name)
    } else {
        Some(format!("{dir}/{name}"))
    }
}

/// Resolve a metafile's download source, following a Modrinth/CurseForge
/// update reference when the metafile has no direct URL.
pub fn resolve_metafile(metafile: &MetaFile) -> Result<(String, String, String), String> {
    if let Some(dl) = &metafile.download {
        if !dl.url.is_empty() {
            return Ok((dl.url.clone(), dl.hash.clone(), dl.hash_format.clone()));
        }
    }
    if let Some(update) = &metafile.update {
        if let Some(m) = &update.modrinth {
            if !m.version.is_empty() {
                let info = crate::modplatform::modrinth::Modrinth
                    .version_by_id(&m.version)?;
                if let Some(f) = info.files.iter().find(|f| f.primary).or(info.files.first()) {
                    return Ok((f.url.clone(), f.hashes.sha1.clone(), "sha1".to_string()));
                }
            } else if !m.mod_id.is_empty() {
                let versions = crate::modplatform::modrinth::Modrinth
                    .versions(&m.mod_id, None, None)?;
                if let Some(v) = versions.first() {
                    if let Some(f) = v.files.iter().find(|f| f.primary).or(v.files.first()) {
                        return Ok((f.url.clone(), f.hashes.sha1.clone(), "sha1".to_string()));
                    }
                }
            }
        }
    }
    Err("metafile has no resolvable download".to_string())
}

/// Install a packwiz pack from an extracted directory into `game_dir`.
/// Returns `(installed, skipped)`.
pub fn install_from_dir(
    pack_dir: &Path,
    game_dir: &Path,
    side: &str,
) -> Result<(usize, Vec<String>), String> {
    let pack = read_pack(pack_dir)?;
    let index_name = pack.index.clone().unwrap_or_else(|| "index.toml".to_string());
    let index = read_index(pack_dir, &index_name)?;
    let mut installed = 0usize;
    let mut skipped = Vec::new();
    for entry in &index.files {
        if entry.metafile {
            let meta_path = pack_dir.join(&entry.file);
            let Ok(text) = std::fs::read_to_string(&meta_path) else {
                skipped.push(entry.file.clone());
                continue;
            };
            let meta: MetaFile = match toml::from_str(&text) {
                Ok(m) => m,
                Err(_) => {
                    skipped.push(entry.file.clone());
                    continue;
                }
            };
            if !side_allowed(&meta.side, side) {
                skipped.push(format!("{} (side {})", entry.file, meta.side));
                continue;
            }
            let (url, hash, format) = match resolve_metafile(&meta) {
                Ok(v) => v,
                Err(e) => {
                    skipped.push(format!("{}: {e}", entry.file));
                    continue;
                }
            };
            let Some(dest_rel) = dest_for_metafile(&entry.file, &meta) else {
                skipped.push(entry.file.clone());
                continue;
            };
            let dest = crate::fs::safe_join(game_dir, &dest_rel)
                .ok_or_else(|| format!("unsafe path in packwiz index: {dest_rel}"))?;
            let alg = if format.is_empty() { None } else { Some(format.as_str()) };
            let exp = if hash.is_empty() { None } else { Some(hash.as_str()) };
            crate::net::download_verified_hash(&url, &dest, alg, exp)?;
            installed += 1;
        }
    }
    Ok((installed, skipped))
}

fn side_allowed(file_side: &str, want: &str) -> bool {
    if file_side.is_empty() || file_side.eq_ignore_ascii_case("both") {
        return true;
    }
    file_side.eq_ignore_ascii_case(want)
}

/// Download a packwiz pack zip and install it.
pub fn install_from_zip(
    zip_path: &Path,
    game_dir: &Path,
    side: &str,
) -> Result<(usize, Vec<String>), String> {
    let tmp = std::env::temp_dir().join(format!("prism-packwiz-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    crate::fs::extract_zip(zip_path, &tmp, true)?;
    // Find the directory containing pack.toml (may be nested).
    let pack_dir = if tmp.join("pack.toml").is_file() {
        tmp.clone()
    } else {
        crate::fs::list_files(&tmp)
            .into_iter()
            .find(|p| p.file_name().map(|n| n == "pack.toml").unwrap_or(false))
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or("no pack.toml found in archive")?
    };
    let result = install_from_dir(&pack_dir, game_dir, side);
    let _ = std::fs::remove_dir_all(&tmp);
    result
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
        assert_eq!(p.minecraft(), Some("1.20.1"));
        assert!(p.components().iter().any(|(u, _)| u == "net.fabricmc.fabric-loader"));
    }

    #[test]
    fn parses_index_and_metafile() {
        let idx: IndexToml = toml::from_str(
            r#"hash-format = "sha256"
[[files]]
file = "mods/foo.pw.toml"
hash = "abc"
metafile = true
"#,
        )
        .unwrap();
        assert_eq!(idx.files.len(), 1);
        assert!(idx.files[0].metafile);
        assert_eq!(idx.files[0].hash_format, "");

        let meta: MetaFile = toml::from_str(
            r#"name = "Foo"
filename = "foo-1.0.jar"
side = "both"
[download]
url = "https://example.com/foo.jar"
hash-format = "sha512"
hash = "deadbeef"
"#,
        )
        .unwrap();
        assert_eq!(meta.filename, "foo-1.0.jar");
        assert_eq!(meta.download.as_ref().unwrap().hash_format, "sha512");
        assert_eq!(
            dest_for_metafile("mods/foo.pw.toml", &meta).as_deref(),
            Some("mods/foo-1.0.jar")
        );
    }

    #[test]
    fn resolves_direct_metafile_download() {
        let meta = MetaFile {
            download: Some(Download {
                url: "https://e/x.jar".into(),
                hash: "abc".into(),
                hash_format: "sha1".into(),
                mode: String::new(),
            }),
            ..Default::default()
        };
        let (url, hash, fmt) = resolve_metafile(&meta).unwrap();
        assert_eq!(url, "https://e/x.jar");
        assert_eq!(hash, "abc");
        assert_eq!(fmt, "sha1");
    }

    #[test]
    fn rejects_metafile_without_source() {
        let meta = MetaFile::default();
        assert!(resolve_metafile(&meta).is_err());
    }
}
