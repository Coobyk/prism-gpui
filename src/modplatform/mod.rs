//! Mod platform providers. Mirrors launcher/modplatform/*.
//! Start with Modrinth; CurseForge/FTB/etc. follow.

pub mod atlauncher;
pub mod curseforge;
pub mod ftb;
pub mod modrinth;
pub mod packwiz;
pub mod technic;

pub trait ModProvider {
    fn name(&self) -> &'static str;
    fn search(&self, query: &str) -> Vec<ModHit>;
}

#[derive(Debug, Clone)]
pub struct ModHit {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub downloads: u64,
}
