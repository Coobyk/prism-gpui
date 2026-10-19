//! Modrinth modpack (.mrpack) import/export.

use crate::fs;
use crate::net;
use crate::pack::{PackDependencies, PackFile, PackManifest};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Index {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    game: String,
    #[serde(rename = "versionId")]
    version_id: String,
    name: String,
    #[serde(default)]
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct MrFile {
    path: String,
    #[serde(default)]
    hashes: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: u64,
}

pub fn manifest_from_pack(pack: &Path) -> Result<PackManifest, String> {
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut index_text = String::new();
    {
        let mut entry = zip
            .by_name("modrinth.index.json")
            .map_err(|_| "not a .mrpack (missing modrinth.index.json)")?;
        std::io::Read::read_to_string(&mut entry, &mut index_text).map_err(|e| e.to_string())?;
    }
    let index: Index = serde_json::from_str(&index_text).map_err(|e| e.to_string())?;
    let mut deps = PackDependencies::default();
    for (k, v) in &index.dependencies {
        match k.as_str() {
            "minecraft" => deps.minecraft = Some(v.clone()),
            "fabric-loader" => deps.fabric_loader = Some(v.clone()),
            "forge" => deps.forge = Some(v.clone()),
            "quilt-loader" => deps.quilt_loader = Some(v.clone()),
            "neoforge" => deps.neo_forge = Some(v.clone()),
            _ => {}
        }
    }
    Ok(PackManifest {
        name: index.name,
        version: index.version_id,
        dependencies: deps,
        files: index
            .files
            .into_iter()
            .map(|f| PackFile {
                path: f.path,
                sha1: f.hashes.get("sha1").cloned(),
                downloads: f.downloads,
                file_size: f.file_size,
            })
            .collect(),
    })
}

/// Install a .mrpack into `instance_dir` (creating it), downloading pack files.
pub fn install(pack: &Path, instance_dir: &Path) -> Result<PackManifest, String> {
    let manifest = manifest_from_pack(pack)?;
    let mc_dir = instance_dir.join("minecraft");
    std::fs::create_dir_all(&mc_dir).map_err(|e| e.to_string())?;

    for f in &manifest.files {
        let dest = fs::safe_join(&mc_dir, &f.path)
            .ok_or_else(|| format!("unsafe path in pack: {}", f.path))?;
        if dest.is_file() {
            continue;
        }
        let Some(url) = f.downloads.first() else { continue };
        net::download_verified(url, &dest, f.sha1.as_deref())?;
    }

    // Extract overrides/ and client-overrides/.
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        let rel = name
            .strip_prefix("overrides/")
            .or_else(|| name.strip_prefix("client-overrides/"));
        if let Some(rel) = rel {
            if rel.is_empty() {
                continue;
            }
            let dest = fs::safe_join(&mc_dir, rel)
                .ok_or_else(|| format!("unsafe path in pack: {rel}"))?;
            if entry.is_dir() {
                std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            } else {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(manifest)
}

/// Export an instance as a .mrpack (overrides only; mods listed by hash).
pub fn export(instance_dir: &Path, out: &Path, name: &str, version: &str) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut files: Vec<MrFile> = Vec::new();
    let mc_dir = instance_dir.join("minecraft");
    let mods_dir = mc_dir.join("mods");
    if mods_dir.is_dir() {
        for path in fs::list_files(&mods_dir) {
            let rel = path
                .strip_prefix(&mc_dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let sha1 = net::sha1_file(&path);
            files.push(MrFile {
                path: rel,
                hashes: sha1
                    .into_iter()
                    .map(|h| ("sha1".to_string(), h))
                    .collect(),
                downloads: vec![],
                file_size: path.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }

    let mut dependencies = std::collections::BTreeMap::new();
    if let Some(pack) = crate::meta::read_component_pack(instance_dir) {
        for c in &pack.components {
            match c.uid.as_str() {
                "net.minecraft" => {
                    dependencies.insert("minecraft".to_string(), c.version.clone());
                }
                "net.fabricmc.fabric-loader" => {
                    dependencies.insert("fabric-loader".to_string(), c.version.clone());
                }
                "org.quiltmc.quilt-loader" => {
                    dependencies.insert("quilt-loader".to_string(), c.version.clone());
                }
                "net.minecraftforge" => {
                    dependencies.insert("forge".to_string(), c.version.clone());
                }
                "net.neoforged" => {
                    dependencies.insert("neoforge".to_string(), c.version.clone());
                }
                _ => {}
            }
        }
    }
    let index = Index {
        format_version: 1,
        game: "minecraft".into(),
        version_id: version.into(),
        name: name.into(),
        files,
        dependencies,
    };
    let text = serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?;
    zip.start_file("modrinth.index.json", opts)
        .map_err(|e| e.to_string())?;
    zip.write_all(text.as_bytes()).map_err(|e| e.to_string())?;

    // Bundle configs as overrides.
    for sub in ["config", "defaultconfigs", "resourcepacks", "shaderpacks"] {
        let dir = mc_dir.join(sub);
        if dir.is_dir() {
            for path in fs::list_files(&dir) {
                let rel = path.strip_prefix(&mc_dir).map_err(|e| e.to_string())?;
                let zip_name = format!("overrides/{}", rel.to_string_lossy().replace('\\', "/"));
                zip.start_file(zip_name, opts).map_err(|e| e.to_string())?;
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                zip.write_all(&data).map_err(|e| e.to_string())?;
            }
        }
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let inst = td.path().join("inst");
        let mc = inst.join("minecraft").join("mods");
        std::fs::create_dir_all(&mc).unwrap();
        std::fs::write(mc.join("a.jar"), b"hello").unwrap();
        let out = td.path().join("pack.mrpack");
        export(&inst, &out, "Test", "1.0").unwrap();
        let m = manifest_from_pack(&out).unwrap();
        assert_eq!(m.name, "Test");
        assert_eq!(m.files.len(), 1);
        assert!(m.files[0].sha1.is_some());
    }
}
