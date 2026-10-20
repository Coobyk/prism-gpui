//! Filesystem helpers. Mirrors launcher/FileSystem, MMCZip, PathUtils.

use std::path::{Component, Path, PathBuf};

/// Copy a directory tree recursively. Skips the given relative sub-paths.
pub fn copy_dir_recursive(src: &Path, dst: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        if skip.iter().any(|s| rel.starts_with(s)) {
            continue;
        }
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Reject paths that escape a root (defends against zip-slip).
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    let candidate = Path::new(&rel);
    let mut out = PathBuf::from(root);
    for comp in candidate.components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

/// Recursively collect files (not dirs) under a directory.
pub fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(list_files(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Extract a zip archive into `dest`, rejecting entries that escape it.
/// When `strip_top` is set, a single shared top-level directory is removed
/// (packs that nest everything under one folder).
pub fn extract_zip(zip_path: &Path, dest: &Path, strip_top: bool) -> Result<usize, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    extract_zip_reader(std::io::BufReader::new(file), dest, strip_top)
}

/// Extract a zip from any reader.
pub fn extract_zip_reader<R: std::io::Read + std::io::Seek>(
    reader: R,
    dest: &Path,
    strip_top: bool,
) -> Result<usize, String> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let strip = if strip_top {
        common_top_dir(&mut zip)
    } else {
        None
    };
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let mut name = entry.name().to_string();
        if let Some(top) = &strip {
            name = name
                .strip_prefix(top.as_str())
                .unwrap_or(&name)
                .trim_start_matches('/')
                .to_string();
        }
        if name.is_empty() {
            continue;
        }
        let Some(target) = safe_join(dest, &name) else {
            return Err(format!("unsafe path in archive: {name}"));
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

/// If every entry in the archive shares one top-level directory, return it.
fn common_top_dir<R: std::io::Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Option<String> {
    let mut top: Option<String> = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).ok()?;
        let name = entry.name().to_string();
        let first = name.split('/').next()?.to_string();
        if !name.contains('/') {
            return None; // a root-level file means no stripping
        }
        match &top {
            None => top = Some(first),
            Some(t) if *t == first => {}
            _ => return None,
        }
    }
    top
}

/// Write a zip archive from the files under `src`, storing `rel`-prefixed names.
pub fn zip_dir(src: &Path, out: &Path, prefix: &str) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(out).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for path in list_files(src) {
        let rel = path.strip_prefix(src).map_err(|e| e.to_string())?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(format!("{prefix}{rel}"), opts)
            .map_err(|e| e.to_string())?;
        let data = std::fs::read(&path).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_join_blocks_escape() {
        let root = Path::new("/tmp/root");
        assert!(safe_join(root, "a/b.txt").is_some());
        assert!(safe_join(root, "../evil").is_none());
        assert!(safe_join(root, "/etc/passwd").is_none());
    }

    #[test]
    fn extract_zip_roundtrip_and_strips_top_dir() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("src");
        std::fs::create_dir_all(src.join("top/inner")).unwrap();
        std::fs::write(src.join("top/a.txt"), "a").unwrap();
        std::fs::write(src.join("top/inner/b.txt"), "b").unwrap();
        let zip_path = td.path().join("p.zip");
        zip_dir(&src, &zip_path, "").unwrap();

        let dest = td.path().join("out");
        let n = extract_zip(&zip_path, &dest, true).unwrap();
        assert_eq!(n, 2);
        // The shared "top" directory is stripped.
        assert!(dest.join("a.txt").is_file());
        assert!(dest.join("inner/b.txt").is_file());
    }

    #[test]
    fn extract_zip_rejects_escape() {
        use std::io::Write;
        let td = tempfile::tempdir().unwrap();
        let zip_path = td.path().join("evil.zip");
        let f = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zip.start_file("../evil.txt", opts).unwrap();
        zip.write_all(b"x").unwrap();
        zip.finish().unwrap();
        let dest = td.path().join("out");
        assert!(extract_zip(&zip_path, &dest, false).is_err());
    }

    #[test]
    fn copy_tree() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("a.txt"), "a").unwrap();
        std::fs::write(src.join("sub/b.txt"), "b").unwrap();
        let dst = td.path().join("dst");
        copy_dir_recursive(&src, &dst, &[]).unwrap();
        assert!(dst.join("a.txt").is_file());
        assert!(dst.join("sub/b.txt").is_file());
    }
}
