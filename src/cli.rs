//! Headless CLI mode: useful for servers, testing, and scripting.

use crate::modplatform::{ModProvider, modrinth::Modrinth};
use crate::{instance, java, launch, meta};
use std::path::Path;

pub fn run(args: &[String], data_root: &Path) -> Result<(), String> {
    let settings = crate::settings::GlobalSettings::load(data_root);
    crate::net::set_proxy(settings.proxy_url());
    crate::modplatform::curseforge::set_api_key(settings.curseforge_api_key());
    // Native plugins may register behaviour that CLI commands rely on, so
    // initialise them once at start-up (manifest plugins are read on demand).
    let plugins = crate::plugin::PluginHost::discover(data_root);
    for warning in &plugins.warnings {
        eprintln!("warning: plugin: {warning}");
    }
    for warning in plugins.init_natives(data_root) {
        eprintln!("warning: plugin: {warning}");
    }
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
        Some("platforms") => {
            for p in crate::modplatform::Platform::ALL {
                let key = if p.needs_api_key() {
                    if crate::modplatform::curseforge::has_api_key() {
                        "api key: set"
                    } else {
                        "api key: missing"
                    }
                } else {
                    ""
                };
                println!(
                    "{:<12} mods={:<5} packs={:<5} {}",
                    p.label(),
                    p.supports_mods(),
                    p.supports_packs(),
                    key
                );
            }
            Ok(())
        }
        Some("search-platform") => {
            let platform = args
                .get(1)
                .ok_or("usage: search-platform <platform> <kind> <query> [mc] [loader]")?;
            let kind = args
                .get(2)
                .ok_or("usage: search-platform <platform> <kind> <query> [mc] [loader]")?;
            let query = args.get(3).map(|s| s.as_str()).unwrap_or("");
            let mc = args.get(4).map(|s| s.as_str());
            let loader = args.get(5).map(|s| s.as_str());
            cmd_search_platform(platform, kind, query, mc, loader)
        }
        Some("install-mod") => {
            let platform = args.get(1).ok_or("usage: install-mod <platform> <id> <instance>")?;
            let id = args.get(2).ok_or("usage: install-mod <platform> <id> <instance>")?;
            let inst = args.get(3).ok_or("usage: install-mod <platform> <id> <instance>")?;
            cmd_install_mod(data_root, platform, id, inst)
        }
        Some("install-ftb") => {
            let id = args.get(1).ok_or("usage: install-ftb <pack-id> [version] [name]")?;
            let version = args.get(2).map(|s| s.as_str());
            let name = args.get(3).map(|s| s.as_str());
            cmd_install_ftb(data_root, id, version, name)
        }
        Some("install-technic") => {
            let slug = args.get(1).ok_or("usage: install-technic <slug> [name]")?;
            let name = args.get(2).map(|s| s.as_str());
            cmd_install_technic(data_root, slug, name)
        }
        Some("install-atl") => {
            let safe = args.get(1).ok_or("usage: install-atl <safe-name> [version] [name]")?;
            let version = args.get(2).map(|s| s.as_str());
            let name = args.get(3).map(|s| s.as_str());
            cmd_install_atl(data_root, safe, version, name)
        }
        Some("install-packwiz") => {
            let url = args.get(1).ok_or("usage: install-packwiz <url|file> [name]")?;
            let name = args.get(2).map(|s| s.as_str());
            cmd_install_packwiz(data_root, url, name)
        }
        Some("install-pack-cf") => {
            let id = args.get(1).ok_or("usage: install-pack-cf <mod-id> [name]")?;
            let name = args.get(2).map(|s| s.as_str());
            cmd_install_pack_cf(data_root, id, name)
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
        Some("add-offline") => {
            let username = args.get(1).ok_or("usage: add-offline <username>")?;
            let account = crate::accounts::offline_account(username);
            let mut store = crate::accounts::AccountStore::load(data_root);
            store.upsert(account.clone());
            store.save(data_root)?;
            println!("added offline account {} ({})", account.username, account.uuid);
            Ok(())
        }
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
            let name = args.get(1).ok_or("usage: launch <instance> [--dry-run]")?;
            let dry_run = args.iter().any(|a| a == "--dry-run");
            cmd_launch(data_root, name, dry_run)
        }
        Some("profile") => {
            let name = args.get(1).ok_or("usage: profile <instance>")?;
            cmd_profile(data_root, name)
        }
        Some("create") => {
            let name = args.get(1).ok_or("usage: create <name> <mc-version>")?;
            let ver = args.get(2).ok_or("usage: create <name> <mc-version>")?;
            let dir = instance::create_instance(data_root, name, ver)?;
            match meta::ensure_minecraft(data_root, ver) {
                Ok(n) => println!("created {} ({n} Minecraft files)", dir.display()),
                Err(e) => println!("created {} (warning: {e})", dir.display()),
            }
            Ok(())
        }
        Some("mc-versions") => {
            let list = meta::minecraft_versions(data_root)?;
            let filter = args.get(1).map(|s| s.to_ascii_lowercase());
            let mut shown = 0;
            for v in &list {
                if let Some(f) = &filter {
                    if !v.id.to_ascii_lowercase().contains(f) && v.version_type != *f {
                        continue;
                    }
                }
                println!("  {:<20} {:<10} {}", v.id, v.version_type, v.release_time);
                shown += 1;
                if shown >= 40 {
                    break;
                }
            }
            println!("({} versions listed of {})", shown, list.len());
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
            let uid = args.get(1).ok_or("usage: versions <uid> [minecraft]")?;
            let mc = args.get(2).map(|s| s.as_str());
            if crate::metaindex::is_indexed_component(uid) {
                let mcs: Vec<String> = mc.map(|m| vec![m.to_string()]).unwrap_or_default();
                let list = crate::metaindex::versions_for(data_root, uid, &mcs)?;
                println!(
                    "{} ({}) — {} versions",
                    crate::metaindex::component_label(uid),
                    uid,
                    list.len()
                );
                for v in list.iter().take(40) {
                    println!(
                        "  {:<18} {:<10}{}",
                        v.version,
                        v.minecraft.as_deref().unwrap_or(""),
                        if v.recommended { " *" } else { "" }
                    );
                }
            } else {
                let idx = meta::list_versions(data_root, uid)
                    .ok_or_else(|| format!("no metadata for {uid}"))?;
                println!("{} ({})", idx.name, idx.uid);
                for v in idx.versions.iter().take(40) {
                    println!(
                        "  {:<16} {}{}",
                        v.version,
                        v.release_type,
                        if v.recommended { " *" } else { "" }
                    );
                }
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
            // Fabric's Minecraft constraint lives on the intermediary
            // dependency, so resolve the whole chain.
            let chain = match args.get(2) {
                Some(v) => vec![
                    crate::metaindex::ResolvedComponent {
                        uid: "net.fabricmc.intermediary".to_string(),
                        version: game_version.clone(),
                    },
                    crate::metaindex::ResolvedComponent {
                        uid: "net.fabricmc.fabric-loader".to_string(),
                        version: v.clone(),
                    },
                ],
                None => crate::metaindex::resolve_chain(
                    data_root,
                    "net.fabricmc.fabric-loader",
                    &game_version,
                    true,
                )?,
            };
            let mut missing = Vec::new();
            for c in &chain {
                meta::set_component(&inst.dir, &c.uid, &c.version)?;
                if let Err(e) = crate::metaindex::ensure_component(data_root, &c.uid, &c.version) {
                    missing.push(format!("{}: {e}", c.uid));
                }
            }
            let loader_version = chain
                .iter()
                .find(|c| c.uid == "net.fabricmc.fabric-loader")
                .map(|c| c.version.clone())
                .unwrap_or_default();
            println!("added Fabric {loader_version} (Minecraft {game_version}) to {name}");
            for m in missing {
                eprintln!("warning: {m}");
            }
            Ok(())
        }
        Some("add-forge") => {
            let name = args
                .get(1)
                .ok_or("usage: add-forge <instance> [forge-version] [--legacy]")?;
            let inst = find_instance(data_root, name)?;
            let explicit = args.get(2).filter(|a| !a.starts_with("--")).cloned();
            let legacy = args.iter().any(|a| a == "--legacy");
            let (game_version, _) = instance_game_loader(&inst.dir);
            let mc = game_version.ok_or("could not determine Minecraft version")?;
            let forge_version = match &explicit {
                Some(v) => v.clone(),
                None => crate::metaindex::resolve(data_root, "net.minecraftforge", &mc, true)?,
            };
            if legacy {
                // Run the vendor installer jar directly instead of using
                // ForgeWrapper, which is useful for debugging or old packs.
                let java = java::discover_java(data_root)
                    .into_iter()
                    .next()
                    .ok_or("no Java installation found")?;
                let url = crate::forge::forge_installer_url(&mc, &forge_version);
                println!("running Forge installer {mc}-{forge_version}…");
                crate::forge::run_installer(&url, &java.path, &inst.dir)?;
            } else {
                crate::metaindex::ensure_component(data_root, "net.minecraftforge", &forge_version)?;
            }
            meta::set_component(&inst.dir, "net.minecraftforge", &forge_version)?;
            println!("installed Forge {forge_version} into {name}");
            Ok(())
        }
        Some("add-quilt") => {
            let name = args.get(1).ok_or("usage: add-quilt <instance> [loader-version]")?;
            let inst = find_instance(data_root, name)?;
            let (game_version, _) = instance_game_loader(&inst.dir);
            let mc = game_version.ok_or("could not determine Minecraft version")?;
            let chain = match args.get(2) {
                Some(v) => vec![crate::metaindex::ResolvedComponent {
                    uid: "org.quiltmc.quilt-loader".to_string(),
                    version: v.clone(),
                }],
                None => crate::metaindex::resolve_chain(
                    data_root,
                    "org.quiltmc.quilt-loader",
                    &mc,
                    true,
                )?,
            };
            for c in &chain {
                meta::set_component(&inst.dir, &c.uid, &c.version)?;
                if let Err(e) = crate::metaindex::ensure_component(data_root, &c.uid, &c.version) {
                    eprintln!("warning: {}: {e}", c.uid);
                }
            }
            let loader_version = chain
                .iter()
                .find(|c| c.uid == "org.quiltmc.quilt-loader")
                .map(|c| c.version.clone())
                .unwrap_or_default();
            println!("added Quilt {loader_version} to {name}");
            Ok(())
        }
        Some("add-litelloader") => {
            let name = args
                .get(1)
                .ok_or("usage: add-litelloader <instance> [version]")?;
            let inst = find_instance(data_root, name)?;
            let (game_version, _) = instance_game_loader(&inst.dir);
            let mc = game_version.ok_or("could not determine Minecraft version")?;
            let chain = match args.get(2) {
                Some(v) => vec![crate::metaindex::ResolvedComponent {
                    uid: "com.mumfrey.liteloader".to_string(),
                    version: v.clone(),
                }],
                None => crate::metaindex::resolve_chain(
                    data_root,
                    "com.mumfrey.liteloader",
                    &mc,
                    true,
                )?,
            };
            for c in &chain {
                meta::set_component(&inst.dir, &c.uid, &c.version)?;
                if let Err(e) = crate::metaindex::ensure_component(data_root, &c.uid, &c.version) {
                    eprintln!("warning: {}: {e}", c.uid);
                }
            }
            let loader = chain
                .iter()
                .find(|c| c.uid == "com.mumfrey.liteloader")
                .map(|c| c.version.clone())
                .unwrap_or_default();
            println!("added LiteLoader {loader} (Minecraft {mc}) to {name}");
            Ok(())
        }
        Some("add-neoforge") => {
            let name = args
                .get(1)
                .ok_or("usage: add-neoforge <instance> [version] [--legacy]")?;
            let inst = find_instance(data_root, name)?;
            let (game_version, _) = instance_game_loader(&inst.dir);
            let mc = game_version.ok_or("could not determine Minecraft version")?;
            let legacy = args.iter().any(|a| a == "--legacy");
            let version = match args.get(2).filter(|a| !a.starts_with("--")) {
                Some(v) => v.clone(),
                None => crate::metaindex::resolve(data_root, "net.neoforged", &mc, true)?,
            };
            if legacy {
                let java = java::discover_java(data_root)
                    .into_iter()
                    .next()
                    .ok_or("no Java installation found")?;
                let url = crate::forge::neoforge_installer_url(&version);
                println!("running NeoForge installer {version}…");
                crate::forge::run_installer(&url, &java.path, &inst.dir)?;
            } else {
                crate::metaindex::ensure_component(data_root, "net.neoforged", &version)?;
            }
            meta::set_component(&inst.dir, "net.neoforged", &version)?;
            println!("installed NeoForge {version} into {name}");
            Ok(())
        }
        Some("groups") => {
            let store = crate::groups::GroupStore::load(data_root);
            if store.groups.is_empty() {
                println!("(no groups)");
            }
            for (name, g) in &store.groups {
                println!("{name} ({}): {}", g.instances.len(), g.instances.join(", "));
            }
            Ok(())
        }
        Some("group-set") => {
            let name = args.get(1).ok_or("usage: group-set <instance> <group|->")?;
            let group = args.get(2).ok_or("usage: group-set <instance> <group|->")?;
            let inst = find_instance(data_root, name)?;
            let id = inst
                .dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| name.clone());
            let mut store = crate::groups::GroupStore::load(data_root);
            let group = if group == "-" { None } else { Some(group.as_str()) };
            store.set_group(&id, group);
            store.save(data_root)?;
            println!("set group of {name} to {}", group.unwrap_or("(none)"));
            Ok(())
        }
        Some("settings") => {
            let sub = args.get(1).map(|s| s.as_str()).unwrap_or("list");
            let mut s = crate::settings::GlobalSettings::load(data_root);
            match sub {
                "list" => {
                    for section in &s.ini.order {
                        println!("[{section}]");
                        if let Some(entries) = s.ini.sections.get(section) {
                            for (k, v) in entries {
                                println!("{k}={v}");
                            }
                        }
                    }
                    Ok(())
                }
                "pages" => {
                    for p in crate::settings::pages() {
                        println!("{p}");
                    }
                    Ok(())
                }
                "defs" => {
                    for d in crate::settings::SETTINGS {
                        println!(
                            "{:<24} {:<16} default={:<8} {}",
                            d.key, d.page, d.default, d.label
                        );
                    }
                    Ok(())
                }
                "get" => {
                    let key = args.get(2).ok_or("usage: settings get <key>")?;
                    println!("{}", s.get(key).unwrap_or(""));
                    Ok(())
                }
                "set" => {
                    let key = args.get(2).ok_or("usage: settings set <key> <value>")?;
                    let value = args.get(3).ok_or("usage: settings set <key> <value>")?;
                    s.set(key, value);
                    s.save().map_err(|e| e.to_string())?;
                    println!("set {key}={value}");
                    Ok(())
                }
                other => Err(format!("unknown settings subcommand: {other}")),
            }
        }
        Some("install-url") => {
            let name = args.get(1).ok_or("usage: install-url <instance> <url>")?;
            let url = args.get(2).ok_or("usage: install-url <instance> <url>")?;
            let inst = find_instance(data_root, name)?;
            let dir = inst.dir.join("minecraft").join("mods");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let filename = url
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("download.jar")
                .split('?')
                .next()
                .unwrap_or("download.jar");
            let dest = dir.join(filename);
            crate::net::download_verified(url, &dest, None)?;
            println!("installed {} → {}", filename, dest.display());
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
                    "{:<28} {:<8} {:<10} {}{}",
                    w.name,
                    if w.has_level_dat { "valid" } else { "?" },
                    human_size(w.size_bytes),
                    w.display_name,
                    if w.last_played > 0 {
                        format!(" (last played {})", w.last_played)
                    } else {
                        String::new()
                    }
                );
            }
            Ok(())
        }
        Some("servers") => {
            let name = args.get(1).ok_or("usage: servers <instance>")?;
            let inst = find_instance(data_root, name)?;
            let list = crate::servers::list_servers(&inst.dir);
            if list.is_empty() {
                println!("(no servers)");
            }
            for s in list {
                println!("{:<24} {}", s.name, s.ip);
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
            if let Some(a) = crate::updater::select_asset(&r) {
                println!(
                    "asset for {}: {} ({} bytes)",
                    crate::updater::platform_keyword(),
                    a.name,
                    a.size
                );
            } else {
                println!("no installable asset found for this platform");
            }
            let mut settings = crate::settings::GlobalSettings::load(data_root);
            settings.set("LastUpdateCheck", &now_unix().to_string());
            let _ = settings.save();
            Ok(())
        }
        Some("self-update") => {
            let apply = args.iter().any(|a| a == "--apply");
            cmd_self_update(data_root, apply)
        }
        Some("plugins") => {
            let host = crate::plugin::PluginHost::discover(data_root);
            host.print();
            for e in host.init_natives(data_root) {
                eprintln!("warning: {e}");
            }
            Ok(())
        }
        Some("search-source") => {
            let name = args.get(1).ok_or("usage: search-source <source> <query>")?;
            let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
            let host = crate::plugin::PluginHost::discover(data_root);
            for e in host.init_natives(data_root) {
                eprintln!("warning: {e}");
            }
            let sources = host.sources();
            let Some((plugin, source)) = sources
                .iter()
                .find(|(_, s)| s.name.eq_ignore_ascii_case(name))
            else {
                let names: Vec<&str> = sources.iter().map(|(_, s)| s.name.as_str()).collect();
                return Err(if names.is_empty() {
                    "no plugin content sources are installed".to_string()
                } else {
                    format!("unknown source '{name}' (available: {})", names.join(", "))
                });
            };
            let hits = crate::plugin::search_source(source, query)?;
            println!(
                "{} ({plugin}) — {} results",
                source.name,
                hits.len()
            );
            for h in &hits {
                println!(
                    "  {:<24} {:<10} {}",
                    h.id,
                    if h.downloads > 0 {
                        h.downloads.to_string()
                    } else {
                        String::new()
                    },
                    h.title
                );
                if !h.description.is_empty() {
                    println!("      {}", h.description);
                }
            }
            Ok(())
        }
        Some("install-source") => {
            let name = args.get(1).ok_or("usage: install-source <source> <id> <instance>")?;
            let id = args.get(2).ok_or("usage: install-source <source> <id> <instance>")?;
            let inst_name = args
                .get(3)
                .ok_or("usage: install-source <source> <id> <instance>")?;
            let inst = find_instance(data_root, inst_name)?;
            let host = crate::plugin::PluginHost::discover(data_root);
            for e in host.init_natives(data_root) {
                eprintln!("warning: {e}");
            }
            let sources = host.sources();
            let Some((_, source)) = sources
                .iter()
                .find(|(_, s)| s.name.eq_ignore_ascii_case(name))
            else {
                return Err(format!("unknown plugin source: {name}"));
            };
            let hit = crate::plugin::SourceHit {
                id: id.clone(),
                title: id.clone(),
                description: String::new(),
                downloads: 0,
            };
            let dir = inst.dir.join("minecraft").join("mods");
            let path = crate::plugin::install_source(source, &hit, &dir)?;
            println!("installed {}", path.display());
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
        Some("help") | Some("--help") | Some("-h") | None => {
            print_help();
            Ok(())
        }
        Some("--version") | Some("-V") | Some("version") => {
            println!("prism-gpui {}", env!("CARGO_PKG_VERSION"));
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
    println!("  add-forge <instance> [version]  run the Forge installer for an instance");
    println!("  add-neoforge <instance> [version]  run the NeoForge installer");
    println!("  add-litelloader <instance> [version]  add LiteLoader (<=1.12.2)");
    println!("  search-type <type> <query>  search Modrinth by project type");
    println!("  check-mods <instance>  look up installed mods on Modrinth");
    println!("  install-pack <slug> [name]  install a Modrinth modpack");
    println!("  platforms         list content platforms and their status");
    println!("  search-platform <platform> <kind> <query> [mc] [loader]  search any platform");
    println!("  install-mod <platform> <id> <instance>  install a mod from any platform");
    println!("  install-pack-cf <project-id> [name]  install a CurseForge modpack");
    println!("  install-ftb <pack-id> [version] [name]  install an FTB modpack");
    println!("  install-technic <slug> [name]  install a Technic modpack");
    println!("  install-atl <safe-name> [version] [name]  install an ATLauncher modpack");
    println!("  install-packwiz <url|file> [name]  install a packwiz pack");
    println!("  login             sign in with a Microsoft account");
    println!("  add-offline <name>  add an offline account");
    println!("  accounts          list saved accounts");
    println!("  logout <uuid|name>  remove an account");
    println!("  set-component <instance> <uid> <version>  change a component version");
    println!("  settings [list|get <k>|set <k> <v>]  read/write global settings");
    println!("  install-url <instance> <url>  download a mod directly into mods/");
    println!("  groups            list instance groups");
    println!("  group-set <instance> <group|->  assign an instance to a group");
    println!("  open <instance>   open the instance folder");
    println!("  worlds <instance>  list worlds");
    println!("  servers <instance>  list multiplayer servers");
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
    // Indexed components (loaders) are fetched from Prism's meta index, which
    // also materialises their libraries and maven-only artifacts.
    let (resolved, failed) = crate::metaindex::ensure_instance_components(data_root, &pack);
    for c in &pack.components {
        if c.uid == "net.minecraft" {
            if let Err(e) = meta::ensure_minecraft(data_root, &c.version) {
                eprintln!("warning: net.minecraft {}: {e}", c.version);
            }
        }
        let path = meta_dir.join(&c.uid).join(format!("{}.json", c.version));
        let Some(v) = meta::read_version_json(&path) else {
            continue;
        };
        total += meta::ensure_libraries(data_root, &v)?;
        meta::ensure_asset_index(data_root, &v)?;
    }
    for c in &pack.components {
        if c.uid == "net.minecraft" {
            match meta::ensure_all_assets(data_root, &c.version) {
                Ok(n) => total += n,
                Err(e) => eprintln!("warning: assets for {}: {e}", c.version),
            }
        }
    }
    println!("updated {name}: {resolved} components resolved, {total} files downloaded");
    for f in &failed {
        eprintln!("warning: {f}");
    }
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
    let instances_root = instance::instances_dir(data_root);
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
        if let Some(f) = &m.dependencies.neo_forge {
            let _ = meta::set_component(&inst_dir, "net.neoforged", f);
        }
        if let Some(f) = &m.dependencies.quilt_loader {
            let _ = meta::set_component(&inst_dir, "org.quiltmc.quilt-loader", f);
        }
    }
    println!("installed pack {} → {}", slug, inst_dir.display());
    Ok(())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cmd_self_update(data_root: &Path, apply: bool) -> Result<(), String> {
    let release = crate::updater::latest_release()?;
    if !crate::updater::is_newer(env!("CARGO_PKG_VERSION"), &release.tag_name) {
        println!(
            "already up to date ({} >= {})",
            env!("CARGO_PKG_VERSION"),
            release.tag_name
        );
        return Ok(());
    }
    println!(
        "update available: {} → {}",
        env!("CARGO_PKG_VERSION"),
        release.tag_name
    );
    let asset =
        crate::updater::select_asset(&release).ok_or("no installable asset for this platform")?;
    println!("downloading {} ({} bytes)…", asset.name, asset.size);
    let path = crate::updater::download_asset(asset, data_root)?;
    println!("downloaded to {}", path.display());
    if !apply {
        println!("re-run with --apply to install it over the current binary");
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let written = crate::updater::apply_update(&path, &exe)?;
    println!(
        "updated {} — restart to use {}",
        written.display(),
        release.tag_name
    );
    Ok(())
}

fn find_instance(data_root: &Path, name: &str) -> Result<instance::Instance, String> {
    instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))
}

// ---- Unified platform commands ----

fn parse_kind(s: &str) -> Result<crate::modplatform::ContentKind, String> {
    use crate::modplatform::ContentKind;
    match s.trim().to_ascii_lowercase().as_str() {
        "mod" | "mods" => Ok(ContentKind::Mod),
        "modpack" | "modpacks" | "pack" => Ok(ContentKind::Modpack),
        "resourcepack" | "resourcepacks" | "rp" => Ok(ContentKind::ResourcePack),
        "shader" | "shaders" | "shaderpack" => Ok(ContentKind::Shader),
        other => Err(format!("unknown content kind: {other}")),
    }
}

fn parse_platform(s: &str) -> Result<crate::modplatform::Platform, String> {
    crate::modplatform::Platform::from_name(s)
        .ok_or_else(|| format!("unknown platform: {s} (try mr, cf, ftb, technic, atl)"))
}

/// The Minecraft version and loader of an instance, derived from its
/// component pack.
fn instance_target(inst_dir: &Path) -> (Option<String>, Option<String>) {
    let mut mc = None;
    let mut loader = None;
    if let Some(pack) = meta::read_component_pack(inst_dir) {
        for c in &pack.components {
            if c.uid == "net.minecraft" {
                mc = Some(c.version.clone());
            } else if !c.dependency_only {
                let name = match c.uid.as_str() {
                    "net.fabricmc.fabric-loader" => "fabric",
                    "net.minecraftforge" => "forge",
                    "net.neoforged" => "neoforge",
                    "org.quiltmc.quilt-loader" => "quilt",
                    _ => continue,
                };
                loader = Some(name.to_string());
            }
        }
    }
    (mc, loader)
}

fn cmd_search_platform(
    platform: &str,
    kind: &str,
    query: &str,
    mc: Option<&str>,
    loader: Option<&str>,
) -> Result<(), String> {
    let platform = parse_platform(platform)?;
    let kind = parse_kind(kind)?;
    let q = crate::modplatform::SearchQuery::new(query)
        .with_game_version(mc.map(|s| s.to_string()))
        .with_loader(loader.map(|s| s.to_string()));
    let hits = crate::modplatform::platform::search(platform, kind, &q)?;
    if hits.is_empty() {
        println!("(no results)");
    }
    for h in hits {
        println!(
            "{:<28} {:<12} {:<10} {}",
            h.title,
            h.id,
            format_count(h.downloads),
            truncate(&h.description, 60)
        );
    }
    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    let cleaned = s.replace('\n', " ");
    if cleaned.chars().count() <= n {
        cleaned
    } else {
        let mut out: String = cleaned.chars().take(n.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Compact download/install counts (1.2k, 3.4M).
fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Create an instance directory with `instance.cfg` and the given components.
fn create_instance_with_components(
    data_root: &Path,
    name: &str,
    components: &[(String, String)],
) -> Result<std::path::PathBuf, String> {
    let settings = crate::settings::GlobalSettings::load(data_root);
    let dir = instance::instances_dir(data_root).join(name);
    if dir.exists() {
        return Err(format!("instance already exists: {name}"));
    }
    std::fs::create_dir_all(dir.join("minecraft")).map_err(|e| e.to_string())?;
    let cfg = format!(
        "[General]\nInstanceType=OneSix\nConfigVersion=1.3\nname={name}\nMinMemAlloc={}\nMaxMemAlloc={}\n",
        settings.min_mem(),
        settings.max_mem()
    );
    std::fs::write(dir.join("instance.cfg"), cfg).map_err(|e| e.to_string())?;
    let mut comps: Vec<(String, String)> = components.to_vec();
    if !comps.iter().any(|(u, _)| u == "net.minecraft") {
        comps.insert(0, ("net.minecraft".into(), "1.20.1".into()));
    }
    for (uid, version) in &comps {
        let _ = meta::set_component(&dir, uid, version);
        if uid == "net.minecraft" {
            // Prism instances need the Mojang version JSON, libraries and
            // asset index before they can launch.
            if let Err(e) = meta::ensure_minecraft(data_root, version) {
                eprintln!("warning: could not fetch Minecraft {version} metadata: {e}");
            }
        }
    }
    Ok(dir)
}

fn sanitize_name(name: &str) -> String {
    name.replace(['/', '\\', ':'], "_").trim().to_string()
}

fn cmd_install_mod(
    data_root: &Path,
    platform: &str,
    id: &str,
    instance_name: &str,
) -> Result<(), String> {
    let platform = parse_platform(platform)?;
    let inst = find_instance(data_root, instance_name)?;
    let (mc, loader) = instance_target(&inst.dir);
    let (subdir, kind) = ("mods", crate::modplatform::ContentKind::Mod);
    let hit = crate::modplatform::PlatformHit {
        platform,
        kind,
        id: id.to_string(),
        slug: id.to_string(),
        title: id.to_string(),
        description: String::new(),
        downloads: 0,
        extra: String::new(),
    };
    let dest = inst.dir.join("minecraft").join(subdir);
    let outcome = crate::modplatform::platform::install(
        &hit,
        &dest,
        mc.as_deref(),
        loader.as_deref(),
    )?;
    println!("{}", outcome.message);
    Ok(())
}

fn cmd_install_ftb(
    data_root: &Path,
    id: &str,
    version: Option<&str>,
    name: Option<&str>,
) -> Result<(), String> {
    let pack_id: u64 = id.parse().map_err(|_| format!("invalid FTB pack id: {id}"))?;
    let version_id = match version {
        Some(v) if !v.is_empty() => Some(v.parse::<u64>().map_err(|_| format!("invalid version: {v}"))?),
        _ => None,
    };
    let info = crate::modplatform::ftb::Ftb
        .pack_info(pack_id)
        .map_err(|e| format!("cannot fetch pack {id}: {e}"))?;
    let components = crate::modplatform::ftb::Ftb
        .manifest(pack_id, version_id)
        .map(|m| m.components())
        .unwrap_or_default();
    let dir_name = sanitize_name(name.unwrap_or(&info.name));
    let inst_dir = create_instance_with_components(data_root, &dir_name, &components)?;
    let game_dir = inst_dir.join("minecraft");
    let manifest = crate::modplatform::ftb::Ftb.install(pack_id, version_id, &game_dir)?;
    println!(
        "installed FTB pack {} v{} ({} files) → {}",
        manifest.name,
        manifest.version,
        manifest.files.len(),
        inst_dir.display()
    );
    Ok(())
}

fn cmd_install_technic(data_root: &Path, slug: &str, name: Option<&str>) -> Result<(), String> {
    let pack = crate::modplatform::technic::Technic
        .pack(slug)
        .map_err(|e| format!("cannot fetch Technic pack {slug}: {e}"))?;
    let mc = pack.minecraft.clone();
    let components = vec![("net.minecraft".to_string(), mc.clone())];
    let dir_name = sanitize_name(name.unwrap_or(&pack.title()));
    let inst_dir = create_instance_with_components(data_root, &dir_name, &components)?;
    let game_dir = inst_dir.join("minecraft");
    let (summary, components) = crate::modplatform::technic::Technic.install(slug, &game_dir)?;
    for (uid, version) in &components {
        if let Err(e) = meta::set_component(&inst_dir, uid, version) {
            eprintln!("warning: could not set component {uid}: {e}");
        }
    }
    println!("{summary} → {}", inst_dir.display());
    Ok(())
}

fn cmd_install_atl(
    data_root: &Path,
    safe_name: &str,
    version: Option<&str>,
    name: Option<&str>,
) -> Result<(), String> {
    let atl = crate::modplatform::atlauncher::AtLauncher;
    let packs = atl.packs()?;
    let pack = packs
        .iter()
        .find(|p| p.safe_name().eq_ignore_ascii_case(safe_name))
        .ok_or_else(|| format!("ATLauncher pack not found: {safe_name}"))?;
    let version = match version {
        Some(v) if !v.is_empty() => v.to_string(),
        _ => pack
            .latest_version()
            .map(|v| v.version.clone())
            .ok_or("pack has no versions")?,
    };
    let mc = pack
        .latest_version()
        .map(|v| v.minecraft.clone())
        .unwrap_or_default();
    let components = vec![("net.minecraft".to_string(), mc)];
    let dir_name = sanitize_name(name.unwrap_or(&pack.name));
    let inst_dir = create_instance_with_components(data_root, &dir_name, &components)?;
    let game_dir = inst_dir.join("minecraft");
    let (n, blocked, failed) = atl.install(&pack.safe_name(), &version, &game_dir)?;
    println!(
        "installed ATLauncher pack {} v{version} ({n} files) → {}",
        pack.name,
        inst_dir.display()
    );
    if !blocked.is_empty() {
        println!("mods needing manual download ({}):", blocked.len());
        for b in blocked {
            println!("  - {b}");
        }
    }
    if !failed.is_empty() {
        println!("downloads that failed ({}):", failed.len());
        for f in failed {
            println!("  - {f}");
        }
    }
    Ok(())
}

fn cmd_install_packwiz(
    data_root: &Path,
    url: &str,
    name: Option<&str>,
) -> Result<(), String> {
    let local = Path::new(url);
    let (zip_path, cleanup) = if local.is_file() {
        (local.to_path_buf(), false)
    } else {
        let tmp = std::env::temp_dir().join(format!("prism-packwiz-{}.zip", std::process::id()));
        crate::net::download_verified(url, &tmp, None)?;
        (tmp, true)
    };
    // Inspect pack.toml first so components can be applied.
    let probe = std::env::temp_dir().join(format!("prism-pw-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&probe);
    std::fs::create_dir_all(&probe).map_err(|e| e.to_string())?;
    crate::fs::extract_zip(&zip_path, &probe, true)?;
    let pack_dir = if probe.join("pack.toml").is_file() {
        probe.clone()
    } else {
        crate::fs::list_files(&probe)
            .into_iter()
            .find(|p| p.file_name().map(|n| n == "pack.toml").unwrap_or(false))
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or("no pack.toml found in archive")?
    };
    let pack = crate::modplatform::packwiz::read_pack(&pack_dir)?;
    let components = pack.components();
    let dir_name = sanitize_name(name.unwrap_or(&pack.name));
    let inst_dir = create_instance_with_components(data_root, &dir_name, &components)?;
    let game_dir = inst_dir.join("minecraft");
    let (installed, skipped) =
        crate::modplatform::packwiz::install_from_dir(&pack_dir, &game_dir, "client")?;
    let _ = std::fs::remove_dir_all(&probe);
    if cleanup {
        let _ = std::fs::remove_file(&zip_path);
    }
    println!(
        "installed packwiz pack {} ({installed} files, {} skipped) → {}",
        pack.name,
        skipped.len(),
        inst_dir.display()
    );
    for s in skipped.iter().take(20) {
        println!("  skipped: {s}");
    }
    Ok(())
}

fn cmd_install_pack_cf(data_root: &Path, id: &str, name: Option<&str>) -> Result<(), String> {
    let cf = crate::modplatform::curseforge::CurseForge;
    let file = cf
        .latest_any(id)?
        .ok_or_else(|| format!("no files for CurseForge project {id}"))?;
    let url = file
        .resolved_url()
        .ok_or("pack file has no download URL")?;
    let tmp_dir = std::env::temp_dir().join(format!("prism-cfpack-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let pack_path = tmp_dir.join(if file.file_name.is_empty() {
        "pack.zip".to_string()
    } else {
        file.file_name.clone()
    });
    crate::net::download_verified(&url, &pack_path, None)?;
    let manifest = crate::pack::curseforge::manifest_from_pack(&pack_path)?;
    let components: Vec<(String, String)> = {
        let d = &manifest.dependencies;
        let mut v = Vec::new();
        if let Some(mc) = &d.minecraft {
            v.push(("net.minecraft".to_string(), mc.clone()));
        }
        if let Some(f) = &d.fabric_loader {
            v.push(("net.fabricmc.fabric-loader".to_string(), f.clone()));
        }
        if let Some(f) = &d.forge {
            v.push(("net.minecraftforge".to_string(), f.clone()));
        }
        if let Some(f) = &d.neo_forge {
            v.push(("net.neoforged".to_string(), f.clone()));
        }
        if let Some(f) = &d.quilt_loader {
            v.push(("org.quiltmc.quilt-loader".to_string(), f.clone()));
        }
        v
    };
    let dir_name = sanitize_name(name.unwrap_or(&manifest.name));
    let inst_dir = create_instance_with_components(data_root, &dir_name, &components)?;
    let installed = crate::pack::curseforge::install(&pack_path, &inst_dir)?;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    println!(
        "installed CurseForge pack {} v{} ({} mods) → {}",
        installed.name,
        installed.version,
        installed.files.len(),
        inst_dir.display()
    );
    Ok(())
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
    // `import_archive` detects mrpack / CurseForge / Prism zips from their
    // contents, materialises components and picks a non-colliding name.
    let result = crate::pack::import_archive(path, data_root, name)?;
    println!("{} → {}", result.summary, result.dir.display());
    for (uid, version) in &result.components {
        println!("  component {uid} {version}");
    }
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

/// Print the launch command and component summary for an instance.
fn cmd_profile(data_root: &Path, name: &str) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let java_path = java::ensure_for_instance(data_root, &inst.dir)?;
    let account = crate::accounts::AccountStore::load(data_root)
        .active_account()
        .cloned();
    let profile =
        launch::build_profile_with_account(&inst.dir, data_root, &java_path, account.as_ref())
            .ok_or("could not build launch profile")?;
    if let Some(pack) = meta::read_component_pack(&inst.dir) {
        println!("components:");
        for c in &pack.components {
            println!(
                "  {:<32} {:<16} {}",
                c.uid,
                c.version,
                crate::metaindex::component_label(&c.uid)
            );
        }
    }
    println!("java:      {}", profile.java_path.display());
    println!("main:      {}", profile.main_class);
    println!("game dir:  {}", profile.game_dir.display());
    println!("memory:    {}M–{}M", profile.min_mem_mb, profile.max_mem_mb);
    println!("classpath: {} jars", profile.classpath.len());
    if !profile.wrapper.is_empty() {
        println!("wrapper:   {}", profile.wrapper.join(" "));
    }
    if !profile.pre_launch.is_empty() {
        println!("pre-launch: {}", profile.pre_launch);
    }
    println!("jvm args:  {}", profile.jvm_args.join(" "));
    println!("game args: {}", profile.game_args.join(" "));
    Ok(())
}

fn cmd_launch(data_root: &Path, name: &str, dry_run: bool) -> Result<(), String> {
    let inst = instance::discover_instances(data_root)
        .into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| format!("instance not found: {name}"))?;
    let java_path = java::ensure_for_instance(data_root, &inst.dir)?;
    let account = crate::accounts::AccountStore::load(data_root)
        .active_account()
        .cloned();
    let profile =
        launch::build_profile_with_account(&inst.dir, data_root, &java_path, account.as_ref())
            .ok_or("could not build launch profile")?;
    println!(
        "launching {} with {} as {} ({} jars, main {})",
        inst.name,
        java_path.display(),
        account.as_ref().map(|a| a.username.as_str()).unwrap_or("offline"),
        profile.classpath.len(),
        profile.main_class
    );
    if dry_run {
        let cmd = profile.jvm_command();
        println!("command: {:?}", cmd);
        return Ok(());
    }
    // Download the client jar and any missing assets before starting the JVM.
    match profile.ensure_ready() {
        Ok(0) => {}
        Ok(n) => println!("fetched {n} missing files"),
        Err(e) => eprintln!("warning: {e}"),
    }
    if !profile.pre_launch.trim().is_empty() {
        println!("running pre-launch: {}", profile.pre_launch);
        launch::run_hook(&profile.pre_launch, &profile.game_dir)?;
    }
    let status = profile
        .jvm_command()
        .status()
        .map_err(|e| format!("spawn failed: {e}"))?;
    println!("exit: {status}");
    if !profile.post_exit.trim().is_empty() {
        println!("running post-exit: {}", profile.post_exit);
        launch::run_hook(&profile.post_exit, &profile.game_dir)?;
    }
    Ok(())
}
