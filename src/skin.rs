//! Skin viewer. Mirrors launcher/ui/SkinViewer (fetches renders from
//! Crafatar, an open Minecraft avatar/skin render service).

use std::path::Path;

/// Service that renders a player's skin/avatar from their UUID.
pub struct SkinService;

impl SkinService {
    /// URL for a full-body render (front, 3D) of a player.
    pub fn body_url(uuid: &str) -> String {
        format!(
            "https://crafatar.com/renders/body/{}?size=256&overlay",
            uuid.replace('-', "")
        )
    }

    /// URL for a square head render of a player.
    pub fn head_url(uuid: &str) -> String {
        format!(
            "https://crafatar.com/avatars/{}?size=128&overlay",
            uuid.replace('-', "")
        )
    }

    /// Download a body render; returns the PNG bytes.
    pub fn body_bytes(uuid: &str) -> Result<Vec<u8>, String> {
        crate::net::get_bytes(&Self::body_url(uuid))
    }

    /// Download the raw skin texture from Mojang's session texture server.
    /// Requires the texture hash, which is resolved from the profile.
    pub fn skin_url_from_texture(texture_id: &str) -> String {
        format!("https://textures.minecraft.net/texture/{texture_id}")
    }

    /// Convenience: persist a body render to disk (used by tests and CLI).
    pub fn save_body(uuid: &str, dest: &Path) -> Result<(), String> {
        let bytes = Self::body_bytes(uuid)?;
        std::fs::write(dest, bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_strip_dashes() {
        let uuid = "b50ad385-829d-3141-a216-7e7d7539ba7f";
        assert!(SkinService::body_url(uuid).contains("b50ad385829d3141a2167e7d7539ba7f"));
        assert!(SkinService::head_url(uuid).contains("b50ad385829d3141a2167e7d7539ba7f"));
    }
}

// ---- Skin management (mirrors launcher/minecraft/skins/SkinRequests.cpp) ----

/// The two player models a skin can be made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinVariant {
    Classic,
    Slim,
}

impl SkinVariant {
    /// The value Mojang's API expects in the `variant` form field.
    pub fn as_str(&self) -> &'static str {
        match self {
            SkinVariant::Classic => "classic",
            SkinVariant::Slim => "slim",
        }
    }

    pub fn from_str(value: &str) -> Self {
        if value.eq_ignore_ascii_case("slim") {
            SkinVariant::Slim
        } else {
            SkinVariant::Classic
        }
    }
}

const MC_SKINS_URL: &str = "https://api.minecraftservices.com/minecraft/profile/skins";
const MC_ACTIVE_SKIN_URL: &str = "https://api.minecraftservices.com/minecraft/profile/skins/active";
const MC_CAPE_URL: &str = "https://api.minecraftservices.com/minecraft/profile/capes/active";

/// A skin as returned by Mojang's profile API.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MojangSkin {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// Build a `multipart/form-data` body for a skin upload.
///
/// Mojang expects a `file` part holding the PNG and a `variant` part with
/// `classic` or `slim`.
pub fn build_skin_upload_body(png: &[u8], variant: SkinVariant) -> (String, Vec<u8>) {
    let boundary = "----PrismLauncherSkinUpload";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"skin.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(png);
    body.extend_from_slice(
        format!("\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"variant\"\r\n\r\n{}\r\n--{boundary}--\r\n", variant.as_str())
            .as_bytes(),
    );
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// Upload a skin PNG for the signed-in account.
pub fn upload_skin(access_token: &str, png: &[u8], variant: SkinVariant) -> Result<(), String> {
    let (content_type, body) = build_skin_upload_body(png, variant);
    let response = crate::net::agent()
        .post(MC_SKINS_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("skin upload failed: HTTP {status}"));
    }
    Ok(())
}

/// Remove the active skin, restoring the default.
pub fn reset_skin(access_token: &str) -> Result<(), String> {
    let response = crate::net::agent()
        .delete(MC_ACTIVE_SKIN_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("skin reset failed: HTTP {status}"));
    }
    Ok(())
}

/// List the account's skins.
pub fn list_skins(access_token: &str) -> Result<Vec<MojangSkin>, String> {
    let body = crate::net::agent()
        .get(MC_SKINS_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

/// Activate one of the account's skins.
pub fn activate_skin(access_token: &str, skin_id: &str) -> Result<(), String> {
    let response = crate::net::agent()
        .put(MC_ACTIVE_SKIN_URL)
        .set("Authorization", &format!("Bearer {access_token}"))
        .send_string(&format!("{{\"skinId\":\"{skin_id}\"}}"))
        .map_err(|e| e.to_string())?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("skin activation failed: HTTP {status}"));
    }
    Ok(())
}

