# Prism Launcher — Migration Plan: C++/Qt → Rust + GPUI

This document scopes a full rewrite of Prism Launcher (currently ~390 `.cpp` /
~425 `.h` files, Qt6) into Rust with the [GPUI](https://gpui.rs) UI framework
(Zed's GPU-accelerated UI). A complete port is multi-month; this plan breaks it
into incremental, shippable phases and maps C++ subsystems to Rust crates.

A working toolchain scaffold lives in the sibling project
`/home/coobyk/Projects/prism-gpui` (builds and runs with `cargo run`).

## 0. Why Rust + GPUI

- Rust: memory safety for a launcher that parses untrusted modpacks/archives,
  handles credentials, and shells out to Java processes.
- GPUI: GPU-accelerated, retained-mode-ish element tree (`div()`, `Render`),
  async-friendly via `smol`-style executor, good fit for a snappy desktop app.
- Replaces Qt6, libarchive, toml++, cmark, murmur2, qdcss, etc. with pure-Rust
  crates, simplifying packaging and cross-compilation.

## 1. Architecture mapping

| C++ area | Rust crate / module | Notes |
|---|---|---|
| `launcher/Application`, `main.cpp` | `prism::app` | `Application::new().run(...)` in GPUI |
| `launcher/net/*` (HttpMetaCache, NetJob) | `reqwest` + `tokio`/`async-std` cache | Replace Qt network stack |
| `launcher/meta/*` (Version, VersionList, Index) | `prism::meta` | serde_json; keep Mojang/Prism format |
| `launcher/minecraft/*` | `prism::minecraft` | Instance model, launch, assets |
| `launcher/modplatform/*` | `prism::modplatform` | Modrinth, CurseForge, FTB, ATLauncher, Technic, Packwiz |
| `launcher/settings/*` | `prism::settings` | toml/ini via `toml` + `serde` |
| `launcher/java/*` | `prism::java` | `which`/process detection, javacheck port |
| `launcher/tasks/*`, `InstanceTask` | `prism::tasks` | async tasks, progress channels |
| `launcher/archive/*`, `MMCZip` | `zip`, `tar`, `flate2`, `sevenz-rust` | drop libarchive |
| `launcher/FileSystem`, `GZip` | `std::fs`, `flate2` | |
| `libraries/libnbtplusplus` | `fastnbt` or `nbt` crate | |
| `libraries/murmur2` | `murmur2` crate | |
| `libraries/qdcss` | `cssparser` | |
| `libraries/rainbow` | GPUI color helpers | |
| `launcher/ui/*` | `prism::ui` GPUI views | |
| `launcher/ui/instanceview` | `prism::ui::instance_list` | virtualized list via `uniform_list` |
| `launcher/ui/pages/*` | `prism::ui::pages` | settings/instance pages |
| `launcher/ui/dialogs/*` | `prism::ui::dialogs` | GPUI modal overlays |
| `launcher/ui/themes` | `prism::ui::theme` | GPUI `Theme`/`Global` |
| `launcher/ui/widgets` | `prism::ui::widgets` | reusable components |
| i18n (`translations/`) | `fluent` + `unic-langid` | replaces Qt tr() |
| `console/` | `tracing` + `tracing-subscriber` | |

## 2. Workspace / crate layout (proposed)

```
prism-gpui/
├── Cargo.toml            # workspace
├── crates/
│   ├── prism-core/       # domain: instances, versions, settings, java
│   ├── prism-net/        # HTTP, meta cache, modplatform APIs
│   ├── prism-minecraft/  # launch, assets, components
│   ├── prism-modplatform/# per-platform providers
│   ├── prism-ui/         # GPUI views, widgets, theme
│   └── prism-app/        # binary entrypoint
└── resources/            # icons, translations, css
```

## 3. Phases

### Phase 0 — Toolchain & CI (1-2 weeks)
- Rust workspace, `rustfmt`, `clippy`, GitHub Actions matrix (Linux/macOS/Windows).
- GPUI window shell + theming (done in scaffold).
- Logging (`tracing`), crash reporting, single-instance lock (port `LocalPeer`).

### Phase 1 — Core domain, no UI (3-5 weeks)
- `BaseInstance`/`MinecraftInstance` data model + instance list persistence
  (port `InstanceList`, `BaseInstance`, `MinecraftInstance`).
- Settings system (`SettingsObject`, INI) → typed config structs with serde.
- Java detection (`JavaInstallList`, `JavaVersion`).
- File system helpers, hashing (sha1/md5/murmur2), archive extraction.
- Unit tests mirroring `tests/`.

### Phase 2 — Networking & metadata (3-5 weeks)
- `reqwest`-based `NetJob` equivalent with progress + cancellation.
- Meta cache (HttpMetaCache) with ETag/disk cache.
- Mojang version manifest + Prism meta index parsing.
- Auth (Microsoft/Xbox/Minecraft) — port `minecraft/auth`.

### Phase 3 — Launch path (2-4 weeks)
- `LaunchController`: component updates, asset download, classpath, JVM args.
- Process spawning + log capture (port `LoggedProcess`).
- Console window in GPUI.

### Phase 4 — UI shell (4-6 weeks)
- MainWindow: toolbar, instance grid/list, search/filter, context menus.
- InstanceWindow: tabs (Mods, Resource packs, Worlds, Screenshots, Notes, Settings).
- Dialogs: new instance, import, copy, settings, java setup wizard.
- Theming (light/dark, custom CSS → GPUI style).

### Phase 5 — Mod platforms (4-8 weeks)
- Modrinth, CurseForge, FTB, ATLauncher, Technic, Packwiz providers.
- Mod browser UI with search/filter/install, dependency resolution.

### Phase 6 — Polish & parity (ongoing)
- Translations (fluent), accessibility, keyboard nav, drag-drop.
- Auto-updater, news feed, screenshots viewer, skins viewer.
- Performance: virtualized instance list, async image loading.

## 4. Key risks

- **GPUI API churn**: pin a GPUI version; track Zed releases. GPUI is
  pre-1.0 and breaking changes are frequent.
- **Qt→GPUI widget gap**: no built-in table/tree; build with `uniform_list`
  and custom elements. High effort for complex dialogs.
- **Platform specifics**: macOS sandbox, Windows code signing, Linux .desktop.
- **Auth flows**: Microsoft OAuth device code + browser; needs `oauth2` crate.
- **Archive formats**: CurseForge/FTB use various; keep `sevenz`/`zip`.
- **Migration of user data**: instance dirs and `prismlauncher.cfg` must remain
  compatible. Parse existing INI in Rust (keep format).

## 5. Suggested first milestone (MVP)

1. Launch app window (done in scaffold).
2. Read existing Prism instance list from disk and display names/icons.
3. Open an instance folder / launch Minecraft with a hardcoded version.

This validates the data layer + launch path before tackling UI breadth.

## 6. Non-goals for v1

- Full mod-platform parity (Modrinth first).
- Plugin system (Qt-based) — drop or reimplement later.
- In-app skin editor.

## 7. Effort estimate

Roughly 6-12 engineer-months for MVP parity with core launcher; 18-24 for full
parity including all mod platforms. Recommend incremental coexistence: keep C++
Prism Launcher shipping while Rust port reaches parity, sharing the same
instance directory format.

## 8. Implementation status

The `prism-gpui` binary implements the following today (all covered by unit
tests; `cargo test` green):

### Data / core
- `instance.rs` — instance discovery, `instance.cfg` read/write, create /
  remove / rename / duplicate.
- `settings.rs` — global `prismlauncher.cfg` read/write (`GlobalSettings`).
- `meta.rs` — Prism component pack (`mmc-pack.json`), Mojang version JSON,
  component index listing, library/asset download, `set_component`.
- `java.rs` — Java detection + automatic Mojang runtime download.
- `launch.rs` — launch profile builder (classpath, JVM/game args, substitutions)
  with account credentials injected.
- `net.rs` — sha1-verified downloads, meta cache helpers.
- `fs.rs` — recursive copy, zip-slip-safe joins, file listing.
- `worlds.rs` — world (save) list / delete / rename, with real names read from
  `level.dat`.
- `nbt.rs` — minimal big-endian NBT reader (gzip/zlib/raw) for
  `level.dat` and `servers.dat`.
- `servers.rs` — multiplayer server list parsed from `servers.dat`.
- `groups.rs` — instance groups (`instgroups.json`) with a main-window sidebar.
- `forge.rs` — Forge/NeoForge version resolution + installer runner.
- `skin.rs` — player skin renders (Crafatar).
- `desktop.rs` — open files/folders/URLs via the OS.
- `accounts.rs` + `auth/mod.rs` — Microsoft device-code → Xbox → XSTS →
  Minecraft chain; account store; vanilla-compatible offline UUIDs.
- `updater.rs` — GitHub release check with semver comparison.
- `news.rs` — Prism news feed (Atom/RSS parser).
- `i18n.rs` — JSON translation catalog with key fallback.

### Mod platforms
- `modplatform/modrinth.rs` — search (typed facets), versions, version lookup by
  file hash, downloads.
- `modplatform/curseforge.rs`, `ftb.rs`, `technic.rs`, `atlauncher.rs`,
  `packwiz.rs` — provider clients.

### Modpacks
- `pack/mrpack.rs` — Modrinth pack import/export (+ dependency extraction).
- `pack/curseforge.rs` — CurseForge manifest parse + override extraction +
  export.
- `pack/mmc.rs` — Prism/MultiMC instance zip import/export.

### UI (GPUI)
- Main window: instance list with icons, group sidebar/filter, launch, new
  instance, settings, news, skin.
- Instance window: tabs (Mods, Resource Packs, Shaders, Worlds, Screenshots,
  Logs), launch, browse content, open folder, per-tab file actions
  (enable/disable mods, open screenshots/logs).
- Content browser: Modrinth search across mods / modpacks / resource packs /
  shaders with install (modpacks become new instances).
- Console window streaming live launch output.
- Settings, New Instance, Instance Settings, News, Screenshots, Servers, and
  Skin windows.
- `theme.rs` — Catppuccin-based light/dark palettes; `widgets/text_input.rs` —
  reusable GPUI text field.

### CLI (headless)
`list java java-download search search-type check-mods install install-pack
install-url import export packinfo create rm rename duplicate components
versions set-component add-fabric add-forge add-neoforge launch log news
check-update open worlds world-rm servers groups group-set settings login
add-offline accounts logout --version`.

### Still open
- Forge/NeoForge/Quilt in-process installers (WiX-style).
- Skin viewer / icon rendering (needs a GPUI asset source).
- Full settings page parity, accessibility, virtualized lists.
- CurseForge downloads need `CF_API_KEY`; FTB/Technic/ATLauncher clients are
  scaffolded but not wired into a UI.
- Auto-update application (check only today).
