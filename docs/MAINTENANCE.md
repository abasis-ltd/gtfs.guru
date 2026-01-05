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

When you want to publish a new version (e.g., to PyPI or Crates.io):

1. Update version numbers in `Cargo.toml`.
2. Commit and merge to `main`.
3. Create a GitHub Release:
    * Go to "Releases" -> "Draft a new release".
    * Create a tag (e.g., `v0.2.0`).
    * Click "Publish release".

The CI/CD pipeline (`.github/workflows/release.yml`) will automatically:

* Build binaries for all platforms.
* Publish to PyPI.
* Upload assets to the release page.
