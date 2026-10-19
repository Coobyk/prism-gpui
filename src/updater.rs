//! Update checker. Mirrors launcher/updater (GitHub releases).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub name: String,
}

/// Fetch the latest Prism Launcher release.
pub fn latest_release() -> Result<Release, String> {
    let body = crate::net::get_string(
        "https://api.github.com/repos/PrismLauncher/PrismLauncher/releases/latest",
    )?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Compare two dotted versions; returns true if `b` is newer than `a`.
pub fn is_newer(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split(['.', '-'])
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
}
