//! Modpack import/export. Mirrors launcher/InstanceImportTask, InstanceCopyTask,
//! launcher/minecraft/PackProfile and the various modplatform pack formats.

pub mod curseforge;
pub mod mmc;
pub mod mrpack;

use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct PackDependencies {
    pub minecraft: Option<String>,
    pub fabric_loader: Option<String>,
    pub forge: Option<String>,
    pub quilt_loader: Option<String>,
    pub neo_forge: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PackFile {
    pub path: String,
    pub sha1: Option<String>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct PackManifest {
    pub name: String,
    pub version: String,
    pub dependencies: PackDependencies,
    pub files: Vec<PackFile>,
}

/// Guess the pack format from a file name/extension.
pub fn detect_format(path: &Path) -> &'static str {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.ends_with(".mrpack") {
        "mrpack"
    } else if name.ends_with(".zip") {
        "zip"
    } else {
        "unknown"
    }
}

/// Detect a pack's real format by inspecting the archive contents, which is
/// more reliable than the extension (`.mrpack` files are also zip archives).
///
/// Returns one of `mrpack`, `curseforge`, `mmc` or `unknown`.
pub fn detect_archive_format(path: &Path) -> Result<&'static str, String> {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !name.ends_with(".zip") && !name.ends_with(".mrpack") {
        return Ok("unknown");
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut saw_manifest = false;
    for i in 0..zip.len() {
        let Ok(entry) = zip.by_index(i) else { continue };
        let entry_name = entry.name().to_string();
        let base = entry_name.rsplit('/').next().unwrap_or(&entry_name);
        if base == "modrinth.index.json" {
            return Ok("mrpack");
        }
        if base == "instance.cfg" {
            return Ok("mmc");
        }
        if base == "manifest.json" {
            saw_manifest = true;
        }
    }
    if saw_manifest {
        return Ok("curseforge");
    }
    Ok("unknown")
}

/// Result of importing a pack archive as a new instance.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub dir: std::path::PathBuf,
    pub name: String,
    pub format: &'static str,
    pub summary: String,
    pub components: Vec<(String, String)>,
}

/// Import any supported pack archive (`.mrpack`, CurseForge or Prism/MultiMC
/// zip) into a fresh instance under `data_root`.
///
/// This is what drag-and-drop and the CLI import command use, so it must not
/// depend on a pre-existing instance.
pub fn import_archive(
    archive: &Path,
    data_root: &Path,
    name_hint: Option<&str>,
) -> Result<ImportResult, String> {
    let format = detect_archive_format(archive)?;
    if format == "unknown" {
        return Err(format!(
            "{} is not a recognised pack (expected .mrpack, a Modrinth/CurseForge zip, or a Prism instance zip)",
            archive.display()
        ));
    }
    let instances_root = crate::instance::instances_dir(data_root);
    std::fs::create_dir_all(&instances_root).map_err(|e| e.to_string())?;
    let stem = archive
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "imported pack".to_string());
    let wanted = sanitize_instance_name(name_hint.unwrap_or(&stem));
    let name = unique_instance_name(&instances_root, &wanted);

    if format == "mmc" {
        let dir = mmc::import(archive, &instances_root, &name)?;
        let components = crate::meta::read_component_pack(&dir)
            .map(|p| {
                p.components
                    .into_iter()
                    .map(|c| (c.uid, c.version))
                    .collect()
            })
            .unwrap_or_default();
        return Ok(ImportResult {
            dir,
            name,
            format,
            summary: "imported Prism instance archive".to_string(),
            components,
        });
    }

    let dir = instances_root.join(&name);
    std::fs::create_dir_all(dir.join("minecraft")).map_err(|e| e.to_string())?;
    write_instance_config(data_root, &dir, &name)?;
    let manifest = if format == "mrpack" {
        mrpack::install(archive, &dir)?
    } else {
        curseforge::install(archive, &dir)?
    };
    let components =
        crate::modplatform::platform::pack_components(&manifest.dependencies);
    for (uid, version) in &components {
        let _ = crate::meta::set_component(&dir, uid, version);
    }
    Ok(ImportResult {
        dir,
        name,
        format,
        summary: format!(
            "imported {} {} ({} files)",
            manifest.name,
            manifest.version,
            manifest.files.len()
        ),
        components,
    })
}

