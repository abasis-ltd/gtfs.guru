# Real-World Parity

The golden suite (`docs/golden.md`) pins byte-exact output on controlled
fixtures. It cannot see an ecosystem-wide regression, because its inputs are
small feeds this project wrote itself. This harness covers the other side:
twelve real MobilityData datasets carrying the rare feature combinations —
flexible services, Fares v2, pathways — at sizes from 31 KiB to 229 MiB
unpacked, checked on every commit and compared against the canonical Java
validator before every release.

The feed `.zip` files are never committed. They are pinned by immutable dataset
id and sha256 and downloaded on demand.

## Quick start

```bash
cargo build --release -p gtfs-guru
export GTFS_VALIDATOR_BIN=./target/release/gtfs-guru

python3 scripts/real_world_corpus.py fetch          # ~76 MiB, once
MODE=self scripts/ci_real_world.sh                  # what every commit runs
MODE=full scripts/ci_real_world.sh                  # adds the Java baseline
```

`GTFS_GURU_CORPUS_DIR` moves the corpus somewhere else; the default is
`benchmark-feeds/real-world/`, already covered by `.gitignore`.

## The corpus and why these feeds

Run `python3 scripts/real_world_corpus.py describe` for the live matrix. The
selection rests on three axes.

**Subfeature coverage.** The requested scope is flexible services, Fares v2 and
pathways, which MobilityData splits into 18 subfeatures. All 18 are covered.
That is harder than it sounds: the catalog holds roughly 2 800 active GTFS
feeds, and some of these subfeatures are close to unique — 3 feeds worldwide
publish Pathway Signs, 6 publish Time-Based Fares, 6 publish Fixed-Stops Demand
Responsive Transit. Several corpus entries are there because they are among the
only feeds that exercise a rule at all.

**Value-space depth.** A feature flag says a file exists, not that its
interesting values appear. The corpus deliberately covers pathway modes 1, 2, 4,
5, 6 and 7 including fare gates and exit gates (`mdb-502`), a complete
`areas.txt` plus `stop_areas.txt` wiring next to a feed whose area ids dangle
(`mdb-503` against `tld-716`), `fare_transfer_type` 0 and 2, all five
`fare_media_type` values, and booking types 0 and 1.

**Size.** Unpacked sizes climb roughly half a decade at a time: 0.03, 0.14,
0.23, 0.35, 1.3, 3.4, 3.4, 9.6, 17, 108, 115 and 229 MiB. The small end keeps
the per-commit gate fast; the large end is where memory behaviour and
super-linear algorithms show up. The largest feed holds 2.6 M `stop_times` rows
and drives peak RSS to roughly 0.9 GB.

Two further properties come along for free and are worth keeping: five
countries (US, CA, DE, FR, PT) so region-dependent rules fire, and a spread of
real-world defects — a stray `stop_times_old.txt`, header-only `pathways.txt`,
`fare_containers.txt` from a pre-final Fares v2 draft, booking-rule references
with no `booking_rules.txt`, and one feed with 72 455 errors where a small
regression could otherwise hide inside a large total.

`corpus.json` records, per feed, the subfeatures it is there for and a
`rationale` sentence. A `wanted` list at the end records what is knowingly
absent.

## Reproducible storage

Datasets are immutable snapshots served without authentication:

```
https://files.mobilitydatabase.org/<feed_id>/<dataset_id>/<dataset_id>.zip
```

`fetch` downloads only what is missing or hash-mismatched, verifies sha256
against `corpus.json`, and writes atomically. A hash mismatch is a hard error,
never a silent overwrite. `verify` re-hashes what is on disk and downloads
nothing.

## Detecting new versions

Two independent things drift, and neither is adopted automatically.

**Datasets.** Each feed also has a mutable `latest.zip` alias in the same
bucket. `check-updates` issues one `HEAD` against the pin and one against the
alias and compares ETags, so drift detection costs no downloads. When a feed has
moved it resolves the new immutable `dataset_id` for you:

```bash
python3 scripts/real_world_corpus.py check-updates --catalog
```

`--catalog` additionally reads the public MobilityData catalog CSV and reports a
feed that went `inactive` or was redirected. A stale pin is deliberately *not* a
failure: freezing the corpus is what makes notice baselines comparable over
time. Re-pin when you want the newer data, with
`scripts/real_world_corpus.py resolve <feed_id> --download` for the new fields,
and expect the baseline to move in the same commit.

**The Java baseline.** `gate.json` pins the release tag, asset name and sha256
of `MobilityData/gtfs-validator`. A cached jar is reused whenever its hash
matches the pin, so a warm cache does no network I/O; a mismatch is refused
rather than accepted. `jar --check-latest` reports a newer upstream release, and
`jar --adopt-latest` re-pins it deliberately.

