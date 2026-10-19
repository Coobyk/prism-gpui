//! Networking helpers: verified downloads, meta cache.
//! Mirrors launcher/net/* (HttpMetaCache, NetJob, ChecksumValidator).

use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

const UA: &str = "prism-gpui/0.1 (github.com/Coobyk/prism-gpui)";

static AGENT: OnceLock<ureq::Agent> = OnceLock::new();

/// Proxy URL from the environment, honouring `PRISM_PROXY` first, then the
/// standard `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` variables.
fn proxy_url() -> Option<String> {
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

/// Download `url` to `dest`, verifying the sha1 if provided. Skips if the
/// destination already matches.
pub fn download_verified(url: &str, dest: &Path, expected_sha1: Option<&str>) -> Result<(), String> {
    if dest.is_file() {
        if let Some(exp) = expected_sha1 {
            if sha1_file(dest).as_deref() == Some(exp) {
                return Ok(());
            }
        } else {
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

    if let Some(exp) = expected_sha1 {
        let got = sha1_file(&tmp).ok_or("could not hash download")?;
        if got != exp {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("sha1 mismatch for {url}: expected {exp}, got {got}"));
        }
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
