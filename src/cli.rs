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
        Some("install") => {
            let name = args.get(1).ok_or("usage: install <instance> <slug>")?;
            let slug = args.get(2).ok_or("usage: install <instance> <slug>")?;
            cmd_install(data_root, name, slug)
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
    println!("  search <query>    search Modrinth");
    println!("  install <instance> <slug>  install a Modrinth mod");
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
