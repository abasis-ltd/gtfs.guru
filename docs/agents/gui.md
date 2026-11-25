# Desktop GUI (gtfs_validator_gui)

## Scope

- Tauri-based desktop app wrapping the Rust validator.
- UI and validation run locally; validation happens off the UI thread.

## Prerequisites

- Rust (stable)
- Tauri CLI (`cargo install tauri-cli`)
- On Linux, GTK/WebKit system packages – see `docs/system-dependencies.md`
- Node.js only if frontend tooling is needed

## Development and Build

```bash
cargo tauri dev
```

```bash
cargo tauri build
```

Release artifacts are written under `target/release/bundle/`.

## Windows release signing

Connect SimplySign Desktop on the signing PC and check that the Abasis Limited
code signing certificate is available in `Cert:\CurrentUser\My`. Install the
Windows SDK and set `TAURI_WINDOWS_SIGNTOOL_PATH` to the full path to
`signtool.exe` if the Tauri CLI cannot find it.

Run `powershell -NoProfile -File scripts/prepare-windows-release.ps1` from a
clean checkout of the release source. Upload the resulting
`dist/gtfs-guru-windows-signed-build.zip` to a draft release for the version
before pushing the tag. CI checks the source tree, installer hashes, publisher
signatures and timestamps. It signs the final installer bytes with the Tauri
updater key and publishes the release after staging all assets.

The certificate thumbprint is configured in
`crates/gtfs_validator_gui/tauri.windows-signing.conf.json`. Windows
installations use their installer type marker to select the MSI or NSIS entry
in `latest.json`.

## Running inside a Wayland session

On a Linux host with Wayland, WebKitGTK's hardware-accelerated
compositing may fail on startup, and the app exits immediately with:

```text
Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
```

Work around it by disabling the DMA-BUF renderer, which falls back to
software compositing:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 ./target/debug/gtfs-guru-desktop
```

This also applies to `cargo tauri dev` (set the variable before the command).
