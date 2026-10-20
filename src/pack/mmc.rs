//! Prism/MultiMC instance zip import/export.

use crate::fs;
use std::io::Write;
use std::path::Path;

/// Export an instance as a Prism/MultiMC zip.
pub fn export(instance_dir: &Path, out: &Path) -> Result<(), String> {
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let skip = [
        "minecraft/logs",
        "minecraft/crash-reports",
        "minecraft/.cache",
    ];
    for path in fs::list_files(instance_dir) {
        let rel = path.strip_prefix(instance_dir).map_err(|e| e.to_string())?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if skip.iter().any(|s| rel_str.starts_with(s)) {
            continue;
        }
        zip.start_file(rel_str, opts).map_err(|e| e.to_string())?;
        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Import a Prism/MultiMC zip into a fresh instance directory named `name`.
/// Returns the created directory.
pub fn import(
    pack: &Path,
    instances_root: &Path,
    name: &str,
) -> Result<std::path::PathBuf, String> {
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let inst_dir = instances_root.join(name);
    std::fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;
    let mut found_cfg = false;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let entry_name = entry.name().to_string();
        // Some packs nest everything under a top-level folder; strip it.
        let rel = strip_top_dir(&entry_name);
        if rel.is_empty() {
            continue;
        }
        if rel == "instance.cfg" {
            found_cfg = true;
        }
        let dest =
            fs::safe_join(&inst_dir, rel).ok_or_else(|| format!("unsafe path in pack: {rel}"))?;
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
    if !found_cfg {
        return Err("archive has no instance.cfg".into());
    }
    Ok(inst_dir)
}

fn strip_top_dir(name: &str) -> &str {
    // If every entry shares a common top directory, drop it. Here we simply
    // strip a leading "<something>/" only when it is not a known top-level file.
    const KNOWN: &[&str] = &["instance.cfg", "mmc-pack.json", "minecraft", "pack.json"];
    for k in KNOWN {
        if name == *k || name.starts_with(&format!("{k}/")) {
            return name;
        }
    }
    match name.split_once('/') {
        Some((_, rest)) => rest,
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let inst = td.path().join("MyInst");
        std::fs::create_dir_all(inst.join("minecraft").join("mods")).unwrap();
        std::fs::write(inst.join("instance.cfg"), "[General]\n").unwrap();
        std::fs::write(inst.join("mmc-pack.json"), "{}").unwrap();
        std::fs::write(inst.join("minecraft/mods/x.jar"), b"x").unwrap();
        let out = td.path().join("inst.zip");
        export(&inst, &out).unwrap();
        let root = td.path().join("instances");
        let created = import(&out, &root, "Restored").unwrap();
        assert!(created.join("instance.cfg").is_file());
        assert!(created.join("minecraft/mods/x.jar").is_file());
    }
}