## What is measured

Per feed, per validator: exit status and signal, timeout, wall seconds, peak RSS
(measured with `wait4` on the process itself, not self-reported), error, warning
and info totals, the notice fingerprint (every code with its severity and total),
the detected feature list, and parse failures. A parse failure means the
validator could not read the input — anything in `system_errors.json`, a
`csv_parsing_failed`, an unreadable or absent `report.json` — as distinct from
the feed merely being invalid.

## Expected delta versus regression

Two different comparisons, with two different approval mechanisms.

**Against the previous gtfs.guru result.** `scripts/real_world/baseline/*.json`
holds each feed's committed fingerprint. Any change is a regression until the
baseline is updated in the same commit, which makes the delta a reviewable JSON
diff rather than an invisible drift. Approving one is explicit:

```bash
python3 scripts/real_world_parity.py update-baseline real_world_actual/results.json \
  --reason "flex booking-rule rule now also checks drop-off windows (#123)"
```

`--reason` is mandatory and is stored in the file, so a reviewer sees why the
numbers moved, and `impact` can quote it later.

**Against MobilityData/gtfs-validator.** Cross-validator differences are
recorded in `expected_deltas.json` with a reason. Each entry pins both totals,
so a difference that *changes size* stops being covered and resurfaces. An
approval that no longer matches any real difference is stale and fails the run:
an allowlist nobody prunes decays into standing permission for a difference no
one has looked at since. Delete the entry in the commit that removes the
difference.

## The parity regression gate

Per feed, weighted: `crash_free` 3, `parse_clean` 3, `notices_stable` 2,
`features_stable` 1, `java_parity` 2 (only where the Java baseline ran). The
score is the share of applicable weight earned, and the run fails below
`min_score` (100 by default) — so a crash, a parse failure or an unexplained
notice change lowers the score and fails the build, exactly as intended. The
published score is rounded to two places; the pass decision uses the unrounded
ratio, so a near miss cannot round its way to a perfect run.

A feed the run was asked to validate but for which no result came back fails
`crash_free` and `parse_clean` rather than being skipped — otherwise a feed that
never ran would leave the score at 100. A tool absent because the run never
asked for it (`--tools java`) is still a skip.

`scripts/real_world_parity_test.py` covers these decisions offline, with no
corpus and no jar; the `Gate tests` job runs it on every pull request that
touches the harness.

Time and memory are deliberately *not* part of the score. Shared CI runners are
too noisy for that. They are compared against the baseline with loose thresholds
(3× wall time, 1.5× RSS, with floors) and reported as warnings; `--strict-perf`
promotes them to failures for a controlled machine.

## CI schedule

| Trigger | Mode | What it buys |
| --- | --- | --- |
| pull request, push to `main` | `self` | gtfs.guru alone over all 12 feeds against the committed baseline. About a minute with a warm corpus cache. |
| release tag (via `release.yml`) | `full` | Adds the Java baseline, the timing and memory table, and the impact report. Blocks the release. |
| Monday 05:00 UTC | `full` | Prophylactic: catches upstream validator releases, dataset drift and runner-image changes between releases. |
| manual dispatch | either | On demand. |

### Caching

Neither the corpus nor the jar is re-downloaded per release. Both caches are
keyed on what they hold rather than on the file describing it: the corpus key is
a hash over the sorted `dataset_id:sha256` pairs, and the jar key is the pinned
release tag plus the first half of its sha256. Editing a `rationale` string, a
gate weight or a performance threshold therefore leaves both caches intact, and
only a genuine re-pin evicts them. `ensure_jar` independently reuses any local
jar whose hash matches the pin, so a warm cache costs no network at all.

Two GitHub behaviours still allow an occasional re-download, neither of them a
correctness problem: caches idle for 7 days are evicted, and a cache created by
a tag run is scoped to that tag. In practice the Monday `full` run on `main`
keeps both caches warm and in the default-branch scope where tag runs can read
them. A partial restore is harmless — `fetch` downloads only what is missing.

Running the full cross-validator comparison on *every* commit was considered and
rejected: it costs tens of minutes and a 40 MB jar download per run, and its
timing numbers are too noisy to gate on. The cheap half of the signal — crashes,
parse failures, notice drift on real feeds — is what actually catches
regressions, and it is affordable per commit.

## Release impact report

Because baselines are committed, the impact of a release on real-world
validation is literally the diff of what the corpus reports:

```bash
python3 scripts/real_world_parity.py impact --since v1.2.3 --out impact.md
```

With no `--since` it uses the previous tag. It needs no network and no stored
artefacts, and reports feeds added, removed or re-pinned, a Java baseline move,
per-feed error and warning deltas, every notice code that changed, and the
recorded reason for each change. The `full` CI job writes it to the job summary
and uploads it as an artifact.

