//! Headless CLI mode: useful for servers, testing, and scripting.

use crate::modplatform::{ModProvider, modrinth::Modrinth};
use crate::{instance, java, launch, meta};
use std::path::Path;

pub fn run(args: &[String], data_root: &Path) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("list") => cmd_list(data_root),
        Some("java") => cmd_java(data_root),
        Some("search") => {
            let q = args.get(1).ok_or("usage: search <query>")?;
            cmd_search(q);
            Ok(())
        }
        Some("launch") => {
            let name = args.get(1).ok_or("usage: launch <instance>")?;
            cmd_launch(data_root, name)
        }
        Some("create") => {
            let name = args.get(1).ok_or("usage: create <name> <mc-version>")?;
            let ver = args.get(2).ok_or("usage: create <name> <mc-version>")?;
            let dir = instance::create_instance(data_root, name, ver)?;
            println!("created {}", dir.display());
            Ok(())
        }
        Some("rm") | Some("remove") => {
            let name = args.get(1).ok_or("usage: rm <name>")?;
            instance::remove_instance(data_root, name)?;
            println!("removed {name}");
            Ok(())
        }
        Some("rename") => {
            let from = args.get(1).ok_or("usage: rename <from> <to>")?;
            let to = args.get(2).ok_or("usage: rename <from> <to>")?;
            let dir = instance::rename_instance(data_root, from, to)?;
            println!("renamed to {}", dir.display());
            Ok(())
        }
        Some("duplicate") | Some("dup") => {
            let from = args.get(1).ok_or("usage: duplicate <from> <to>")?;
            let to = args.get(2).ok_or("usage: duplicate <from> <to>")?;
            let dir = instance::duplicate_instance(data_root, from, to)?;
            println!("duplicated to {}", dir.display());
            Ok(())
        }
        Some("java-download") => {
            let comp = args
                .get(1)
                .ok_or("usage: java-download <component> <major>")?;
            let major = args.get(2).ok_or("usage: java-download <component> <major>")?;
            let bin = java::ensure_runtime(data_root, comp, major)?;
            println!("java: {}", bin.display());
            Ok(())
        }
        Some("news") => {
            cmd_news()
        }
        Some("log") => {
            let name = args.get(1).ok_or("usage: log <instance> [lines]")?;
            let lines = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(50);
            cmd_log(data_root, name, lines)
        }
        Some("update") => {
            let name = args.get(1).ok_or("usage: update <instance>")?;
            cmd_update(data_root, name)
        }
        Some("assets") => {
            let name = args.get(1).ok_or("usage: assets <instance> [limit]")?;
            let limit = args.get(2).and_then(|s| s.parse::<usize>().ok());
            cmd_assets(data_root, name, limit)
        }
        Some("install") => {
            let name = args.get(1).ok_or("usage: install <instance> <slug>")?;
            let slug = args.get(2).ok_or("usage: install <instance> <slug>")?;
            cmd_install(data_root, name, slug)
        }
        Some("packinfo") => {
            let path = args.get(1).ok_or("usage: packinfo <file>")?;
            cmd_packinfo(Path::new(path))
        }
        Some("import") => {
            let path = args.get(1).ok_or("usage: import <file> [name]")?;
            let name = args.get(2).map(|s| s.as_str());
            cmd_import(data_root, Path::new(path), name)
        }
        Some("export") => {
            let name = args.get(1).ok_or("usage: export <instance> <file> [format]")?;
            let file = args.get(2).ok_or("usage: export <instance> <file> [format]")?;
            let format = args.get(3).map(|s| s.as_str()).unwrap_or("mrpack");
            cmd_export(data_root, name, Path::new(file), format)
        }
        Some("help") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown command: {other}")),
    }
}

fn print_help() {
    println!("prism-gpui <command>");
    println!("  list              list instances");
    println!("  java              list Java installations");
    println!("  java-download <component> <major>  download a Java runtime");
    println!("  create <name> <mc-version>  create an instance");
    println!("  rm <name>         delete an instance");
    println!("  rename <from> <to>  rename an instance");
    println!("  duplicate <from> <to>  duplicate an instance");
    println!("  update <instance>  download missing libraries and assets");
    println!("  assets <instance> [limit]  download missing assets");
    println!("  search <query>    search Modrinth");
    println!("  install <instance> <slug>  install a Modrinth mod");
    println!("  packinfo <file>   inspect a modpack");
    println!("  import <file> [name]  import a modpack");
    println!("  export <instance> <file> [format]  export (mrpack|curseforge|mmc)");
    println!("  news              show the Prism news feed");
    println!("  log <instance> [lines]  tail an instance log");
    println!("  launch <instance> launch an instance (foreground)");
}

