# prism-gpui

Prism Launcher rewritten in Rust + GPUI.

## Build

```sh
cargo build
cargo run           # GUI
cargo run -- list   # headless CLI
```

## CLI

```
prism-gpui list                     list instances
prism-gpui java                     list Java installations
prism-gpui update <instance>        download missing libraries/assets
prism-gpui assets <instance> [max]  download missing asset objects
prism-gpui search <query>           search Modrinth
prism-gpui install <instance> <slug>  install a Modrinth mod
prism-gpui launch <instance>        launch (foreground)
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

## Releases

Two GitHub Actions workflows build the launcher:

- **CI** runs on every push to `main`: it formats the code, lints it with
  clippy (`-D warnings`), runs the tests and builds the binary on Linux,
  Windows and macOS.
- **Release** runs when a `v*` tag is pushed: it builds the three binaries,
  packages them next to the readme and publishes them as release assets.

```sh
git tag -a v0.1.2 -m "Prism Launcher (Rust) 0.1.2"
git push origin v0.1.2
```

The assets are named after the platform (`prism-gpui-Linux-x86_64.tar.gz`,
`prism-gpui-Windows-x86_64.zip`, `prism-gpui-macOS-aarch64.tar.gz`), which is
what the built in updater looks for: `prism-gpui self-update --apply` downloads
the release, checks its digest and replaces the running binary.

> The token that pushes this repository does not have the `workflow` scope, so
> the files under `.github/workflows` have to be pushed over SSH
> (`git push ssh-origin main`). Everything else pushes normally.

## Data directory

Defaults to `~/.local/share/PrismLauncher`. Override with `PRISM_DATA_DIR`.
