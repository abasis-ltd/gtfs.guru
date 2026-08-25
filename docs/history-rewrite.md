# Generated Artifact History Cleanup

GTF-29 keeps one canonical embedded WASM snapshot, stops tracking duplicate
package directories, and rejects unrelated build outputs. Old generations and
legacy binaries remain reachable from existing commits until the repository
history is rewritten. This maintenance operation is intentionally separate from
the code change because it replaces commit IDs on every affected branch and tag.

## Scope

`scripts/history-rewrite-paths.txt` is the reviewed removal policy. It covers
legacy CLI executables, benchmark/profile output, old website copies, and every
generated wasm-pack package. It deliberately preserves test-feed ZIP fixtures,
the canonical validator JAR, GUI icons, source assets, and release metadata.
The canonical `website/pkg` and `website/pkg-mt` snapshot is temporarily
removed with its old generations and restored once on the rewritten `main`.

## Preconditions

1. Merge the GTF-29 migration and confirm the release workflow can build and
   deploy both `website/pkg/` and `website/pkg-mt/` from a clean checkout.
2. Freeze pushes and merges. Record every branch and tag SHA from GitHub and
   notify contributors that existing clones must be replaced or hard-reset.
3. Disable branch protection only for the short publication window and require
   a second maintainer to verify the recorded refs and removal policy.
4. Work from a fresh mirror clone outside every developer checkout. Keep an
   untouched mirror backup until the rewritten repository has been validated.

## Prepare and verify a rewritten mirror

Replace the example paths with dedicated, empty locations. Never run this in a
normal working copy.

```bash
git clone --mirror https://github.com/abasis-ltd/gtfs.guru.git /absolute/path/gtfs-guru-rewrite.git
cp -R /absolute/path/gtfs-guru-rewrite.git /absolute/path/gtfs-guru-backup.git
git --git-dir=/absolute/path/gtfs-guru-rewrite.git archive \
  --format=tar --output=/absolute/path/current-wasm-assets.tar \
  main website/pkg website/pkg-mt
cd /absolute/path/gtfs-guru-rewrite.git

git filter-repo --force --invert-paths \
  --paths-from-file /absolute/path/gtfs.guru/scripts/history-rewrite-paths.txt

git fsck --full
git count-objects -vH
git rev-list --objects --branches --tags | cut -d' ' -f2- | \
  python3 /absolute/path/gtfs.guru/scripts/check_repository_artifacts.py \
    --paths-from-stdin --purge-canonical-website
```

Restore one current WASM snapshot on top of the rewritten `main`, then validate
from that non-bare checkout:

```bash
git clone /absolute/path/gtfs-guru-rewrite.git /absolute/path/gtfs-guru-verify
cd /absolute/path/gtfs-guru-verify
tar -xf /absolute/path/current-wasm-assets.tar
git add website/pkg website/pkg-mt
git commit -m "Restore canonical embedded WASM assets"
git push origin main

python3 scripts/check_repository_artifacts.py
cargo test -p gtfs-guru-web
```

Compare the rewritten heads and tags with the recorded ref names and inspect
the final `main` tree. Commit IDs will differ; ref names must match and the only
intentional tree change is the generated-artifact cleanup plus the restoration
commit on `main`. Historical tags stay source-only; published release artifacts
remain the immutable way to reproduce old binaries.

## Publish during the maintenance window

`git filter-repo` removes the original remote deliberately. After both
maintainers approve the local mirror, restore the exact GitHub remote and update
only branches and tags:

```bash
git remote add origin https://github.com/abasis-ltd/gtfs.guru.git
git push --force origin --all
git push --force origin --tags
```

Immediately restore branch protection, clone GitHub into a new directory, run
`git fsck --full`, the repository artifact check, and the release-critical test
suite. Keep the backup mirror until production and the next release are healthy.
Do not merge old branches after publication; recreate required work by
cherry-picking patches onto the rewritten `main`.
