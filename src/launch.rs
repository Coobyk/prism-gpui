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
    /// Optional wrapper command prepended to the JVM invocation (e.g.
    /// `gamemoderun`, `mangohud`, `prime-run`).
    pub wrapper: Vec<String>,
    /// Shell command run before the JVM starts.
    pub pre_launch: String,
    /// Shell command run after the game exits.
    pub post_exit: String,
    /// Maximum console lines to retain in memory.
    pub console_max_lines: usize,
}

impl LaunchProfile {
    pub fn jvm_command(&self) -> Command {
        let mut cmd = if self.wrapper.is_empty() {
            Command::new(&self.java_path)
        } else {
            let mut c = Command::new(&self.wrapper[0]);
            for a in &self.wrapper[1..] {
                c.arg(a);
            }
            c.arg(&self.java_path);
            c
        };
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

/// Split a shell-ish command string into argv, honouring single/double quotes.
pub fn split_command(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    for ch in s.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    cur.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    started = true;
                }
                c if c.is_whitespace() => {
                    if started || !cur.is_empty() {
                        out.push(std::mem::take(&mut cur));
                        started = false;
                    }
                }
                c => {
                    cur.push(c);
                    started = true;
                }
            },
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Run a pre-launch / pre-command hook in the instance's game directory.
pub fn run_hook(command: &str, game_dir: &Path) -> Result<(), String> {
    let argv = split_command(command);
    if argv.is_empty() {
        return Ok(());
    }
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(game_dir)
        .status()
        .map_err(|e| format!("hook {} failed: {e}", argv[0]))?;
    if !status.success() {
        return Err(format!("hook {} exited with {status}", argv[0]));
    }
    Ok(())
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

    // Memory + JVM args: instance.cfg overrides global settings, which
    // override built-in defaults.
    let cfg_path = inst_dir.join("instance.cfg");
    let cfg = std::fs::read_to_string(&cfg_path).unwrap_or_default();
    let cfg_get = |key: &str| -> Option<String> {
        cfg.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split('=').nth(1))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    let settings = crate::settings::GlobalSettings::load(data_root);
    let min_mem = cfg_get("MinMemAlloc")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| settings.min_mem());
    let max_mem = cfg_get("MaxMemAlloc")
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| settings.max_mem());

    // Global JVM args, then per-instance overrides.
    let mut extra_jvm = settings.jvm_args();
    if let Some(inst_args) = cfg_get("JvmArgs") {
        extra_jvm.extend(split_command(&inst_args));
    }
    jvm_args.extend(extra_jvm);

    let wrapper = if let Some(w) = cfg_get("WrapperCommand") {
        split_command(&w)
    } else {
        split_command(settings.wrapper_command())
    };

    let pre_launch = cfg_get("PreLaunchCommand")
        .unwrap_or_else(|| settings.pre_launch_command().to_string());
    let post_exit =
        cfg_get("PostExitCommand").unwrap_or_else(|| settings.post_exit_command().to_string());

    classpath.push(inst_dir.join("minecraft").join("client.jar"));

    Some(LaunchProfile {
        java_path: java_path.to_path_buf(),
        min_mem_mb: min_mem,
        max_mem_mb: max_mem,
        game_dir,
        assets_dir,
        main_class: version.main_class,
        classpath,
        jvm_args,
        game_args,
        wrapper,
        pre_launch,
        post_exit,
        console_max_lines: settings.console_max_lines(),
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
    fn split_command_handles_quotes() {
        assert_eq!(split_command("gamemoderun mangohud"), vec!["gamemoderun", "mangohud"]);
        assert_eq!(
            split_command("env FOO='a b' bar"),
            vec!["env", "FOO=a b", "bar"]
        );
        assert_eq!(split_command("  "), Vec::<String>::new());
        assert_eq!(split_command("\"quoted path\"/bin"), vec!["quoted path/bin"]);
    }

    #[test]
    fn wrapper_is_prepended() {
        let p = LaunchProfile {
            java_path: "/usr/bin/java".into(),
            main_class: "Main".into(),
            wrapper: vec!["gamemoderun".into()],
            ..Default::default()
        };
        let dbg = format!("{:?}", p.jvm_command());
        let gi = dbg.find("gamemoderun").unwrap();
        let ji = dbg.find("java").unwrap();
        assert!(gi < ji, "wrapper should come first: {dbg}");
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
