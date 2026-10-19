//! Technic Platform / Solder API client. Mirrors launcher/modplatform/technic.
//!
//! Two pack layouts exist:
//! * **Solder** packs: the pack metadata names a Solder API base URL; builds
//!   are obtained from `{solder}modpack/{slug}/{build}` and each mod entry is a
//!   zip that is extracted over the instance's game directory.
//! * **Direct** packs: the pack metadata contains a single zip `url`.

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SolderPack {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub recommended: String,
    #[serde(default)]
    pub latest: String,
    #[serde(default)]
    pub builds: Vec<String>,
}

/// A Technic platform pack description (search result / pack page).
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlatformPack {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub solder: Option<String>,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub installs: u64,
    #[serde(default)]
    pub icon: Option<IconRef>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct IconRef {
    #[serde(default)]
    pub url: String,
}

impl PlatformPack {
    pub fn title(&self) -> &str {
        if self.display_name.is_empty() {
            &self.name
        } else {
            &self.display_name
        }
    }

    /// The pack identifier used by the platform and Solder APIs.
    pub fn slug(&self) -> &str {
        if !self.slug.is_empty() {
            &self.slug
        } else {
            &self.name
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
struct SearchResponse {
    #[serde(default)]
    modpacks: Vec<PlatformPack>,
}

#[derive(Deserialize, Debug, Clone)]
struct BuildMod {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    md5: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize, Debug, Clone)]
struct PackBuild {
    #[serde(default)]
    minecraft: String,
    #[serde(default)]
    mods: Vec<BuildMod>,
}

pub struct Technic;

impl Technic {
    /// Search the public Technic platform. `build=multimc` is required.
    pub fn search(&self, query: &str) -> Result<Vec<PlatformPack>, String> {
        let url = format!(
            "https://api.technicpack.net/search?build=multimc&q={}",
            urlencode(query)
        );
        let body = crate::net::get_string(&url)?;
        let resp: SearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp.modpacks)
    }

    /// Trending packs, used as the initial browse list.
    pub fn trending(&self) -> Result<Vec<PlatformPack>, String> {
        let body = crate::net::get_string("https://api.technicpack.net/trending?build=multimc")?;
        let resp: SearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        Ok(resp.modpacks)
    }

    /// Fetch a pack's platform page (which includes the Solder URL).
    pub fn pack(&self, slug: &str) -> Result<PlatformPack, String> {
        let body = crate::net::get_string(&format!(
            "https://api.technicpack.net/modpack/{}?build=multimc",
            urlencode(slug)
        ))?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Fetch the Solder build index for a pack.
    pub fn solder_pack(&self, solder: &str, slug: &str) -> Result<SolderPack, String> {
        let url = format!("{}/modpack/{}", solder.trim_end_matches('/'), urlencode(slug));
        let body = crate::net::get_string(&url)?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    fn solder_build(&self, solder: &str, slug: &str, build: &str) -> Result<PackBuild, String> {
        let url = format!(
            "{}/modpack/{}/{}",
            solder.trim_end_matches('/'),
            urlencode(slug),
            urlencode(build)
        );
        let body = crate::net::get_string(&url)?;
        serde_json::from_str(&body).map_err(|e| e.to_string())
    }

    /// Resolve the recommended build of a Solder pack and return the
    /// Minecraft version alongside the mod archives.
    pub fn resolve_build(
        &self,
        solder: &str,
        slug: &str,
    ) -> Result<(String, Vec<(String, String, String)>), String> {
        let pack = self.solder_pack(solder, slug)?;
        let build = if !pack.recommended.is_empty() {
            pack.recommended.clone()
        } else if !pack.latest.is_empty() {
            pack.latest.clone()
        } else {
            pack.builds
                .last()
                .cloned()
                .ok_or("Solder pack has no builds")?
        };
        let resolved = self.solder_build(solder, slug, &build)?;
        let mods = resolved
            .mods
            .into_iter()
            .filter(|m| !m.url.is_empty())
            .map(|m| (m.url, m.md5, format!("{}-{}", m.name, m.version)))
            .collect();
        Ok((resolved.minecraft, mods))
    }

    /// Install a pack into `game_dir` (the instance's `minecraft` folder).
    pub fn install(&self, slug: &str, game_dir: &std::path::Path) -> Result<String, String> {
        let pack = self.pack(slug)?;
        std::fs::create_dir_all(game_dir).map_err(|e| e.to_string())?;
        if let Some(solder) = pack.solder.as_deref().filter(|s| !s.is_empty()) {
            let (mc, mods) = self.resolve_build(solder, pack.slug())?;
            let tmp = std::env::temp_dir().join(format!(
                "prism-technic-{}-{}",
                std::process::id(),
                sanitize(pack.slug())
            ));
            let _ = std::fs::remove_dir_all(&tmp);
            std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
            let mut n = 0usize;
            for (i, (url, md5, label)) in mods.iter().enumerate() {
                let archive = tmp.join(format!("{i}.zip"));
                let checksum = if md5.is_empty() { None } else { Some(md5.as_str()) };
                crate::net::download_verified_md5(url, &archive, checksum)
                    .map_err(|e| format!("{label}: {e}"))?;
                if let Err(e) = crate::fs::extract_zip(&archive, game_dir, false) {
                    // Some Solder entries are plain jars rather than archives.
                    if label.is_empty() {
                        return Err(e);
                    }
                    let jar = game_dir.join("mods").join(format!("{label}.jar"));
                    if let Some(parent) = jar.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    std::fs::copy(&archive, &jar).map_err(|e| e.to_string())?;
                }
                n += 1;
            }
            let _ = std::fs::remove_dir_all(&tmp);
            return Ok(format!(
                "installed Technic pack {} (MC {mc}, {n} archives)",
                pack.title()
            ));
        }
        let url = pack
            .url
            .as_deref()
            .filter(|u| !u.is_empty())
            .ok_or("pack has neither a Solder URL nor a download URL")?;
        let tmp = std::env::temp_dir().join(format!("prism-technic-{}.zip", std::process::id()));
        crate::net::download_verified(url, &tmp, None)?;
        crate::fs::extract_zip(&tmp, game_dir, true)?;
        let _ = std::fs::remove_file(&tmp);
        Ok(format!("installed Technic pack {}", pack.title()))
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_response() {
        let json = r#"{"modpacks":[
            {"id":"1","name":"tekkitmain","slug":"tekkitmain","displayName":"Tekkit",
             "url":"https://www.technicpack.net/modpack/tekkitmain.1","installs":42,
             "solder":"https://solder.technicpack.net/api/","minecraft":"1.4.7","version":"1.0"}
        ]}"#;
        let r: SearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.modpacks.len(), 1);
        assert_eq!(r.modpacks[0].title(), "Tekkit");
        assert_eq!(r.modpacks[0].slug(), "tekkitmain");
        assert_eq!(r.modpacks[0].solder.as_deref(), Some("https://solder.technicpack.net/api/"));
    }

    #[test]
    fn parses_solder_pack_and_build() {
        let p: SolderPack =
            serde_json::from_str(r#"{"recommended":"1.2","latest":"1.3","builds":["1.3","1.2"]}"#)
                .unwrap();
        assert_eq!(p.recommended, "1.2");
        assert_eq!(p.builds.len(), 2);

        let b: PackBuild = serde_json::from_str(
            r#"{"minecraft":"1.12.2","mods":[{"name":"a","version":"1","md5":"x","url":"https://e/a.zip"}]}"#,
        )
        .unwrap();
        assert_eq!(b.minecraft, "1.12.2");
        assert_eq!(b.mods[0].url, "https://e/a.zip");
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(urlencode("a b"), "a+b");
        assert_eq!(urlencode("x/y"), "x%2Fy");
    }
}
