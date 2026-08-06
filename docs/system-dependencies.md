# System Dependencies (Linux)

The desktop GUI crate (Tauri) is part of the default cargo workspace, so a
plain `cargo build` or `cargo test` on Linux needs these system libraries
installed first – without them the build fails with missing `pkg-config`
headers for GTK/WebKit.

macOS and Windows need no extra system packages beyond Rust itself.

### Fedora / RHEL

```bash
sudo dnf install pkgconf-pkg-config glib2-devel gtk3-devel javascriptcoregtk4.1-devel libsoup3-devel webkit2gtk4.1-devel
```

### Debian / Ubuntu

```bash
sudo apt-get update
sudo apt install pkg-config libglib2.0-dev libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev libwebkit2gtk-4.1-dev
```