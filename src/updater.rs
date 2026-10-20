//! Update checker and applier. Mirrors launcher/updater (GitHub releases).

use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    #[serde(default)]
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    /// GitHub's digest, e.g. `sha256:abc…` (present on newer releases).
    #[serde(default)]
    pub digest: Option<String>,
}

impl Asset {
    /// The `(algorithm, hex)` pair from GitHub's digest field, if present.
    pub fn checksum(&self) -> Option<(&str, &str)> {
        let d = self.digest.as_deref()?;
        let (alg, hex) = d.split_once(':')?;
        Some((alg, hex))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// The GitHub repository the launcher checks for its own updates.
pub const SELF_REPO: &str = "Coobyk/prism-gpui";

/// Fetch the latest Prism Launcher release (the upstream C++ launcher).
pub fn latest_release() -> Result<Release, String> {
    latest_release_from("PrismLauncher/PrismLauncher")
}

/// Fetch the latest release of this launcher's own repository.
pub fn latest_self_release() -> Result<Release, String> {
    latest_release_from(SELF_REPO)
}

/// Fetch the latest release of a GitHub repository.
///
/// A repository without releases answers with a 404, which is not an error
/// here: it simply means there is nothing to update to.
pub fn latest_release_from(repo: &str) -> Result<Release, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let body = match crate::net::get_string(&url) {
        Ok(body) => body,
        Err(e) if e.contains("404") => {
            return Err(format!("{repo} has no releases yet"));
        }
        Err(e) => return Err(e),
    };
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Compare two dotted versions; returns true if `b` is newer than `a`.
pub fn is_newer(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split(['.', '-', '+'])
            .filter_map(|p| p.parse::<u32>().ok())
            .collect()
    };
    let (av, bv) = (norm(a), norm(b));
    for i in 0..av.len().max(bv.len()) {
        let x = av.get(i).copied().unwrap_or(0);
        let y = bv.get(i).copied().unwrap_or(0);
        if y != x {
            return y > x;
        }
    }
    false
}

/// The platform keyword used in Prism's release asset names.
pub fn platform_keyword() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

/// Pick the best installable asset for this platform, preferring a plain
/// archive over an AppImage/other format we cannot self-update from.
pub fn select_asset(release: &Release) -> Option<&Asset> {
    let key = platform_keyword();
    let candidates: Vec<&Asset> = release
        .assets
        .iter()
        .filter(|a| a.name.contains(key) && !a.browser_download_url.is_empty())
        .collect();
    let score = |name: &str| -> i32 {
        let n = name.to_ascii_lowercase();
        if n.ends_with(".tar.gz") || n.ends_with(".zip") {
            3
        } else if n.ends_with(".appimage") {
            2
        } else if n.ends_with(".exe") || n.ends_with(".7z") {
            1
        } else {
            0
        }
    };
    candidates
        .into_iter()
        .max_by_key(|a| (score(&a.name), a.size))
}

/// Where the updater stages downloads.
pub fn staging_dir(data_root: &Path) -> PathBuf {
    data_root.join("update")
}

/// Download an asset into the staging directory, verifying its digest when
/// available. Returns the downloaded path.
pub fn download_asset(asset: &Asset, data_root: &Path) -> Result<PathBuf, String> {
    let dir = staging_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(&asset.name);
    let (alg, hex) = match asset.checksum() {
        Some((a, h)) => (Some(a), Some(h)),
        None => (None, None),
    };
    crate::net::download_verified_hash(&asset.browser_download_url, &dest, alg, hex)?;
    Ok(dest)
}

/// Replace `target_exe` with the binary inside `downloaded`.
///
/// Handles a bare binary/AppImage (copied directly) and `.zip`/`.tar.gz`
/// archives (the launcher binary is located inside and extracted). The current
/// executable is backed up as `<name>.old`. Returns the path written.
pub fn apply_update(downloaded: &Path, target_exe: &Path) -> Result<PathBuf, String> {
    let name = downloaded
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    // The extraction directory has to sit on the same filesystem as the
    // target so the final rename is atomic; fall back to the temp dir.
    let work = extraction_dir(target_exe).join("extract");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;

    let binary = if name.ends_with(".zip") {
        crate::fs::extract_zip(downloaded, &work, true)?;
        find_launcher_binary(&work).ok_or("no launcher binary found in archive")?
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz(downloaded, &work)?;
        find_launcher_binary(&work).ok_or("no launcher binary found in archive")?
    } else {
        downloaded.to_path_buf()
    };

    // Make executable on unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&binary)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(&binary, perms).map_err(|e| e.to_string())?;
    }

    // Back up and atomically replace.
    let backup = target_exe.with_extension("old");
    if target_exe.exists() {
        let _ = std::fs::remove_file(&backup);
        let _ = std::fs::rename(target_exe, &backup);
    }
    match std::fs::rename(&binary, target_exe) {
        Ok(()) => Ok(target_exe.to_path_buf()),
        Err(_) => {
            // Cross-device: copy then remove.
            std::fs::copy(&binary, target_exe).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&binary);
            Ok(target_exe.to_path_buf())
        }
    }
}

