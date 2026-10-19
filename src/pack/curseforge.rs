//! CurseForge modpack (manifest.json) import/export. Mirrors launcher/modplatform/flame.

use crate::fs;
use crate::pack::{PackDependencies, PackFile, PackManifest};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Manifest {
    minecraft: Minecraft,
    #[serde(rename = "manifestType", default)]
    manifest_type: String,
    #[serde(rename = "manifestVersion", default)]
    manifest_version: u32,
    name: String,
    version: String,
    #[serde(default)]
    files: Vec<CfFile>,
    #[serde(default)]
    overrides: String,
}

#[derive(Serialize, Deserialize)]
struct Minecraft {
    version: String,
    #[serde(rename = "modLoaders", default)]
    mod_loaders: Vec<Loader>,
}

#[derive(Serialize, Deserialize)]
struct Loader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Serialize, Deserialize)]
struct CfFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default)]
    required: bool,
}

pub fn manifest_from_pack(pack: &Path) -> Result<PackManifest, String> {
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut text = String::new();
    {
        let mut entry = zip
            .by_name("manifest.json")
            .map_err(|_| "not a CurseForge pack (missing manifest.json)")?;
        std::io::Read::read_to_string(&mut entry, &mut text).map_err(|e| e.to_string())?;
    }
    let m: Manifest = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut deps = PackDependencies {
        minecraft: Some(m.minecraft.version.clone()),
        ..Default::default()
    };
    for l in &m.minecraft.mod_loaders {
        let id = l.id.to_lowercase();
        if let Some(rest) = id.strip_prefix("fabric-") {
            deps.fabric_loader = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("forge-") {
            deps.forge = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("quilt-") {
            deps.quilt_loader = Some(rest.to_string());
        } else if let Some(rest) = id.strip_prefix("neoforge-") {
            deps.neo_forge = Some(rest.to_string());
        }
    }
    Ok(PackManifest {
        name: m.name,
        version: m.version,
        dependencies: deps,
        files: m
            .files
            .into_iter()
            .map(|f| PackFile {
                path: format!("mods/cf-{}-{}.jar", f.project_id, f.file_id),
                sha1: None,
                downloads: vec![],
                file_size: 0,
            })
            .collect(),
    })
}

/// Extract a CurseForge pack's overrides into `instance_dir/minecraft`.
pub fn extract_overrides(pack: &Path, instance_dir: &Path) -> Result<(), String> {
    let mc_dir = instance_dir.join("minecraft");
    std::fs::create_dir_all(&mc_dir).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let Some(rel) = name.strip_prefix("overrides/") else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        let dest = fs::safe_join(&mc_dir, rel)
            .ok_or_else(|| format!("unsafe path in pack: {rel}"))?;
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = dest.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Export an instance to a CurseForge-style zip (metadata + overrides).
pub fn export(instance_dir: &Path, out: &Path, name: &str, version: &str) -> Result<(), String> {
    let mc_dir = instance_dir.join("minecraft");
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let manifest = Manifest {
        minecraft: Minecraft {
            version: "unknown".into(),
            mod_loaders: vec![],
        },
        manifest_type: "minecraftModpack".into(),
        manifest_version: 1,
        name: name.into(),
        version: version.into(),
        files: vec![],
        overrides: "overrides".into(),
    };
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.start_file("manifest.json", opts).map_err(|e| e.to_string())?;
    zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

    for sub in ["config", "defaultconfigs", "mods", "resourcepacks", "shaderpacks"] {
        let dir = mc_dir.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for path in fs::list_files(&dir) {
            let rel = path.strip_prefix(&mc_dir).map_err(|e| e.to_string())?;
            let zip_name = format!("overrides/{}", rel.to_string_lossy().replace('\\', "/"));
            zip.start_file(zip_name, opts).map_err(|e| e.to_string())?;
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}
