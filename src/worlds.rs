//! World (save) management. Mirrors launcher/minecraft/WorldList, World.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct World {
    /// Directory name (stable id).
    pub name: String,
    /// Human-readable name from `level.dat` (falls back to the id).
    pub display_name: String,
    pub dir: PathBuf,
    pub has_level_dat: bool,
    pub size_bytes: u64,
    pub last_played: i64,
}

pub fn saves_dir(instance_dir: &Path) -> PathBuf {
    instance_dir.join("minecraft").join("saves")
}

pub fn list_worlds(instance_dir: &Path) -> Vec<World> {
    let dir = saves_dir(instance_dir);
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let name = e.file_name().to_string_lossy().to_string();
                let level = p.join("level.dat");
                let has_level_dat = level.is_file();
                let (display_name, last_played) = if has_level_dat {
                    crate::nbt::read_level_dat(&level).unwrap_or((name.clone(), 0))
                } else {
                    (name.clone(), 0)
                };
                out.push(World {
                    name,
                    display_name,
                    has_level_dat,
                    size_bytes: dir_size(&p),
                    last_played,
                    dir: p,
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn delete_world(instance_dir: &Path, name: &str) -> Result<(), String> {
    let p = saves_dir(instance_dir).join(name);
    if !p.is_dir() {
        return Err(format!("world not found: {name}"));
    }
    std::fs::remove_dir_all(&p).map_err(|e| e.to_string())
}

pub fn rename_world(instance_dir: &Path, from: &str, to: &str) -> Result<PathBuf, String> {
    let src = saves_dir(instance_dir).join(from);
    let dst = saves_dir(instance_dir).join(to);
    if !src.is_dir() {
        return Err(format!("world not found: {from}"));
    }
    if dst.exists() {
        return Err(format!("world already exists: {to}"));
    }
    std::fs::rename(&src, &dst).map_err(|e| e.to_string())?;
    Ok(dst)
}

fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn list_and_manage() {
        let td = tempfile::tempdir().unwrap();
        let inst = td.path();
        let saves = saves_dir(inst);
        std::fs::create_dir_all(saves.join("World1")).unwrap();
        std::fs::write(saves.join("World1/level.dat"), b"x").unwrap();
        let worlds = list_worlds(inst);
        assert_eq!(worlds.len(), 1);
        assert!(worlds[0].has_level_dat);
        assert!(worlds[0].size_bytes > 0);
        rename_world(inst, "World1", "World2").unwrap();
        assert_eq!(list_worlds(inst)[0].name, "World2");
        delete_world(inst, "World2").unwrap();
        assert!(list_worlds(inst).is_empty());
    }
}
