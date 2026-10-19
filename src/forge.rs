//! Forge / NeoForge installer support. Mirrors launcher/java/`*Installer` for
//! Forge and the `net.neoforged`/`net.minecraftforge` components.
//!
//! Rather than re-implementing the whole installer processor chain, this runs
//! the official installer jar in client mode, which is what produces the
//! version JSON and patched client that the launcher consumes.

use std::path::Path;
use std::process::Command;

const FORGE_PROMOS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";

/// Resolve the recommended (or latest) Forge version for a Minecraft version.
pub fn resolve_forge(mc_version: &str, prefer_recommended: bool) -> Result<String, String> {
    let body = crate::net::get_string(FORGE_PROMOS)?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let promos = json
        .get("promos")
        .and_then(|p| p.as_object())
        .ok_or("forge promotions JSON has no promos")?;
    let keys = if prefer_recommended {
        vec![
            format!("{mc_version}-recommended"),
            format!("{mc_version}-latest"),
        ]
    } else {
        vec![
            format!("{mc_version}-latest"),
            format!("{mc_version}-recommended"),
        ]
    };
    for k in keys {
        if let Some(v) = promos.get(&k).and_then(|v| v.as_str()) {
            return Ok(v.to_string());
        }
    }
    Err(format!("no Forge version for Minecraft {mc_version}"))
}

/// The download URL of a Forge installer jar.
pub fn forge_installer_url(mc_version: &str, forge_version: &str) -> String {
    format!(
        "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{f}/forge-{mc}-{f}-installer.jar",
        mc = mc_version,
        f = forge_version
    )
}

/// The download URL of a NeoForge installer jar.
pub fn neoforge_installer_url(version: &str) -> String {
    format!(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-installer.jar",
        v = version
    )
}

/// List NeoForge versions compatible with a Minecraft version (best-effort:
/// NeoForge versions are prefixed with the Minecraft minor, e.g. `21.1.x`).
pub fn resolve_neoforge(mc_version: &str) -> Result<Vec<String>, String> {
    let body = crate::net::get_string(NEOFORGE_VERSIONS)?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let versions = json
        .get("versions")
        .and_then(|v| v.as_array())
        .ok_or("neoforge versions JSON has no versions")?;
    // Map "1.21.1" -> "21.1".
    let short = mc_version.strip_prefix("1.").unwrap_or(mc_version).to_string();
    let mut out: Vec<String> = versions
        .iter()
        .filter_map(|v| v.as_str())
        .filter(|v| v.starts_with(&format!("{short}.")))
        .map(|v| v.to_string())
        .collect();
    out.reverse(); // newest first
    Ok(out)
}

/// Download and run a Forge-family installer against an instance directory.
/// `installer_url` should point at the installer jar. Returns the log tail on
/// failure so callers can surface a useful error.
pub fn run_installer(installer_url: &str, java: &Path, instance_dir: &Path) -> Result<(), String> {
    let tmp_dir = instance_dir.join(".installer");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let jar = tmp_dir.join("installer.jar");
    crate::net::download_verified(installer_url, &jar, None)?;

    let output = Command::new(java)
        .arg("-jar")
        .arg(&jar)
        .arg("--installClient")
        .arg(instance_dir)
        .current_dir(instance_dir)
        .output()
        .map_err(|e| format!("failed to run installer: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let tail: String = format!("{stdout}\n{stderr}")
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!("installer failed:\n{tail}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_url_shape() {
        let url = forge_installer_url("1.20.1", "47.2.0");
        assert!(url.contains("1.20.1-47.2.0"));
        assert!(url.ends_with("-installer.jar"));
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    #[test]
    #[ignore]
    fn resolves_forge_recommended() {
        let v = resolve_forge("1.20.1", true).unwrap();
        assert!(!v.is_empty());
    }
}
