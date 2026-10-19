//! Launch controller. Mirrors launcher/LaunchController, launch/*.
//! Builds a JVM command from Prism component metadata + Mojang version JSON.

use crate::meta::{self, VersionJson};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Default)]
pub struct LaunchProfile {
    pub java_path: PathBuf,
    pub min_mem_mb: u32,
    pub max_mem_mb: u32,
    pub game_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub main_class: String,
    pub classpath: Vec<PathBuf>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

impl LaunchProfile {
    pub fn jvm_command(&self) -> Command {
        let mut cmd = Command::new(&self.java_path);
        cmd.arg(format!("-Xms{}M", self.min_mem_mb));
        cmd.arg(format!("-Xmx{}M", self.max_mem_mb));
        for a in &self.jvm_args {
            cmd.arg(a);
        }
        let cp = self
            .classpath
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(":");
        cmd.arg("-cp").arg(cp);
        cmd.arg(&self.main_class);
        cmd.args(&self.game_args);
        cmd.current_dir(&self.game_dir);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd
    }
}

/// Build a launch profile for an instance without an account (offline/dev).
pub fn build_profile(
    inst_dir: &Path,
    data_root: &Path,
    java_path: &Path,
) -> Option<LaunchProfile> {
    build_profile_with_account(inst_dir, data_root, java_path, None)
}

/// Build a launch profile for an instance from Prism metadata, substituting the
/// given account's identity and access token into the game arguments.
pub fn build_profile_with_account(
    inst_dir: &Path,
    data_root: &Path,
    java_path: &Path,
    account: Option<&crate::auth::Account>,
) -> Option<LaunchProfile> {
    let pack = meta::read_component_pack(inst_dir)?;
    let meta_dir = data_root.join("meta");
    let libraries_dir = data_root.join("libraries");

    let mut version = VersionJson::default();
    let mut classpath: Vec<PathBuf> = Vec::new();
    let mut jvm_args: Vec<String> = Vec::new();
    let mut saw_minecraft = false;

    // Components are ordered dependency-first; later ones (loader) override.
    for comp in &pack.components {
        let candidate = meta_dir.join(&comp.uid).join(format!("{}.json", &comp.version));
        let Some(v) = meta::read_version_json(&candidate) else {
            continue;
        };
        if comp.uid == "net.minecraft" {
            saw_minecraft = true;
            version.main_class = v.main_class.clone();
            version.asset_index = v.asset_index.clone();
            version.arguments = v.arguments.clone();
            version.minecraft_arguments = v.minecraft_arguments.clone();
            version.version_type = v.version_type.clone();
            version.id = v.id.clone();
        } else if !v.main_class.is_empty() {
            version.main_class = v.main_class.clone();
        }
        for lib in &v.libraries {
            if let Some(p) = lib.maven_path() {
                let full = libraries_dir.join(p);
                if full.is_file() && !classpath.contains(&full) {
                    classpath.push(full);
                }
            }
        }
        jvm_args.extend(collect_strings(v.arguments.as_ref().map(|a| &a.jvm)));
    }
    if !saw_minecraft {
        return None;
    }

    let game_dir = inst_dir.join("minecraft");
    let assets_dir = data_root.join("assets");

    // Substitute Mojang argument placeholders.
    let mut game_args = collect_strings(version.arguments.as_ref().map(|a| &a.game));
    if game_args.is_empty() {
        if let Some(raw) = &version.minecraft_arguments {
            game_args = raw.split_whitespace().map(|s| s.to_string()).collect();
        }
    }
    let player_name = account.map(|a| a.username.clone()).unwrap_or_else(|| "Player".into());
    let player_uuid = account
        .map(|a| a.uuid.clone())
        .unwrap_or_else(|| "00000000000000000000000000000000".to_string());
    let access_token = account
        .map(|a| a.access_token.clone())
        .unwrap_or_else(|| "0".to_string());
    let user_type = if account.is_some() { "msa" } else { "legacy" };
    let substitutions = [
        ("${auth_player_name}", player_name.as_str()),
        ("${version_name}", version.id.as_str()),
        ("${game_directory}", game_dir.to_str().unwrap_or(".")),
        ("${assets_root}", assets_dir.to_str().unwrap_or(".")),
        ("${assets_index_name}", version.asset_index.as_ref().map(|a| a.id.as_str()).unwrap_or("")),
        ("${auth_uuid}", player_uuid.as_str()),
        ("${auth_access_token}", access_token.as_str()),
        ("${auth_session}", access_token.as_str()),
        ("${user_type}", user_type),
        ("${version_type}", version.version_type.as_str()),
        ("${user_properties}", "{}"),
    ];
    let subst = |s: &str| -> String {
        let mut out = s.to_string();
        for (k, v) in substitutions {
            out = out.replace(k, v);
        }
        out
    };
    game_args = game_args.into_iter().map(|a| subst(&a)).collect();
    jvm_args = jvm_args.into_iter().map(|a| subst(&a)).collect();

    // Memory from instance.cfg if present.
    let cfg_path = inst_dir.join("instance.cfg");
    let cfg = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let mem = |key: &str, default: u32| -> u32 {
        cfg.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split('=').nth(1))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(default)
    };

    classpath.push(inst_dir.join("minecraft").join("client.jar"));

    Some(LaunchProfile {
        java_path: java_path.to_path_buf(),
        min_mem_mb: mem("MinMemAlloc", 512),
        max_mem_mb: mem("MaxMemAlloc", 4096),
        game_dir,
        assets_dir,
        main_class: version.main_class,
        classpath,
        jvm_args,
        game_args,
    })
}

fn collect_strings(args: Option<&Vec<serde_json::Value>>) -> Vec<String> {
    let Some(args) = args else { return Vec::new() };
    let mut out = Vec::new();
    for a in args {
        if let Some(s) = a.as_str() {
            out.push(s.to_string());
        } else if let Some(obj) = a.as_object() {
            // Rules-based args: keep only unconditional / allowed ones.
            if let Some(values) = obj.get("value") {
                if let Some(s) = values.as_str() {
                    out.push(s.to_string());
                } else if let Some(arr) = values.as_array() {
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            out.push(s.to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_command() {
        let p = LaunchProfile {
            java_path: "/usr/bin/java".into(),
            min_mem_mb: 512,
            max_mem_mb: 1024,
            main_class: "Main".into(),
            ..Default::default()
        };
        let cmd = p.jvm_command();
        let dbg = format!("{:?}", cmd);
        assert!(dbg.contains("java"));
    }
    #[test]
    fn collect_args() {
        let vals: Vec<serde_json::Value> = vec![
            serde_json::json!("-Dfoo=bar"),
            serde_json::json!({"rules": [], "value": ["-Xss1M"]}),
        ];
        let got = collect_strings(Some(&vals));
        assert_eq!(got, vec!["-Dfoo=bar", "-Xss1M"]);
    }
}
