# kitbash-cargo-linux

An unofficial loader that runs **Cargo by KitBash** natively on Linux.

Cargo is an Electron application. This tool takes the official Windows setup, pulls the
platform independent application out of it and runs it on the official Linux build of the
same Electron version. No Wine is involved.

## What it does

1. Downloads the latest Windows setup from the official ToDesktop update server and verifies its
   SHA-512 checksum, or uses a setup file you provide.
2. Extracts the application archive embedded in the NSIS installer (pure Rust, no `7z` needed).
3. Downloads the matching Electron release for Linux x64 and verifies its SHA-256 checksum.
4. Patches the application for Linux:
   - reports `linux` as a supported platform
   - replaces the ToDesktop auto updater (updates are done through this tool)
   - handles `cargo://` deep links (login callback)
   - Blender support: detects installs from `PATH`, `/usr/bin`, `/opt`, tarballs in your home
     directory, Steam and Flatpak, and installs the add-on into `~/.config/blender/<version>`
   - other DCC lookups (Cinema 4D, Houdini, Maya, Unreal) are disabled on Linux
5. Replaces the Windows `better-sqlite3` module with the official Linux prebuild.
6. Installs a desktop entry, icons and the `cargo://` URL handler. The Wayland `app_id` is
   `kitbash-cargo`, which matches the desktop file, so the icon shows up in the task bar.

## Usage

```sh
cargo build --release
./target/release/kitbash-cargo-linux install
```

| Command | Description |
| --- | --- |
| `install` | Download the latest setup and install Cargo |
| `install --setup <file.exe>` | Install from a local Windows setup |
| `install --blender-versions 5.3` | Also allow Blender versions Cargo does not list yet |
| `update` | Install the latest release if a newer one is available |
| `launch [args]` | Start Cargo (used by the desktop entry) |
| `status` | Show installed and latest version |
| `uninstall` | Remove Cargo and the desktop integration, keeping your data |

During installation the loader copies itself to `~/.local/share/kitbash-cargo/`, so the build
directory can be deleted afterwards.

## Files

| Path | Content |
| --- | --- |
| `~/.local/share/kitbash-cargo/` | Application and loader |
| `~/.cache/kitbash-cargo-linux/` | Downloaded setup, Electron and native modules |
| `~/.config/kitbash-cargo-flags.conf` | Optional extra Chromium flags, one per line |
| `~/.kitbash3d/` | Cargo data: database, downloaded kits |
| `~/.config/Cargo by KitBash/` | Electron profile: session, cache |

## Graphics

On Wayland the launcher starts Cargo with `--ozone-platform-hint=auto` and
`--use-angle=vulkan`. With the default OpenGL backend, Chromium 134 crashes its GPU process on
some Mesa setups (`eglCreateImage failed`) and falls back to software rendering. If you
have problems, add flags to `~/.config/kitbash-cargo-flags.conf`. Later flags win:

```
--use-angle=gl
--ozone-platform=x11
```

If the Chromium sandbox cannot start on your system, set `KITBASH_CARGO_NO_SANDBOX=1`.

## Notes

- This is not affiliated with or supported by KitBash3D.
- Cargo sends error reports and analytics (Sentry, PostHog) exactly like on Windows and macOS.
- Patches are matched against the exact source of the application. If a new Cargo release
  changes the patched code, required patches stop the installation with an error and optional
  ones are reported as skipped.
