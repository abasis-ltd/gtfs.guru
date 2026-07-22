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

## Releasing a New Version

A release is intentionally gated by a `v*` tag. Merging to `main` or running the
workflow manually only builds artifacts; it does not publish or deploy anything.

1. Update every package version and the Tauri version.
2. Run `python3 scripts/check-release-version.py --tag vX.Y.Z`.
3. Run the normal Rust, golden, WASM, and browser checks and merge to `main`.
4. Only after explicit release approval, push the matching `vX.Y.Z` tag.

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