/// Select a cape, or clear it when `cape_id` is empty.
pub fn set_cape(access_token: &str, cape_id: &str) -> Result<(), String> {
    let agent = crate::net::agent();
    let request = agent
        .put(MC_CAPE_URL)
        .set("Authorization", &format!("Bearer {access_token}"));
    let response = if cape_id.is_empty() {
        request.call().map_err(|e| e.to_string())?
    } else {
        request
            .send_string(&format!("{{\"capeId\":\"{cape_id}\"}}"))
            .map_err(|e| e.to_string())?
    };
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("cape change failed: HTTP {status}"));
    }
    Ok(())
}

/// Directory holding the skins the user keeps locally.
pub fn local_skin_dir(data_root: &Path) -> std::path::PathBuf {
    data_root.join("skins")
}

/// Import a skin PNG into the local skin folder, returning its path.
pub fn import_local_skin(
    data_root: &Path,
    name: &str,
    png: &[u8],
) -> Result<std::path::PathBuf, String> {
    let dir = local_skin_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!("{}.png", safe.trim_matches('_')));
    std::fs::write(&path, png).map_err(|e| e.to_string())?;
    Ok(path)
}

/// List the locally stored skins.
pub fn list_local_skins(data_root: &Path) -> Vec<std::path::PathBuf> {
    let dir = local_skin_dir(data_root);
    let mut out: Vec<_> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Check that a file is a Minecraft skin texture (64x64, or 64x32 for the
/// legacy format). The dimensions come straight from the PNG header, so no
/// image decoding is needed.
pub fn validate_skin_file(path: &Path) -> Result<(u32, u32), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
        return Err("not a PNG file".into());
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if (width, height) != (64, 64) && (width, height) != (64, 32) {
        return Err(format!(
            "skin must be 64x64 pixels (64x32 for legacy skins), got {width}x{height}"
        ));
    }
    Ok((width, height))
}

/// Copy a skin file into the launcher's local skin folder.
pub fn install_skin(data_root: &Path, source: &Path) -> Result<std::path::PathBuf, String> {
    validate_skin_file(source)?;
    let dir = local_skin_dir(data_root);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skin".into());
    let mut target = dir.join(format!("{name}.png"));
    let mut n = 2;
    while target.exists() {
        target = dir.join(format!("{name} ({n}).png"));
        n += 1;
    }
    std::fs::copy(source, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Remove a skin from the local folder.
pub fn remove_local_skin(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod management_tests {
    use super::*;

    /// A minimal PNG header, enough for the dimension check.
    fn fake_png(width: u32, height: u32) -> Vec<u8> {
        let mut data = vec![0u8; 24];
        data[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        data[12..16].copy_from_slice(b"IHDR");
        data[16..20].copy_from_slice(&width.to_be_bytes());
        data[20..24].copy_from_slice(&height.to_be_bytes());
        data
    }

    #[test]
    fn skin_variants_match_mojang() {
        assert_eq!(SkinVariant::Classic.as_str(), "classic");
        assert_eq!(SkinVariant::Slim.as_str(), "slim");
        assert_eq!(SkinVariant::from_str("SLIM"), SkinVariant::Slim);
        assert_eq!(SkinVariant::from_str("classic"), SkinVariant::Classic);
    }

    #[test]
    fn upload_body_has_file_and_variant_parts() {
        let (content_type, body) = build_skin_upload_body(b"PNGDATA", SkinVariant::Slim);
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"file\""));
        assert!(text.contains("name=\"variant\""));
        assert!(text.contains("slim"));
        assert!(text.contains("PNGDATA"));
    }

    #[test]
    fn validates_skin_dimensions() {
        let td = tempfile::tempdir().unwrap();
        let good = td.path().join("good.png");
        std::fs::write(&good, fake_png(64, 64)).unwrap();
        assert_eq!(validate_skin_file(&good).unwrap(), (64, 64));

        let legacy = td.path().join("legacy.png");
        std::fs::write(&legacy, fake_png(64, 32)).unwrap();
        assert_eq!(validate_skin_file(&legacy).unwrap(), (64, 32));

        let bad = td.path().join("bad.png");
        std::fs::write(&bad, fake_png(128, 128)).unwrap();
        assert!(validate_skin_file(&bad).unwrap_err().contains("64x64"));

        let not_png = td.path().join("not.png");
        std::fs::write(&not_png, b"hello world").unwrap();
        assert!(
            validate_skin_file(&not_png)
                .unwrap_err()
                .contains("not a PNG")
        );
    }

    #[test]
    fn installs_and_lists_local_skins() {
        let td = tempfile::tempdir().unwrap();
        let src = td.path().join("mine.png");
        std::fs::write(&src, fake_png(64, 64)).unwrap();
        let installed = install_skin(td.path(), &src).unwrap();
        assert!(installed.is_file());
        assert_eq!(list_local_skins(td.path()).len(), 1);

        // Installing the same file twice keeps both.
        install_skin(td.path(), &src).unwrap();
        assert_eq!(list_local_skins(td.path()).len(), 2);

        remove_local_skin(&installed).unwrap();
        assert_eq!(list_local_skins(td.path()).len(), 1);
    }
}
