//! Multiplayer server list. Reads `servers.dat` (NBT) — mirrors the launcher's
//! server list page.

use crate::nbt::{self, Nbt};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Server {
    pub name: String,
    pub ip: String,
}

pub fn servers_dat_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join("minecraft").join("servers.dat")
}

/// Read the server list from an instance's `servers.dat`.
pub fn list_servers(instance_dir: &Path) -> Vec<Server> {
    let path = servers_dat_path(instance_dir);
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    let Some(root) = nbt::parse(&bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(Nbt::List(items)) = root.get("servers") {
        for item in items {
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let ip = item.get("ip").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !ip.is_empty() {
                out.push(Server { name, ip });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal uncompressed servers.dat with two servers.
    fn sample() -> Vec<u8> {
        let mut raw = Vec::new();
        raw.push(10u8); // root compound
        raw.extend_from_slice(&0u16.to_be_bytes());
        raw.push(9u8); // List
        raw.extend_from_slice(&7u16.to_be_bytes());
        raw.extend_from_slice(b"servers");
        raw.push(10u8); // element type: compound
        raw.extend_from_slice(&2i32.to_be_bytes()); // 2 elements
        for (name, ip) in [("Alpha", "alpha.example:25565"), ("Beta", "beta.example")] {
            // compound contents (unnamed)
            raw.push(8u8);
            raw.extend_from_slice(&4u16.to_be_bytes());
            raw.extend_from_slice(b"name");
            raw.extend_from_slice(&(name.len() as u16).to_be_bytes());
            raw.extend_from_slice(name.as_bytes());
            raw.push(8u8);
            raw.extend_from_slice(&2u16.to_be_bytes());
            raw.extend_from_slice(b"ip");
            raw.extend_from_slice(&(ip.len() as u16).to_be_bytes());
            raw.extend_from_slice(ip.as_bytes());
            raw.push(0u8); // end compound
        }
        raw.push(0u8); // end root
        raw
    }

    #[test]
    fn lists_servers() {
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(td.path().join("minecraft")).unwrap();
        std::fs::write(td.path().join("minecraft/servers.dat"), sample()).unwrap();
        let servers = list_servers(td.path());
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "Alpha");
        assert_eq!(servers[1].ip, "beta.example");
    }

    #[test]
    fn missing_file_is_empty() {
        let td = tempfile::tempdir().unwrap();
        assert!(list_servers(td.path()).is_empty());
    }

    // Silence unused-import warning in case Write is only used transitively.
    #[allow(dead_code)]
    fn _use_write<W: Write>(_w: &mut W) {}
}
