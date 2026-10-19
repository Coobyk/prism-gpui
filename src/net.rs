//! Networking helpers: verified downloads, meta cache.
//! Mirrors launcher/net/* (HttpMetaCache, NetJob, ChecksumValidator).

use sha1::Sha1;
use sha2::{Digest as _, Sha256, Sha512};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

const UA: &str = "prism-gpui/0.1 (github.com/Coobyk/prism-gpui)";

static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

/// Proxy explicitly configured by the launcher's settings (wins over env).
/// Must be set once, before the first request, via [`set_proxy`].
static PROXY_OVERRIDE: OnceLock<Option<String>> = OnceLock::new();

/// Install the proxy from global settings. Safe to call repeatedly; only the
/// first call takes effect (the agent is built lazily on first request).
pub fn set_proxy(proxy: Option<String>) {
    let _ = PROXY_OVERRIDE.set(proxy);
}

/// Proxy URL: an explicit setting wins, then `PRISM_PROXY`, then the standard
/// `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` environment variables.
fn proxy_url() -> Option<String> {
    if let Some(Some(p)) = PROXY_OVERRIDE.get() {
        return Some(p.clone());
    }
    for key in [
        "PRISM_PROXY",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// The shared HTTP agent (with optional proxy configuration).
pub fn agent() -> &'static ureq::Agent {
    AGENT.get_or_init(|| {
        let mut builder = ureq::AgentBuilder::new().user_agent(UA);
        if let Some(url) = proxy_url() {
            match ureq::Proxy::new(&url) {
                Ok(proxy) => {
                    builder = builder.proxy(proxy);
                }
                Err(e) => eprintln!("ignoring invalid proxy {url:?}: {e}"),
            }
        }
        builder.build()
    })
}

pub fn sha1_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

pub fn md5_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = md5::Md5::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

pub fn sha512_file(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha512::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

/// Hash a file with the named algorithm (`sha1`, `sha256`, `sha512`, `md5`).
pub fn hash_file(path: &Path, algorithm: &str) -> Option<String> {
    match algorithm.to_ascii_lowercase().as_str() {
        "sha1" => sha1_file(path),
        "sha256" => sha256_file(path),
        "sha512" => sha512_file(path),
        "md5" => md5_file(path),
        _ => None,
    }
}

/// Download `url` to `dest`, verifying a checksum of the named algorithm when
/// provided.
pub fn download_verified_hash(
    url: &str,
    dest: &Path,
    algorithm: Option<&str>,
    expected: Option<&str>,
) -> Result<(), String> {
    let checksum = match (algorithm, expected) {
        (Some(a), Some(e)) if !e.is_empty() => Some((a.to_string(), e.to_string())),
        _ => None,
    };
    if dest.is_file()
        && (checksum.is_none()
            || checksum
                .as_ref()
                .and_then(|(a, e)| hash_file(dest, a).map(|g| g == *e))
                .unwrap_or(false))
    {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    let resp = agent().get(url).call().map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut out).map_err(|e| e.to_string())?;
    drop(out);
    if let Some((a, e)) = checksum {
        let got = hash_file(&tmp, &a).ok_or_else(|| format!("unknown hash algorithm: {a}"))?;
        if got != e {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("{a} mismatch for {url}: expected {e}, got {got}"));
        }
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Download `url` to `dest`, verifying the sha1 if provided. Skips if the
/// destination already matches.
pub fn download_verified(url: &str, dest: &Path, expected_sha1: Option<&str>) -> Result<(), String> {
    download_with(url, dest, expected_sha1.map(|s| (s, true)))
}

/// Download `url` to `dest`, verifying an md5 checksum if provided.
pub fn download_verified_md5(
    url: &str,
    dest: &Path,
    expected_md5: Option<&str>,
) -> Result<(), String> {
    download_with(url, dest, expected_md5.map(|s| (s, false)))
}

/// Shared download path: `checksum` is `(value, is_sha1)`.
fn download_with(
    url: &str,
    dest: &Path,
    checksum: Option<(&str, bool)>,
) -> Result<(), String> {
    let verify = |path: &Path| -> Result<(), String> {
        match checksum {
            None => Ok(()),
            Some((exp, true)) => match sha1_file(path) {
                Some(got) if got == exp => Ok(()),
                Some(got) => Err(format!("sha1 mismatch for {url}: expected {exp}, got {got}")),
                None => Err("could not hash download".to_string()),
            },
            Some((exp, false)) => match md5_file(path) {
                Some(got) if got == exp => Ok(()),
                Some(got) => Err(format!("md5 mismatch for {url}: expected {exp}, got {got}")),
                None => Err("could not hash download".to_string()),
            },
        }
    };
    if dest.is_file() {
        if checksum.is_none() || verify(dest).is_ok() {
            return Ok(());
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("part");
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    std::io::copy(&mut reader, &mut out).map_err(|e| e.to_string())?;
    drop(out);

    if let Err(e) = verify(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// GET a URL as a string.
pub fn get_string(url: &str) -> Result<String, String> {
    agent()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

/// GET a URL as raw bytes.
pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    agent()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes_known_string() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha1_file(&p).unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }
}