fn cmd_list(data_root: &Path) -> Result<(), String> {
    let instances = instance::discover_instances(data_root);
    if instances.is_empty() {
        println!("(no instances in {})", data_root.display());
    }
    for inst in instances {
        let mut version = "?".to_string();
        let mut loader = "vanilla".to_string();
        if let Some(pack) = meta::read_component_pack(&inst.dir) {
            for c in &pack.components {
                if c.uid == "net.minecraft" {
                    version = c.version.clone();
                } else if c.uid != "org.lwjgl3" && !c.dependency_only {
                    loader = format!("{} {}", c.cached_name, c.version);
                }
            }
        }
        println!(
            "{:<32} {:<12} {}  ({}–{} MB)",
            inst.name, version, loader, inst.min_mem, inst.max_mem
        );
    }
    Ok(())
}

fn cmd_java(data_root: &Path) -> Result<(), String> {
    let installs = java::discover_java(data_root);
    for j in installs {
        println!(
            "{:<50} {:<12} {:<16} {}",
            j.path.display(),
            j.version,
            j.vendor,
            j.arch
        );
    }
    Ok(())
}

fn cmd_search(query: &str) {
    let provider = Modrinth;
    let hits = provider.search(query);
    if hits.is_empty() {
        println!("no results (or network error)");
    }
    for h in hits {
        println!("{:<40} {:<30} {} dl", h.title, h.slug, h.downloads);
    }
}

fn cmd_update(data_root: &Path, name: &str) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let pack = meta::read_component_pack(&inst.dir).ok_or("no mmc-pack.json")?;
    let meta_dir = data_root.join("meta");
    let mut total = 0usize;
    for c in &pack.components {
        let path = meta_dir.join(&c.uid).join(format!("{}.json", c.version));
        let Some(v) = meta::read_version_json(&path) else {
            continue;
        };
        total += meta::ensure_libraries(data_root, &v)?;
        meta::ensure_asset_index(data_root, &v)?;
    }
    println!("updated {name}: {total} files downloaded");
    Ok(())
}

fn cmd_news() -> Result<(), String> {
    let entries = crate::news::fetch()?;
    if entries.is_empty() {
        println!("(no news entries)");
    }
    for e in entries {
        println!("{} — {}", e.title, e.url);
    }
    Ok(())
}

fn cmd_log(data_root: &Path, name: &str, lines: usize) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let log = inst.dir.join("minecraft").join("logs").join("latest.log");
    let text = std::fs::read_to_string(&log)
        .map_err(|e| format!("could not read {}: {e}", log.display()))?;
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    for line in &all[start..] {
        println!("{line}");
    }
    Ok(())
}

fn cmd_packinfo(path: &Path) -> Result<(), String> {
    let manifest = match crate::pack::detect_format(path) {
        "mrpack" => crate::pack::mrpack::manifest_from_pack(path),
        "zip" => crate::pack::curseforge::manifest_from_pack(path)
            .or_else(|_| Err::<crate::pack::PackManifest, _>("not a recognized pack".to_string())),
        _ => Err("unsupported file type".to_string()),
    };
    match manifest {
        Ok(m) => {
            println!("name:    {}", m.name);
            println!("version: {}", m.version);
            println!("minecraft: {:?}", m.dependencies.minecraft);
            println!("fabric:  {:?}", m.dependencies.fabric_loader);
            println!("forge:   {:?}", m.dependencies.forge);
            println!("files:   {}", m.files.len());
            Ok(())
        }
        Err(e) => Err(format!("could not read pack: {e}")),
    }
}

