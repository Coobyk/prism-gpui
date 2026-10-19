//! Skin viewer. Mirrors launcher/ui/SkinViewer (fetches renders from
//! Crafatar, an open Minecraft avatar/skin render service).

use std::path::Path;

/// Service that renders a player's skin/avatar from their UUID.
pub struct SkinService;

impl SkinService {
    /// URL for a full-body render (front, 3D) of a player.
    pub fn body_url(uuid: &str) -> String {
        format!("https://crafatar.com/renders/body/{}?size=256&overlay", uuid.replace('-', ""))
    }

    /// URL for a square head render of a player.
    pub fn head_url(uuid: &str) -> String {
        format!("https://crafatar.com/avatars/{}?size=128&overlay", uuid.replace('-', ""))
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
