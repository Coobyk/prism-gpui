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
    /// Data root, used to fetch assets at launch time.
    pub data_root: PathBuf,
    /// Minecraft version, used to fetch assets at launch time.
    pub mc_version: String,
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
    /// Fetch anything the JVM needs that is missing locally: the vanilla
    /// client jar and the asset objects. Mirrors Prism's launch-time downloads.
    /// Returns the number of files fetched.
    pub fn ensure_ready(&self) -> Result<usize, String> {
        let mut n = 0;
        let meta_path = self
            .data_root
            .join("meta")
            .join("net.minecraft")
            .join(format!("{}.json", self.mc_version));
        if let Some(v) = meta::read_version_json(&meta_path) {
            if meta::ensure_vanilla_client(&self.data_root, &v)? {
                n += 1;
            }
        }
        n += meta::ensure_all_assets(&self.data_root, &self.mc_version).unwrap_or(0);
        Ok(n)
    }

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

/// Extract natives jars into `dest` unless the stamp file already records the
/// same set of jars. Failures are reported but do not abort the launch (some
/// instances use libraries that ship no natives).
pub fn prepare_natives(dest: &Path, jars: &[PathBuf]) -> usize {
    if jars.is_empty() {
        return 0;
    }
    let stamp = dest.join(".jars");
    let signature = jars
        .iter()
        .map(|p| {
            let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            format!("{}:{size}", p.display())
        })
        .collect::<Vec<_>>()
        .join("\n");
    if std::fs::read_to_string(&stamp).map(|s| s == signature).unwrap_or(false) {
        return 0;
    }
    if std::fs::create_dir_all(dest).is_err() {
        return 0;
    }
    match extract_natives(dest, jars) {
        Ok(n) => {
            let _ = std::fs::write(&stamp, signature.as_bytes());
            n
        }
        Err(e) => {
            eprintln!("warning: could not extract natives: {e}");
            0
        }
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
    let mut loader_game_args: Vec<serde_json::Value> = Vec::new();
    let mut loader_game_args_raw: Option<String> = None;
    let mut natives_jars: Vec<PathBuf> = Vec::new();
    let mut saw_minecraft = false;
    let mut mc_version: Option<String> = None;

    // Components are ordered dependency-first; later ones (loader) override.
    for comp in &pack.components {
        let candidate = meta_dir.join(&comp.uid).join(format!("{}.json", &comp.version));
        let Some(v) = meta::read_version_json(&candidate) else {
            continue;
        };
        if comp.uid == "net.minecraft" {
            saw_minecraft = true;
            mc_version = Some(comp.version.clone());
            version.main_class = v.main_class.clone();
            version.asset_index = v.asset_index.clone();
            version.arguments = v.arguments.clone();
            version.minecraft_arguments = v.minecraft_arguments.clone();
            version.version_type = v.version_type.clone();
            version.id = v.id.clone();
        } else {
            // A loader (Fabric/Quilt/Forge/NeoForge) replaces the main class
            // and, when it declares its own arguments, the whole game argument
            // list — Forge's `--launchTarget forgeclient` and friends are
            // mandatory for ForgeWrapper.
            if !v.main_class.is_empty() {
                version.main_class = v.main_class.clone();
            }
            if let Some(args) = &v.arguments {
                if !args.game.is_empty() {
                    loader_game_args = args.game.clone();
                    loader_game_args_raw = None;
                }
            }
            if let Some(raw) = &v.minecraft_arguments {
                if !raw.is_empty() {
                    loader_game_args_raw = Some(raw.clone());
                }
            }
            jvm_args.extend(collect_strings(Some(&v.extra_jvm_args)));
            loader_game_args.extend(v.extra_game_args.clone());
        }
        for lib in &v.libraries {
            if !lib.applies() {
                continue;
            }
            if let Some(p) = lib.maven_path() {
                let full = libraries_dir.join(p);
                if full.is_file() && !classpath.contains(&full) {
                    classpath.push(full);
                }
            }
        }
        // Modern (and legacy) natives jars are extracted for
        // `java.library.path` and also placed on the classpath, which is how
        // LWJGL 3 finds its shared objects.
        for native in meta::natives_artifacts(&libraries_dir, &v) {
            if !classpath.contains(&native) {
                classpath.push(native.clone());
            }
            if !natives_jars.contains(&native) {
                natives_jars.push(native);
            }
        }
        jvm_args.extend(collect_strings(v.arguments.as_ref().map(|a| &a.jvm)));
    }
    if !saw_minecraft {
        return None;
    }

    // Extract natives once per instance; a stamp file records which jars were
    // used so subsequent launches skip the work.
    let inst_name = inst_dir
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "instance".to_string());
    let natives_dir = data_root.join("natives").join(&inst_name);
    prepare_natives(&natives_dir, &natives_jars);

    let game_dir = inst_dir.join("minecraft");
    let assets_dir = data_root.join("assets");

    // Substitute Mojang argument placeholders.
    let mut game_args = if !loader_game_args.is_empty() {
        collect_strings(Some(&loader_game_args))
    } else if let Some(raw) = &loader_game_args_raw {
        raw.split_whitespace().map(|s| s.to_string()).collect()
    } else {
        let vanilla = collect_strings(version.arguments.as_ref().map(|a| &a.game));
        if vanilla.is_empty() {
            version
                .minecraft_arguments
                .as_deref()
                .map(|raw| raw.split_whitespace().map(|s| s.to_string()).collect())
                .unwrap_or_default()
        } else {
            vanilla
        }
    };
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
    let client_id = uuid::Uuid::new_v4().simple().to_string();

    // Natives live per instance so parallel launches do not race on the same
    // extracted files.
    let classpath_string = classpath
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":");

    let substitutions = [
        ("${auth_player_name}", player_name.as_str()),
        ("${version_name}", version.id.as_str()),
        ("${game_directory}", game_dir.to_str().unwrap_or(".")),
        ("${assets_root}", assets_dir.to_str().unwrap_or(".")),
        ("${assets_index_name}", version.asset_index.as_ref().map(|a| a.id.as_str()).unwrap_or("")),
        ("${auth_uuid}", player_uuid.as_str()),
        ("${auth_xuid}", ""),
        ("${auth_access_token}", access_token.as_str()),
        ("${auth_session}", access_token.as_str()),
        // The client id is a fresh per-launch identifier; Xbox user id is only
        // meaningful for Microsoft accounts and stays empty otherwise.
        ("${clientid}", client_id.as_str()),
        ("${user_type}", user_type),
        ("${version_type}", version.version_type.as_str()),
        ("${user_properties}", "{}"),
        ("${natives_directory}", natives_dir.to_str().unwrap_or(".")),
        ("${launcher_name}", "prism-gpui"),
        ("${launcher_version}", env!("CARGO_PKG_VERSION")),
        ("${classpath}", classpath_string.as_str()),
        ("${classpath_separator}", ":"),
        ("${library_directory}", libraries_dir.to_str().unwrap_or(".")),
    ];
    let subst = |s: &str| -> String {
        let mut out = s.to_string();
        for (k, v) in substitutions {
            out = out.replace(k, v);
        }
        out
    };
    game_args = game_args.into_iter().map(|a| subst(&a)).collect();
    // The vanilla JVM arguments contain `-cp ${classpath}`; the launcher builds
    // its own classpath, so drop those tokens to avoid a conflicting stub.
    jvm_args = jvm_args
        .into_iter()
        .filter(|a| !matches!(a.as_str(), "-cp" | "-classpath" | "${classpath}"))
        .map(|a| subst(&a))
        .collect();

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

    // Vanilla client jar: shared library cache, or a legacy per-instance copy.
    // Any missing pieces are fetched by `LaunchProfile::ensure_ready`, which
    // runs off the UI thread right before the JVM starts.
    let mc_version = mc_version.clone().unwrap_or_else(|| version.id.clone());
    classpath.push(meta::client_jar_for(inst_dir, data_root, &mc_version));

    Some(LaunchProfile {
        java_path: java_path.to_path_buf(),
        min_mem_mb: min_mem,
        max_mem_mb: max_mem,
        game_dir,
        assets_dir,
        data_root: data_root.to_path_buf(),
        mc_version,
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
            // Rules-based args: keep only those whose rules allow the current
            // platform (e.g. `-XstartOnFirstThread` is macOS-only).
            if let Some(rules) = obj.get("rules").and_then(|r| r.as_array()) {
                if !crate::meta::rules_allow(rules) {
                    continue;
                }
            }
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

/// Extract every natives jar into `dest`, skipping the declared exclusions.
/// Returns the number of files extracted.
pub fn extract_natives(dest: &Path, jars: &[PathBuf]) -> Result<usize, String> {
    let mut count = 0usize;
    for jar in jars {
        let file = std::fs::File::open(jar).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            if entry.is_dir() {
                continue;
            }
            let Some(path) = entry.enclosed_name() else {
                continue;
            };
            let rel = path.to_string_lossy().replace('\\', "/");
            // Natives jars put everything under the root; ignore META-INF and
            // signature files, which Mojang's own extractor excludes.
            if rel.starts_with("META-INF/") || rel.ends_with(".git") {
                continue;
            }
            let Some(target) = crate::fs::safe_join(dest, &rel) else {
                continue;
            };
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
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

    #[test]
    fn collect_args_honours_os_rules() {
        let vals: Vec<serde_json::Value> = vec![
            serde_json::json!({"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": "-XstartOnFirstThread"}),
            serde_json::json!({"rules": [{"action": "allow", "os": {"name": "linux"}}], "value": "-Dlinux=1"}),
            serde_json::json!({"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"}),
        ];
        let got = collect_strings(Some(&vals));
        if cfg!(target_os = "macos") {
            assert!(got.contains(&"-XstartOnFirstThread".to_string()));
        } else {
            assert!(!got.contains(&"-XstartOnFirstThread".to_string()));
        }
        if cfg!(target_os = "linux") {
            assert!(got.contains(&"-Dlinux=1".to_string()));
        }
        assert!(!got.contains(&"--demo".to_string()));
    }

    #[test]
    fn prepare_natives_extracts_and_stamps() {
        let td = tempfile::tempdir().unwrap();
        let jar = td.path().join("natives.jar");
        {
            let f = std::fs::File::create(&jar).unwrap();
            let mut z = zip::ZipWriter::new(f);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            z.start_file("libfoo.so", opts).unwrap();
            std::io::Write::write_all(&mut z, b"native").unwrap();
            z.start_file("META-INF/MANIFEST.MF", opts).unwrap();
            std::io::Write::write_all(&mut z, b"ignored").unwrap();
            z.finish().unwrap();
        }
        let dest = td.path().join("natives");
        let n = prepare_natives(&dest, std::slice::from_ref(&jar));
        assert_eq!(n, 1, "only libfoo.so should be extracted");
        assert!(dest.join("libfoo.so").is_file());
        assert!(!dest.join("META-INF/MANIFEST.MF").exists());
        // A second run is a no-op thanks to the stamp file.
        assert_eq!(prepare_natives(&dest, std::slice::from_ref(&jar)), 0);
    }

    #[test]
    fn loader_component_overrides_main_class_and_args() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        let inst = root.join("instances/demo");
        std::fs::create_dir_all(&inst).unwrap();
        std::fs::write(
            inst.join("mmc-pack.json"),
            r#"{"components":[
                {"uid":"net.minecraft","version":"1.20.1"},
                {"uid":"net.minecraftforge","version":"47.2.0"}
            ]}"#,
        )
        .unwrap();
        let libs = root.join("libraries");
        std::fs::create_dir_all(libs.join("a/b/1")).unwrap();
        std::fs::write(libs.join("a/b/1/b-1.jar"), b"x").unwrap();

        let mc = root.join("meta/net.minecraft");
        std::fs::create_dir_all(&mc).unwrap();
        std::fs::write(
            mc.join("1.20.1.json"),
            r#"{"id":"1.20.1","mainClass":"net.minecraft.client.main.Main",
                "type":"release","assetIndex":{"id":"5","sha1":"","url":""},
                "arguments":{"jvm":["-Djava.library.path=${natives_directory}",
                                    {"rules":[{"action":"allow","os":{"name":"osx"}}],"value":"-XstartOnFirstThread"},
                                    "-cp","${classpath}"],
                             "game":["--username","${auth_player_name}"]},
                "libraries":[{"name":"a.b:b:1","downloads":{"artifact":{"path":"a/b/1/b-1.jar"}}}]}"#,
        )
        .unwrap();
        let forge = root.join("meta/net.minecraftforge");
        std::fs::create_dir_all(&forge).unwrap();
        std::fs::write(
            forge.join("47.2.0.json"),
            r#"{"uid":"net.minecraftforge","version":"47.2.0","name":"Forge",
                "mainClass":"io.github.zekerzhayard.forgewrapper.installer.Main",
                "minecraftArguments":"--username ${auth_player_name} --launchTarget forgeclient --fml.mcVersion 1.20.1",
                "libraries":[]}"#,
        )
        .unwrap();

        let profile =
            build_profile_with_account(&inst, root, Path::new("/usr/bin/java"), None).unwrap();
        assert_eq!(
            profile.main_class,
            "io.github.zekerzhayard.forgewrapper.installer.Main"
        );
        assert!(profile.game_args.iter().any(|a| a == "--launchTarget"));
        assert!(profile.game_args.iter().any(|a| a == "forgeclient"));
        assert!(profile.game_args.iter().any(|a| a == "1.20.1"));
        // macOS-only JVM argument is dropped elsewhere.
        let has_start = profile.jvm_args.iter().any(|a| a == "-XstartOnFirstThread");
        assert_eq!(has_start, cfg!(target_os = "macos"));
        // The vanilla `-cp ${classpath}` stub is removed; we build our own.
        assert!(!profile.jvm_args.iter().any(|a| a == "-cp"));
        // Natives placeholder is substituted with the per-instance directory.
        assert!(profile
            .jvm_args
            .iter()
            .any(|a| a.contains("natives") && !a.contains("${")));
    }
}
