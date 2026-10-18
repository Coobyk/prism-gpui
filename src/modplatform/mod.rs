//! Mod platform providers. Mirrors launcher/modplatform/*.
//! Start with Modrinth; CurseForge/FTB/etc. follow.

pub mod modrinth;

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
