# prism-gpui

Prism Launcher rewritten in Rust + GPUI.

## Build

```sh
cargo build
cargo run
```

System deps (Linux): `libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev libfreetype-dev`.

## Layout

- `src/instance.rs` — instance discovery (`instances/<name>/instance.cfg`)
- `src/settings.rs` — global `prismlauncher.cfg`
- `src/java.rs` — Java detection
- `src/launch.rs` — launch profile / JVM command
- `src/modplatform/` — mod providers (Modrinth first)
- `src/auth/` — Microsoft auth (skeleton)
- `MIGRATION.md` — full port plan

## Data directory

Defaults to `~/.local/share/PrismLauncher`. Override with `PRISM_DATA_DIR`.