## The mdb-50 catalogue check

The corpus is the per-change gate. Before a release the same question is asked
on a wider set: fifty MobilityDatabase feeds chosen to cover countries,
features and sizes the corpus does not (`scripts/real_world/mdb50.json`). The
harness runs both validators on the same zips with the same date and country
code and diffs notice codes and totals per feed:

```bash
python3 scripts/mdb_parity.py fetch            # latest.zip for every manifest feed
python3 scripts/mdb_parity.py run              # guru and java, ~2 minutes
python3 scripts/mdb_parity.py compare          # writes benchmark-feeds/mdb-50/parity/comparison.json
```

`compare` classifies every difference the way the gate does. It loads
`expected_deltas.json` first (the two feeds shared with the corpus are approved
there) and then `mdb50_expected_deltas.json`, and exits 2 while any difference
is unexplained. A feed is *exact* when the fingerprints are identical and
*exact-on-shared* when every difference has an approval; the release target is
50/50 exact-on-shared. An approval whose pinned totals no longer match is
reported as stale rather than silently dropped.

The zips are `latest.zip` snapshots, not immutable dataset ids, so the manifest
records the sha256 each approval was triaged against and `run` prints a note
(never a failure) when a local zip differs. `run --tools guru` reuses Java
output from an earlier run, which is how a validator change is checked without
waiting for the JVM.

### Review follow-up (2026-09-26)

After the CSV and table-dependency fixes, the pinned corpus has 11/12 exact
notice fingerprints against Java 8.0.1. The remaining difference is the five
`service_never_active` notices on `mdb-502`, an intentional additional rule.
The `mdb-3234` baseline now matches Java; its two old approvals were removed.

The same Rust build against the saved Java reports for the fifty catalogue
snapshots yields 48/50 exact and 50/50 exact-on-shared. The remaining differences
are `service_never_active` and one `stops_match_shape_out_of_order` occurrence on
`mdb-1831` caused by floating point rounding, documented in the approvals.
Four obsolete skip-after-parse approvals were removed from the catalogue set.
These comparisons cover notice codes, severities and totals, not byte-for-byte
identity of all sample contexts or report presentation.

## Files

| Path | Purpose |
| --- | --- |
| `scripts/real_world/corpus.json` | Pinned feeds: dataset id, sha256, sizes, country, date, coverage, rationale |
| `scripts/real_world/gate.json` | Java baseline pin, timeouts, perf thresholds, gate weights |
| `scripts/real_world/expected_deltas.json` | Approved gtfs.guru vs Java differences |
| `scripts/real_world/baseline/*.json` | Per-feed committed notice fingerprint and counts |
| `scripts/real_world/mdb50.json` | The fifty catalogue feeds: id, provider, country, why chosen, snapshot sha256 |
| `scripts/real_world/mdb50_expected_deltas.json` | Approved differences on the catalogue set, same format |
| `scripts/mdb_parity.py` | `fetch`, `run`, `compare` on the catalogue set |
| `scripts/real_world_corpus.py` | `fetch`, `verify`, `check-updates`, `resolve`, `describe` |
| `scripts/real_world_parity.py` | `jar`, `run`, `gate`, `report`, `update-baseline`, `impact` |
| `scripts/ci_real_world.sh` | The wrapper CI runs |
| `.github/workflows/real-world-parity.yml` | Triggers and caching |

## Notes and limits

- The harness measures peak RSS with `fork` plus `wait4`, so it is POSIX-only.
  Linux and macOS work; Windows does not.
- `full` mode finds a JDK that is installed but absent from `PATH`: it tries
  `--java-bin`, then `JAVA_BIN`, then `PATH`, then `JAVA_HOME`, then
  `/usr/lib/jvm`, `/usr/java` and `/Library/Java`, and prints which one it chose.
  A missing runtime is a clear error rather than twelve feeds reported as
  crashes. The chosen binary, heap and `java -version` line are recorded in the
  results file, because wall time and RSS depend on the JVM. Fingerprints do not:
  JDK 21 and JDK 25 were verified to produce byte-identical notice output.
- Validation dates and country codes are pinned per feed, so notices that depend
  on "today" (expired calendars, feed expiration) stay stable indefinitely.
- Runs are single-threaded by default so wall times are comparable; the corpus
  finishes in about 30 seconds for gtfs.guru alone.
- `mdb-3218` and `mdb-84` are marked `inactive` in the MobilityData catalog.
  Their pinned snapshots stay downloadable, but if either ever disappears,
  `mdb-84` is the corpus's only `Continuous Stops` feed and would need a
  replacement.