fn cmd_import(data_root: &Path, path: &Path, name: Option<&str>) -> Result<(), String> {
    let instances_root = data_root.join("instances");
    match crate::pack::detect_format(path) {
        "mrpack" => {
            let m = crate::pack::mrpack::manifest_from_pack(path)?;
            let dir_name = name.unwrap_or(&m.name).replace(['/', '\\'], "_");
            let inst_dir = instances_root.join(&dir_name);
            crate::pack::mrpack::install(path, &inst_dir)?;
            write_instance_cfg(&inst_dir)?;
            println!("imported {} → {}", m.name, inst_dir.display());
            Ok(())
        }
        "zip" => {
            if crate::pack::curseforge::manifest_from_pack(path).is_ok() {
                let m = crate::pack::curseforge::manifest_from_pack(path)?;
                let dir_name = name.unwrap_or(&m.name).replace(['/', '\\'], "_");
                let inst_dir = instances_root.join(&dir_name);
                crate::pack::curseforge::extract_overrides(path, &inst_dir)?;
                write_instance_cfg(&inst_dir)?;
                println!("imported {} → {}", m.name, inst_dir.display());
                Ok(())
            } else {
                let dir_name = name.unwrap_or("Imported");
                let created = crate::pack::mmc::import(path, &instances_root, dir_name)?;
                println!("imported → {}", created.display());
                Ok(())
            }
        }
        _ => Err("unsupported file type".into()),
    }
}

fn write_instance_cfg(inst_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(inst_dir).map_err(|e| e.to_string())?;
    let cfg = "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nMinMemAlloc=512\nMaxMemAlloc=4096\n";
    std::fs::write(inst_dir.join("instance.cfg"), cfg).map_err(|e| e.to_string())?;
    Ok(())
}

fn cmd_export(data_root: &Path, name: &str, out: &Path, format: &str) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    match format {
        "mrpack" => crate::pack::mrpack::export(&inst.dir, out, &inst.name, "1.0.0")?,
        "curseforge" | "cf" => crate::pack::curseforge::export(&inst.dir, out, &inst.name, "1.0.0")?,
        "mmc" | "zip" => crate::pack::mmc::export(&inst.dir, out)?,
        other => return Err(format!("unknown export format: {other}")),
    }
    println!("exported {} → {}", inst.name, out.display());
    Ok(())
}

fn cmd_assets(data_root: &Path, name: &str, limit: Option<usize>) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let (game_version, _) = instance_game_loader(&inst.dir);
    let Some(gv) = game_version else {
        return Err("could not determine Minecraft version".into());
    };
    let index_path = data_root
        .join("assets")
        .join("indexes")
        .join(format!("{gv}.json"));
    if !index_path.is_file() {
        return Err(format!(
            "asset index missing at {} (run `update {name}` first)",
            index_path.display()
        ));
    }
    let n = meta::ensure_assets(data_root, &index_path, limit)?;
    println!("downloaded {n} assets");
    Ok(())
}

fn cmd_install(data_root: &Path, name: &str, slug: &str) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let (game_version, loader) = instance_game_loader(&inst.dir);
    let provider = Modrinth;
    let versions = provider
        .versions(slug, loader.as_deref(), game_version.as_deref())
        .map_err(|e| format!("modrinth: {e}"))?;
    let version = versions
        .first()
        .ok_or_else(|| format!("no compatible version for {slug}"))?;
    let mods_dir = inst.dir.join("minecraft").join("mods");
    let path = provider.download_version_file(version, &mods_dir)?;
    println!("installed {} {} → {}", slug, version.version_number, path.display());
    Ok(())
}

fn instance_game_loader(dir: &Path) -> (Option<String>, Option<String>) {
    let mut game = None;
    let mut loader = None;
    if let Some(pack) = meta::read_component_pack(dir) {
        for c in &pack.components {
            if c.uid == "net.minecraft" {
                game = Some(c.version.clone());
            } else if c.uid == "net.fabricmc.fabric-loader" {
                loader = Some("fabric".to_string());
            } else if c.uid == "net.minecraftforge" {
                loader = Some("forge".to_string());
            } else if c.uid == "net.neoforged" {
                loader = Some("neoforge".to_string());
            } else if c.uid == "org.quiltmc.quilt-loader" {
                loader = Some("quilt".to_string());
            }
        }
    }
    (game, loader)
}

fn cmd_launch(data_root: &Path, name: &str) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let java = java::discover_java(data_root)
        .into_iter()
        .next()
        .ok_or("no Java installation found")?;
    let profile = launch::build_profile(&inst.dir, data_root, &java.path)
        .ok_or("could not build launch profile")?;
    println!(
        "launching {} with {} ({} jars, main {})",
        inst.name,
        java.path.display(),
        profile.classpath.len(),
        profile.main_class
    );
    let status = profile
        .jvm_command()
        .status()
        .map_err(|e| format!("spawn failed: {e}"))?;
    println!("exit: {status}");
    Ok(())
}
