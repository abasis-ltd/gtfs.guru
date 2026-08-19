# Spec Watch: tracking the GTFS specification and the canonical validator

GTFS Guru follows two upstreams it does not release with:

- the specification itself, [`google/transit`](https://github.com/google/transit),
  whose `gtfs/spec/en/reference.md` defines every file, field, and enum value;
- the canonical validator,
  [`MobilityData/gtfs-validator`](https://github.com/MobilityData/gtfs-validator),
  whose releases publish a `rules.json` asset listing every notice code.

Both move on their own schedule. Spec Watch is the reproducible way to notice a
new field, enum value, or rule *before* our support stops being current.

## The pieces

| Piece | What it is |
| --- | --- |
| `crates/gtfs_validator_core/spec_baseline.json` | The upstream state this build is aligned with, plus the differences we have consciously accepted. Single source of truth. |
| `gtfs-guru spec-surface` | What this build actually supports — files, fields, required fields, enum domains, notice codes — read out of the same tables validation uses. |
| `scripts/spec_watch.py` | The detector: fetches upstream, compares it to the surface, subtracts the baseline, reports what is left. |
| `scripts/spec_watch_test.py` | Offline tests that inject a field, an enum value, and a rule into committed fixtures and assert each is detected. |
| `.github/workflows/spec-watch.yml` | Weekly run (Mondays, 06:17 UTC), plus `workflow_dispatch`, plus the detector tests on every relevant pull request. |

Reports carry the baseline too: every JSON report's `summary` has
`specRevision` and `canonicalBaseline`, so a stored report says which upstream
state produced it.

```console
$ gtfs-guru -i feed.zip --stdout | jq '.summary | {validatorVersion, specRevision, canonicalBaseline}'
{
  "validatorVersion": "1.0.0",
  "specRevision": "google/transit@3215f98f26615f1b925dca1bf2205311b747e308",
  "canonicalBaseline": "MobilityData/gtfs-validator@v8.0.1"
}
```

## What counts as drift

A run reports only when at least one of these holds:

1. A field, enum value, file, or notice difference exists that
   `spec_baseline.json` does not already acknowledge.
2. The specification reference moved to a commit newer than the baseline.
3. The canonical validator published a release newer than the baseline.

Anything already listed under `acknowledged` is silent — that is what keeps the
run quiet week after week. With no drift the script prints one line, writes no
files, and touches no issue tracker.

The finding categories are:

| Category | Meaning |
| --- | --- |
| `specFilesNotSupported` | The reference documents a file gtfs.guru does not model. |
| `specFieldsNotSupported` | The reference documents a column gtfs.guru would report as unknown. |
| `fieldsNotInSpec` | gtfs.guru accepts a column the reference does not list (extensions, or a field the spec dropped). |
| `requiredMismatches` | The reference marks a field **Required** and gtfs.guru does not. |
| `enumValuesNotSupported` | The reference defines an enum value gtfs.guru rejects. |
| `enumValuesNotInSpec` | gtfs.guru accepts an enum value the reference does not define. |
| `canonicalNoticesNotImplemented` | The canonical validator publishes a notice code gtfs.guru never emits. |
| `noticesNotInCanonical` | gtfs.guru emits a notice code the canonical validator does not have. |

Presence is only compared for fields the reference marks plainly **Required**.
*Conditionally Required* is prose, not data, so a mechanical comparison of it
would produce noise instead of signal — those changes surface as a moved spec
revision (reason 2) for a human to read.

## Running it by hand

Build the CLI first and hand it over, so the script does not fall back to
`cargo run`:

```bash
cargo build --release -p gtfs-guru
GTFS_VALIDATOR_BIN=./target/release/gtfs-guru python3 scripts/spec_watch.py check
```

On drift it writes `target/spec-watch/spec-drift.md` and
`target/spec-watch/spec-drift.json`. Useful flags:

- `--fail-on-drift` — exit 3 instead of 0, for a local gate.
- `--linear-dry-run` — print what would be sent to Linear.
- `--state-file <path>` — remember the last fingerprint so repeated local runs
  stay quiet until something actually changes.
- `--report-dir <path>` — where the two report files go.
- `--surface <path>` — use a saved `gtfs-guru spec-surface` JSON instead of
  running the CLI.

The offline tests need neither network nor cargo:

```bash
python3 scripts/spec_watch_test.py
```

## Idempotence

Each report carries a fingerprint over the upstream revisions and the
unacknowledged findings — deliberately not over the clock or the pull request
list. Rerunning with nothing new upstream produces the same fingerprint, so:

- the report files are rewritten byte-for-byte identical;
- the Linear issue is recognised as already reporting this drift and left alone.

## Linear

On drift the script keeps **one open issue per team**, titled
*"Spec watch: GTFS specification and canonical validator drift"*, in the
[GTF team](https://linear.app/abasis/team/GTF/all). The report body is the issue
description and ends with a marker:

```html
<!-- gtfs-guru-spec-watch fingerprint=359e084380d465f1 -->
```

That marker is the dedup key:

| Situation | What happens |
| --- | --- |
| No open issue with that title | A new issue is created. |
| Open issue, same fingerprint | Nothing. No update, no comment. |
| Open issue, different fingerprint | The description is replaced and one comment is posted. |
| Only closed issues with that title | A new issue is created — closing the issue is how you tell Spec Watch the gap was handled. |

### Configuring remote access

The script talks to `https://api.linear.app/graphql` with a single credential
from the `LINEAR_API_KEY` environment variable, and resolves the team from its
key (`GTF`) at run time, so no team ID has to be stored.

**Where the credential has to live.** GitHub does not expose secrets to
workflows triggered from a fork, and scheduled workflows run in the repository
that owns them. The weekly run therefore happens in
`abasis-ltd/gtfs.guru`, and the secret has to be added **there** by someone with
admin rights on that repository:

*Settings → Secrets and variables → Actions → New repository secret*, name
`LINEAR_API_KEY`. Optionally add a repository *variable* `LINEAR_TEAM_KEY` to
point the watcher at a team other than `GTF`.

A fork cannot supply that secret to the upstream run, and a pull request from a
fork never sees it. That is intentional: a pull request would otherwise be able
to exfiltrate the key. Work on the watcher is therefore verified by the offline
detector tests, which need no credential at all.

**Which credential.** Two options, in order of preference:

1. **A personal API key from a dedicated Linear account** (a "bot" member of the
   workspace). Linear → *Settings → Security & access → Personal API keys → New
   key*. Issues then appear as created by that account rather than by a person,
   and revoking it does not disturb anyone's own key. Sent as
   `Authorization: <key>` — no `Bearer` prefix.
2. **A personal API key from a maintainer's account** — simplest, and fine to
   start with. Issues appear as created by that person. Whoever owns the key can
   revoke it at any time from the same settings page.

An OAuth application (`Authorization: Bearer <token>`, with `actor=application`
so issues are attributed to the app) is the tidiest option if the workspace
already has one; the script accepts such a token in the same variable and
detects the `lin_oauth` prefix. Setting up an OAuth app just for this is more
work than it is worth.

**To verify without touching Linear**, run the workflow by hand from the Actions
tab with *"Print the Linear payload instead of sending it"* checked, or locally:

```bash
GTFS_VALIDATOR_BIN=./target/release/gtfs-guru \
  python3 scripts/spec_watch.py check --linear-dry-run
```

With no `LINEAR_API_KEY` set the run still writes its report and uploads it as
the `spec-drift-report` artifact; it just says the tracker was skipped.

## Protocol: moving the baseline

Moving the baseline means *"this upstream state is now the one we answer for."*
It is a deliberate act, always reviewed, never automatic.

1. **Read the report.** `spec-drift.md` links the specification diff, every
   merged pull request that touched the static spec, and each new canonical
   release.
2. **Close the real gaps first.** Implement the missing fields, enum values, or
   rules in a normal branch and pull request. A baseline moved over an
   unimplemented field silently converts a gap into an accepted difference.
3. **Regenerate the baseline** on that same branch:

   ```bash
   cargo build --release -p gtfs-guru
   GTFS_VALIDATOR_BIN=./target/release/gtfs-guru \
     python3 scripts/spec_watch.py update-baseline
   ```

   This rewrites `spec_baseline.json`: the new spec commit, the new canonical
   release, and the differences that remain as `acknowledged`.
4. **Review the `acknowledged` diff line by line.** Every entry there is a
   promise that the difference is intended. Entries that are not intended are
   bugs to fix in step 2, not lines to commit. Add a note to the pull request
   describing why each new entry is deliberate.
5. **Rebuild and re-run the checks.** The baseline is compiled into the binary,
   so reports only quote the new revision after a rebuild:

   ```bash
   cargo build --release -p gtfs-guru
   cargo test -p gtfs-guru-core -p gtfs-guru-report
   GTFS_VALIDATOR_BIN=./target/release/gtfs-guru scripts/ci_golden.sh
   GTFS_VALIDATOR_BIN=./target/release/gtfs-guru python3 scripts/spec_watch.py check
   ```

   The last command must print `no drift`.
6. **Close the Linear issue** once the pull request lands. The next drift opens
   a fresh one.

The current `acknowledged` set records, among others: `route_branding_url` and
`trips.continuous_pickup`/`continuous_drop_off` (Google extensions the reference
does not list), `fare_attributes.transfers` and `transfers.transfer_type`
(**Required** in the reference but legitimately empty in practice), and the
gtfs.guru-only notices such as `unused_stop` and the `--google-rules` checks.

## When the detector itself needs work

The comparison is only as good as its two parsers.

- `parse_spec_reference` in `scripts/spec_watch.py` reads the reference's field
  tables. If upstream restructures those tables, the script fails loudly
  (`the specification reference yielded no file sections`) rather than reporting
  a clean bill of health.
- `spec_surface()` in `crates/gtfs_validator_core/src/spec_surface.rs` reads
  `csv_schema.rs` and the enum tables in `csv_validation.rs`. A new enum column
  is picked up automatically; a unit test fails if one stops exporting its
  values.

Both are covered by `scripts/spec_watch_test.py`, which injects a synthetic
field, enum value, and rule into committed fixtures and asserts each is found.
Extend those tests when you extend the parsers.
