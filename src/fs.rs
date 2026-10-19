//! Filesystem helpers. Mirrors launcher/FileSystem, MMCZip, PathUtils.

use std::path::{Component, Path, PathBuf};

/// Copy a directory tree recursively. Skips the given relative sub-paths.
pub fn copy_dir_recursive(src: &Path, dst: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
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
