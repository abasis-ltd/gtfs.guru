# Maintenance Guide: How to Safely Update GTFS Guru

This guide describes the workflow for making changes to the repository without breaking existing functionality.

## The Golden Rule

**Never push directly to `main`.** strict adherence to this rule ensures that the `main` branch is always stable and deployable.

---

## The Workflow

### 1. Create a Topic Branch

For every new feature or fix, start a new branch.

```bash
git checkout main
git pull                     # Get latest changes
git checkout -b my-new-feature # Create your branch
```

### 2. Make Your Changes

Edit files, write code.

### 3. Verify Locally (The "Safety Net")

Before you commit, run the checks locally.

```bash
# 1. Check for basic errors
cargo check

# 2. Run the test suite (CRITICAL)
cargo test --all

# 3. Check code style (Optional but recommended)
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

If `cargo test` fails, **do not commit**. Fix the errors first.

### 4. Commit and Push

```bash
git add .
git commit -m "feat: description of my awesome change"
git push -u origin my-new-feature
```

### 5. Create a Pull Request (PR)

1. Go to GitHub.
2. Click "Compare & pull request".
3. Create the PR.

**Wait for the "Checks" section.**
GitHub Actions will automatically run:

* ✅ Rust Tests (`cargo test`)
* ✅ Code Formatting
* ✅ Clippy Lints

**If the checks turn red ❌:**
Click "Details" to see what failed. Fix it locally, commit, and push again. The PR will update automatically.

**If the checks turn green ✅:**
You are safe! Click **"Squash and merge"**.

---

## Deploying the Website (gtfs.guru)

The live site **does not run on GitHub Pages**, and **merging to `main` does NOT update it.** The `Website` workflow only checks the static assets; the site is published by the `deploy-website` job in `release.yml`, which runs on a `v*` tag.

**Actual hosting:** a VPS at Hetzner Cloud (`157.90.246.102`), where Caddy terminates TLS and proxies to nginx serving static files.

**Deploy path:**

```bash
# 1. Rebuild the WASM validator (needs wasm-pack + binaryen)
./scripts/build-wasm.sh

# 2. Copy the fresh pkg/ into BOTH website copies
#    - website/pkg/                          (what nginx actually serves)
#    - crates/gtfs_validator_web/website/    (embedded in the axum Docker binary)

# 3. Push static files to the server (needs SSH access)
./scripts/deploy-website.sh <server-ip-or-hostname>
```

Notes:

* There are **two copies** of the website in the repo. The repo-root `website/` is what's live; keep `crates/gtfs_validator_web/website/` in sync. The `Website` workflow fails the build when they drift apart.
* The example feed behind the "Try an example feed" button is generated, not hand-edited. Change `scripts/build_demo_feed.py` and re-run it (`python3 scripts/build_demo_feed.py`) to refresh both copies; `--check` is what CI runs.
* Notice documentation is generated from the Rust schema and `src/notice_guides.json`. Run `cargo run -p gtfs-guru-web --bin generate-notice-pages` after changing a notice or guide. Refresh the bundled MobilityData snapshot with `python3 scripts/update_notice_metadata.py`; normal builds never require network access.
* `deploy/update.sh` rebuilds the Docker (axum) stack — that is **not** what serves the live domain.
* Server-level config (headers, TLS, caching) lives in `Caddyfile` and `website/nginx.conf` — since we control the server, custom headers (e.g. COOP/COEP for multithreaded WASM) can be set there.

---

## Releasing a New Version

A release is intentionally gated by a `v*` tag. Merging to `main` or running the
workflow manually only builds artifacts; it does not publish or deploy anything.

1. Update every package version and the Tauri version.
2. Run `python3 scripts/check-release-version.py --tag vX.Y.Z`.
3. Run the normal Rust, golden, WASM, and browser checks and merge to `main`.
4. Only after explicit release approval, push the matching `vX.Y.Z` tag.
5. Move the major-version tag the GitHub Action is published under, so that
   `abasis-ltd/gtfs.guru/action@v1` keeps resolving to the newest release:

   ```bash
   git tag -f v1 vX.Y.Z && git push -f origin v1
   ```

   Without this step every workflow pinned to `@v1` keeps running the previous
   release, and a brand-new major tag does not exist at all.

The tag workflow verifies version consistency before it does any build. It then:

* builds desktop installers and CLI archives for macOS, Linux, and Windows;
* creates the GitHub Release and updater manifest;
* publishes the Rust crates, Python wheel, and npm package;
* rebuilds both WASM tiers and synchronizes the static website to Hetzner;
* verifies that `https://gtfs.guru/pkg/package.json` reports the tag version.

Required release secrets are `CARGO_REGISTRY_TOKEN`, `PYPI_API_TOKEN`,
`NPM_TOKEN`, the Tauri/Apple signing secrets, `HETZNER_HOST`,
`HETZNER_SSH_KEY`, and `HETZNER_KNOWN_HOSTS`. `HETZNER_USER` defaults to
`botuser`; `HETZNER_PATH` defaults to `gtfs-guru-web/` in that user's home.

The known-hosts value must be provisioned out of band (for example from a
trusted existing SSH connection). The workflow deliberately does not use
`ssh-keyscan` at release time.
