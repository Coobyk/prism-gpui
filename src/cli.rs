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
        Some("search-type") => {
            let kind = args.get(1).ok_or("usage: search-type <mod|modpack|resourcepack|shader> <query>")?;
            let q = args.get(2).ok_or("usage: search-type <type> <query>")?;
            cmd_search_typed(kind, q);
            Ok(())
        }
        Some("check-mods") => {
            let name = args.get(1).ok_or("usage: check-mods <instance>")?;
            cmd_check_mods(data_root, name)
        }
        Some("install-pack") => {
            let slug = args.get(1).ok_or("usage: install-pack <slug> [name]")?;
            let name = args.get(2).map(|s| s.as_str());
            cmd_install_pack(data_root, slug, name)
        }
        Some("login") => cmd_login(data_root),
        Some("accounts") => {
            let store = crate::accounts::AccountStore::load(data_root);
            if store.accounts.is_empty() {
                println!("(no accounts)");
            }
            for a in &store.accounts {
                let active = store.active.as_deref() == Some(a.uuid.as_str());
                println!("{}{:<20} {}", if active { "* " } else { "  " }, a.username, a.uuid);
            }
            Ok(())
        }
        Some("logout") => {
            let uuid = args.get(1).ok_or("usage: logout <uuid|username>")?;
            let mut store = crate::accounts::AccountStore::load(data_root);
            let target = store
                .accounts
                .iter()
                .find(|a| &a.uuid == uuid || &a.username == uuid)
                .map(|a| a.uuid.clone())
                .ok_or_else(|| format!("account not found: {uuid}"))?;
            store.remove(&target);
            store.save(data_root)?;
            println!("removed {uuid}");
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
        Some("components") => {
            let name = args.get(1).ok_or("usage: components <instance>")?;
            let inst = find_instance(data_root, name)?;
            if let Some(pack) = meta::read_component_pack(&inst.dir) {
                for c in pack.components {
                    println!("{:<32} {}", c.uid, c.version);
                }
            }
            Ok(())
        }
        Some("versions") => {
            let uid = args.get(1).ok_or("usage: versions <uid>")?;
            let idx = meta::list_versions(data_root, uid)
                .ok_or_else(|| format!("no metadata for {uid}"))?;
            println!("{} ({})", idx.name, idx.uid);
            for v in idx.versions.iter().take(40) {
                println!("  {:<16} {}{}", v.version, v.release_type, if v.recommended { " *" } else { "" });
            }
            Ok(())
        }
        Some("set-component") => {
            let name = args.get(1).ok_or("usage: set-component <instance> <uid> <version>")?;
            let uid = args.get(2).ok_or("usage: set-component <instance> <uid> <version>")?;
            let version = args.get(3).ok_or("usage: set-component <instance> <uid> <version>")?;
            let inst = find_instance(data_root, name)?;
            meta::set_component(&inst.dir, uid, version)?;
            println!("set {uid} = {version} in {name}");
            Ok(())
        }
        Some("add-fabric") => {
            let name = args.get(1).ok_or("usage: add-fabric <instance> [loader-version]")?;
            let inst = find_instance(data_root, name)?;
            let (game_version, _) = instance_game_loader(&inst.dir);
            let game_version = game_version.ok_or("could not determine Minecraft version")?;
            // Resolve loader version: explicit or recommended.
            let loader_version = match args.get(2) {
                Some(v) => v.clone(),
                None => meta::list_versions(data_root, "net.fabricmc.fabric-loader")
                    .and_then(|idx| {
                        idx.versions
                            .iter()
                            .find(|v| v.recommended)
                            .or_else(|| idx.versions.first())
                            .map(|v| v.version.clone())
                    })
                    .ok_or("no fabric loader metadata")?,
            };
            meta::set_component(&inst.dir, "net.fabricmc.intermediary", &game_version)?;
            meta::set_component(&inst.dir, "net.fabricmc.fabric-loader", &loader_version)?;
            println!("added Fabric {loader_version} (intermediary {game_version}) to {name}");
            Ok(())
        }
        Some("open") => {
            let name = args.get(1).ok_or("usage: open <instance>")?;
            let inst = find_instance(data_root, name)?;
            crate::desktop::open_path(&inst.dir.join("minecraft"))?;
            println!("opened {}", inst.dir.display());
            Ok(())
        }
        Some("worlds") => {
            let name = args.get(1).ok_or("usage: worlds <instance>")?;
            let inst = find_instance(data_root, name)?;
            for w in crate::worlds::list_worlds(&inst.dir) {
                println!(
                    "{:<30} {:<8} {}",
                    w.name,
                    if w.has_level_dat { "valid" } else { "?" },
                    human_size(w.size_bytes)
                );
            }
            Ok(())
        }
        Some("world-rm") => {
            let name = args.get(1).ok_or("usage: world-rm <instance> <world>")?;
            let world = args.get(2).ok_or("usage: world-rm <instance> <world>")?;
            let inst = find_instance(data_root, name)?;
            crate::worlds::delete_world(&inst.dir, world)?;
            println!("deleted world {world}");
            Ok(())
        }
        Some("check-update") => {
            let r = crate::updater::latest_release()?;
            println!("latest Prism Launcher: {} ({})", r.tag_name, r.html_url);
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
    println!("  components <instance>  list components");
    println!("  versions <uid>    list available versions for a component");
    println!("  add-fabric <instance> [loader]  install Fabric loader into an instance");
    println!("  search-type <type> <query>  search Modrinth by project type");
    println!("  check-mods <instance>  look up installed mods on Modrinth");
    println!("  install-pack <slug> [name]  install a Modrinth modpack");
    println!("  login             sign in with a Microsoft account");
    println!("  accounts          list saved accounts");
    println!("  logout <uuid|name>  remove an account");
    println!("  set-component <instance> <uid> <version>  change a component version");
    println!("  open <instance>   open the instance folder");
    println!("  worlds <instance>  list worlds");
    println!("  world-rm <instance> <world>  delete a world");
    println!("  check-update      check the latest Prism Launcher release");
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

fn cmd_login(data_root: &Path) -> Result<(), String> {
    let auth = crate::auth::AuthService::new();
    let account = auth.login_interactive(|dc| {
        println!(
            "To sign in, open {} and enter the code: {}",
            dc.verification_uri, dc.user_code
        );
    })?;
    let mut store = crate::accounts::AccountStore::load(data_root);
    store.upsert(account.clone());
    store.save(data_root)?;
    println!("signed in as {} ({})", account.username, account.uuid);
    Ok(())
}

fn cmd_search_typed(kind: &str, query: &str) {
    let provider = Modrinth;
    match provider.search_typed(query, Some(kind), None, None) {
        Ok(hits) => {
            if hits.is_empty() {
                println!("(no results)");
            }
            for h in hits {
                println!("{:<16} {:<28} {}", h.project_type, h.slug, h.title);
            }
        }
        Err(e) => println!("search failed: {e}"),
    }
}

fn cmd_check_mods(data_root: &Path, name: &str) -> Result<(), String> {
    let inst = find_instance(data_root, name)?;
    let mods_dir = inst.dir.join("minecraft").join("mods");
    let provider = Modrinth;
    let mut checked = 0;
    for path in crate::fs::list_files(&mods_dir) {
        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
        if !matches!(ext.as_deref(), Some("jar")) {
            continue;
        }
        let Some(sha1) = crate::net::sha1_file(&path) else {
            continue;
        };
        checked += 1;
        match provider.version_from_hash(&sha1) {
            Ok(version) => {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                println!(
                    "{:<40} current {} ({})",
                    file_name,
                    version.version_number,
                    version.game_versions.first().cloned().unwrap_or_default()
                );
            }
            Err(_) => {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                println!("{file_name:<40} (not on Modrinth / unknown)");
            }
        }
    }
    println!("checked {checked} mods");
    Ok(())
}

fn cmd_install_pack(data_root: &Path, slug: &str, name: Option<&str>) -> Result<(), String> {
    let provider = Modrinth;
    let versions = provider.versions(slug, None, None)?;
    let version = versions.first().ok_or("no versions for pack")?;
    let instances_root = data_root.join("instances");
    let dir_name = name.unwrap_or(slug).replace(['/', '\\'], "_");
    let inst_dir = instances_root.join(&dir_name);
    std::fs::create_dir_all(&inst_dir).map_err(|e| e.to_string())?;
    provider.download_version_file(version, &inst_dir)?;
    // The downloaded file is the .mrpack; locate it by extension.
    let downloaded = crate::fs::list_files(&inst_dir)
        .into_iter()
        .find(|p| p.extension().map(|e| e == "mrpack").unwrap_or(false))
        .ok_or("pack download did not produce a .mrpack")?;
    crate::pack::mrpack::install(&downloaded, &inst_dir)?;
    let cfg = "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nMinMemAlloc=512\nMaxMemAlloc=4096\n";
    std::fs::write(inst_dir.join("instance.cfg"), cfg).map_err(|e| e.to_string())?;
    // Persist pack dependencies into mmc-pack.json.
    if let Ok(m) = crate::pack::mrpack::manifest_from_pack(&downloaded) {
        if let Some(mc) = &m.dependencies.minecraft {
            let _ = meta::set_component(&inst_dir, "net.minecraft", mc);
        }
        if let Some(f) = &m.dependencies.fabric_loader {
            let _ = meta::set_component(&inst_dir, "net.fabricmc.fabric-loader", f);
        }
        if let Some(f) = &m.dependencies.forge {
            let _ = meta::set_component(&inst_dir, "net.minecraftforge", f);
        }
    }
    println!("installed pack {} → {}", slug, inst_dir.display());
    Ok(())
}

fn find_instance(data_root: &Path, name: &str) -> Result<instance::Instance, String> {
    instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
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