/// A scratch directory next to the target executable.
///
/// Renaming across filesystems is not atomic, so the extracted files are
/// written beside the binary they will replace.
pub fn extraction_dir(target_exe: &Path) -> PathBuf {
    let parent = target_exe.parent().unwrap_or(Path::new("."));
    let candidate = parent.join(".prism-update");
    if std::fs::create_dir_all(&candidate).is_ok() {
        return candidate;
    }
    std::env::temp_dir().join("prism-update")
}

/// Locate a Prism launcher binary inside an extracted release.
pub fn find_launcher_binary(root: &Path) -> Option<PathBuf> {
    let mut best: Option<PathBuf> = None;
    for path in crate::fs::list_files(root) {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        let is_launcher = lower == "prismlauncher"
            || lower == "prism-gpui"
            || lower == "prismlauncher.exe"
            || lower == "prismlauncher.appimage"
            || lower.starts_with("prismlauncher")
                && (lower.ends_with(".appimage") || lower.ends_with(".exe"));
        if is_launcher {
            return Some(path);
        }
        if lower.contains("prism") && lower.ends_with(".appimage") {
            best = Some(path);
        }
    }
    best
}

/// Extract a `.tar.gz` archive, rejecting paths that escape the destination.
pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<usize, String> {
    use flate2::read::GzDecoder;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut tar = tar::Archive::new(GzDecoder::new(file));
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut n = 0usize;
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        let Some(rel) = path.to_str() else { continue };
        let Some(target) = crate::fs::safe_join(dest, rel) else {
            return Err(format!("unsafe path in archive: {rel}"));
        };
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            entry.unpack(&target).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("1.0.0", "1.0.1"));
        assert!(is_newer("v11.0.3", "11.1.0"));
        assert!(!is_newer("11.1.0", "11.1.0"));
        assert!(!is_newer("11.2.0", "11.1.0"));
    }

    #[test]
    fn picks_best_asset_for_platform() {
        let release = Release {
            tag_name: "v9.0.0".into(),
            html_url: String::new(),
            name: String::new(),
            body: String::new(),
            prerelease: false,
            assets: vec![
                Asset {
                    name: "PrismLauncher-9.0.0-Linux.AppImage".into(),
                    browser_download_url: "https://e/a".into(),
                    size: 100,
                    digest: None,
                },
                Asset {
                    name: "PrismLauncher-9.0.0-Linux.tar.gz".into(),
                    browser_download_url: "https://e/b".into(),
                    size: 90,
                    digest: Some("sha256:aa".into()),
                },
                Asset {
                    name: "PrismLauncher-9.0.0-Windows.zip".into(),
                    browser_download_url: "https://e/c".into(),
                    size: 999,
                    digest: None,
                },
            ],
        };
        let picked = select_asset(&release).unwrap();
        assert!(picked.name.contains(platform_keyword()));
    }

    #[test]
    fn asset_checksum_parsing() {
        let a = Asset {
            name: "x".into(),
            browser_download_url: String::new(),
            size: 0,
            digest: Some("sha256:deadbeef".into()),
        };
        assert_eq!(a.checksum(), Some(("sha256", "deadbeef")));
    }

    #[test]
    fn finds_launcher_binary_in_tree() {
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(td.path().join("PrismLauncher")).unwrap();
        std::fs::write(td.path().join("PrismLauncher/README.md"), "x").unwrap();
        std::fs::write(td.path().join("PrismLauncher/prismlauncher"), "bin").unwrap();
        let found = find_launcher_binary(td.path()).unwrap();
        assert_eq!(found.file_name().unwrap(), "prismlauncher");
    }

    #[test]
    fn apply_update_replaces_the_binary() {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("prism-gpui");
        std::fs::write(&target, b"old binary").unwrap();
        let staged = td.path().join("PrismLauncher-9.0.0-Linux.tar.gz");
        {
            let f = std::fs::File::create(&staged).unwrap();
            let enc = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
            let mut builder = tar::Builder::new(enc);
            let mut header = tar::Header::new_gnu();
            let data = b"new binary";
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "prismlauncher", &data[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let written = apply_update(&staged, &target).unwrap();
        assert_eq!(written, target);
        assert_eq!(std::fs::read(&target).unwrap(), b"new binary");
        // The previous binary is kept as a backup.
        assert!(td.path().join("prism-gpui.old").exists());
    }

    #[test]
    fn apply_update_copies_a_bare_binary() {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("prism-gpui");
        std::fs::write(&target, b"old").unwrap();
        let downloaded = td.path().join("PrismLauncher-9.0.0-Linux.AppImage");
        std::fs::write(&downloaded, b"new").unwrap();
        apply_update(&downloaded, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn extract_tar_gz_roundtrip() {
        // Build a tiny tar.gz on the fly.
        let td = tempfile::tempdir().unwrap();
        let archive = td.path().join("a.tar.gz");
        {
            let f = std::fs::File::create(&archive).unwrap();
            let enc = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
            let mut builder = tar::Builder::new(enc);
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "d/f.txt", &b"abc"[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let out = td.path().join("out");
        let n = extract_tar_gz(&archive, &out).unwrap();
        assert_eq!(n, 1);
        assert_eq!(std::fs::read_to_string(out.join("d/f.txt")).unwrap(), "abc");
    }
}
