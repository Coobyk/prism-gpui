//! Read the metadata of an installed mod.
//!
//! Mirrors what Prism shows next to a mod: the name, the version and the
//! author. Mods come in three flavours and each has its own descriptor:
//!
//! * Fabric: `fabric.mod.json`
//! * Quilt: `quilt.mod.json`
//! * Forge and NeoForge: `META-INF/mods.toml`

use std::path::Path;

/// What is known about an installed mod.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModInfo {
    /// The name of the mod.
    pub name: String,
    /// The version of the mod.
    pub version: String,
    /// The author of the mod.
    pub author: String,
    /// The identifier of the mod, e.g. `sodium`.
    pub id: String,
}

impl ModInfo {
    /// A one line description, like the one Prism puts under the name.
    pub fn summary(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if !self.version.is_empty() {
            parts.push(&self.version);
        }
        if !self.author.is_empty() {
            parts.push(&self.author);
        }
        parts.join(" · ")
    }
}

/// Read the metadata of a mod jar.
pub fn read_mod_info(path: &Path) -> Option<ModInfo> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;

    // Fabric and Quilt keep a JSON descriptor in the root of the jar.
    for name in ["fabric.mod.json", "quilt.mod.json"] {
        if let Some(info) = read_json_descriptor(&mut archive, name) {
            return Some(info);
        }
    }
    // Forge and NeoForge use a TOML file.
    read_forge_descriptor(&mut archive)
}

/// Read a `fabric.mod.json` or `quilt.mod.json` descriptor.
fn read_json_descriptor<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<ModInfo> {
    let mut file = archive.by_name(name).ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;

    if name == "fabric.mod.json" {
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let mut info = ModInfo {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string(),
            version: value
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            id,
            ..Default::default()
        };
        if let Some(authors) = value.get("authors").and_then(|v| v.as_array()) {
            let names: Vec<String> = authors
                .iter()
                .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                .map(|n| n.to_string())
                .collect();
            info.author = names.join(", ");
        }
        Some(info)
    } else {
        // quilt.mod.json keeps the loader section.
        let loader = value.get("quilt_loader")?;
        let id = loader
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let meta = value.get("metadata");
        Some(ModInfo {
            name: meta
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or(&id)
                .to_string(),
            version: loader
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            author: meta
                .and_then(|m| m.get("contributors"))
                .and_then(|v| v.get("author"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            id,
        })
    }
}

/// Read a `META-INF/mods.toml` descriptor.
fn read_forge_descriptor<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Option<ModInfo> {
    let mut file = archive.by_name("META-INF/mods.toml").ok()?;
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).ok()?;
    let value: toml::Value = text.parse().ok()?;

    // The first entry of the `mods` table describes the mod itself. Most
    // mods use `[[mods]]`, which is an array of tables, some use `[mods]`.
    let mods = value.get("mods")?;
    let entry = match mods {
        toml::Value::Array(entries) => entries.first()?,
        toml::Value::Table(table) => table.values().next()?,
        _ => return None,
    };
    let id = entry
        .get("modId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let author = entry
        .get("authors")
        .and_then(|v| v.as_str())
        .or_else(|| {
            entry
                .get("authors")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
        })
        .unwrap_or_default()
        .to_string();
    Some(ModInfo {
        name: entry
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string(),
        version: entry
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        author,
        id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn jar_with(entries: &[(&str, &str)]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(file.path()).unwrap());
            let options: zip::write::FileOptions<()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, content) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        file
    }

    #[test]
    fn reads_a_fabric_mod() {
        let file = jar_with(&[(
            "fabric.mod.json",
            r#"{"id":"sodium","name":"Sodium","version":"0.5.8","authors":[{"name":"JellySquid"}]}"#,
        )]);
        let info = read_mod_info(file.path()).unwrap();
        assert_eq!(info.name, "Sodium");
        assert_eq!(info.version, "0.5.8");
        assert_eq!(info.author, "JellySquid");
        assert_eq!(info.id, "sodium");
        assert_eq!(info.summary(), "0.5.8 · JellySquid");
    }

    #[test]
    fn reads_a_forge_mod() {
        let file = jar_with(&[(
            "META-INF/mods.toml",
            "modLoader=\"javafml\"\n[[mods]]\nmodId=\"examplemod\"\nversion=\"1.2.3\"\ndisplayName=\"Example Mod\"\nauthors=\"Someone\"\n",
        )]);
        let info = read_mod_info(file.path()).unwrap();
        assert_eq!(info.name, "Example Mod");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.author, "Someone");
        assert_eq!(info.id, "examplemod");
    }

    #[test]
    fn a_jar_without_a_descriptor_has_no_info() {
        let file = jar_with(&[("META-INF/MANIFEST.MF", "Manifest-Version: 1.0\n")]);
        assert!(read_mod_info(file.path()).is_none());
    }
}