/// Write a minimal `instance.cfg` for a newly imported instance.
pub fn write_instance_config(data_root: &Path, dir: &Path, name: &str) -> Result<(), String> {
    let settings = crate::settings::GlobalSettings::load(data_root);
    let cfg = format!(
        "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nname={name}\nMinMemAlloc={}\nMaxMemAlloc={}\n",
        settings.min_mem(),
        settings.max_mem()
    );
    std::fs::write(dir.join("instance.cfg"), cfg).map_err(|e| e.to_string())
}

/// Make a name safe for use as an instance directory.
pub fn sanitize_instance_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "Imported Pack".to_string()
    } else {
        trimmed
    }
}

fn unique_instance_name(instances_root: &Path, wanted: &str) -> String {
    if !instances_root.join(wanted).exists() {
        return wanted.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{wanted} ({n})");
        if !instances_root.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{wanted}-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, opts).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn detects_archive_formats_from_contents() {
        let td = tempfile::tempdir().unwrap();

        let mrpack = td.path().join("pack.zip");
        write_zip(&mrpack, &[("modrinth.index.json", b"{}")]);
        assert_eq!(detect_archive_format(&mrpack).unwrap(), "mrpack");

        let cf = td.path().join("cf.zip");
        write_zip(&cf, &[("manifest.json", b"{}")]);
        assert_eq!(detect_archive_format(&cf).unwrap(), "curseforge");

        let mmc = td.path().join("mmc.zip");
        write_zip(&mmc, &[("instance.cfg", b"name=x\n")]);
        assert_eq!(detect_archive_format(&mmc).unwrap(), "mmc");

        let unknown = td.path().join("x.zip");
        write_zip(&unknown, &[("readme.txt", b"hi")]);
        assert_eq!(detect_archive_format(&unknown).unwrap(), "unknown");
    }

    #[test]
    fn sanitises_and_deduplicates_instance_names() {
        assert_eq!(sanitize_instance_name("My/Pack: 1"), "My_Pack_ 1");
        assert_eq!(sanitize_instance_name("..."), "Imported Pack");
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(td.path().join("Pack")).unwrap();
        assert_eq!(unique_instance_name(td.path(), "Pack"), "Pack (2)");
        std::fs::create_dir_all(td.path().join("Pack (2)")).unwrap();
        assert_eq!(unique_instance_name(td.path(), "Pack"), "Pack (3)");
    }

    #[test]
    fn imports_mrpack_as_new_instance() {
        let td = tempfile::tempdir().unwrap();
        let data_root = td.path();
        let pack = td.path().join("Test Pack.mrpack");
        let index = r#"{
            "formatVersion": 1,
            "game": "minecraft",
            "versionId": "1.0.0",
            "name": "Test Pack",
            "files": [],
            "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.0"}
        }"#;
        write_zip(
            &pack,
            &[
                ("modrinth.index.json", index.as_bytes()),
                ("overrides/config/example.txt", b"hello"),
            ],
        );

        let result = import_archive(&pack, data_root, None).unwrap();
        assert_eq!(result.format, "mrpack");
        assert_eq!(result.name, "Test Pack");
        assert!(result.dir.join("instance.cfg").is_file());
        assert_eq!(
            std::fs::read_to_string(result.dir.join("minecraft/config/example.txt")).unwrap(),
            "hello"
        );
        assert!(result
            .components
            .contains(&("net.minecraft".to_string(), "1.20.1".to_string())));
        assert!(result
            .components
            .contains(&("net.fabricmc.fabric-loader".to_string(), "0.15.0".to_string())));
        // Components are persisted into the instance's component pack.
        let persisted = crate::meta::read_component_pack(&result.dir).unwrap();
        assert!(persisted.components.iter().any(|c| c.uid == "net.minecraft"));

        // Importing the same archive again gets a distinct directory.
        let again = import_archive(&pack, data_root, None).unwrap();
        assert_eq!(again.name, "Test Pack (2)");
    }

    #[test]
    fn rejects_unknown_archive() {
        let td = tempfile::tempdir().unwrap();
        let bad = td.path().join("bad.zip");
        write_zip(&bad, &[("nope.txt", b"x")]);
        let err = import_archive(&bad, td.path(), None).unwrap_err();
        assert!(err.contains("not a recognised pack"));
    }
}
