# GTFS Guru — GitHub Action

Validate a GTFS feed on every push. Errors show up as annotations in the
Security tab, the counts go into the job summary, and a bad feed fails the job.

```yaml
- uses: abasis-ltd/gtfs.guru/action@v1
  with:
    feed: feed.zip
```

The action downloads a released `gtfs-guru` binary (checksum-verified), runs it,
and reports the result. No JVM, no Docker, ~2 seconds of startup.

## Quick start

```yaml
name: Validate GTFS
on: [push, pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write   # so SARIF reaches the Security tab
    steps:
      - uses: actions/checkout@v4
      - uses: abasis-ltd/gtfs.guru/action@v1
        with:
          feed: feed.zip
          fail-on: error
```

## Validate a published feed instead of a committed one

```yaml
      - uses: abasis-ltd/gtfs.guru/action@v1
        with:
          url: https://www.example-transit.org/gtfs.zip
          country-code: US
```

Pair it with `on: schedule` to catch a feed that rots after publication —
expired calendars are the single most common reason a live feed stops working.

## Status badge

Point the action at a badge file, publish that file, and reference it from a
README. The descriptor is a [shields.io endpoint][endpoint], so the rendering
stays consistent with every other badge in the row.

```yaml
      - uses: abasis-ltd/gtfs.guru/action@v1
        with:
          feed: feed.zip
          fail-on: none          # a badge that only ever renders green is not a badge
          badge: badge/gtfs.json

      - name: Publish the badge
        uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: badge
          keep_files: true
```

```markdown
![GTFS](https://img.shields.io/endpoint?url=https://YOUR-ORG.github.io/YOUR-REPO/gtfs.json)
```

`badge-svg:` writes a self-contained SVG instead, for places that cannot reach
shields.io.

## Inputs

| Input | Default | Description |
| --- | --- | --- |
| `feed` | | Path to a GTFS zip or unpacked directory. |
| `url` | | Remote GTFS zip to download instead. Mutually exclusive with `feed`. |
| `version` | `latest` | Validator release to run: `latest` or a tag such as `v1.0.0`. |
| `fail-on` | `error` | Fail the job at `none`, `error`, or `warning`. |
| `country-code` | | ISO country code for region-specific rules, e.g. `US`. |
| `date` | | Validate against `YYYY-MM-DD` instead of today. |
| `thorough` | `false` | Also report missing recommended fields and columns. |
| `google-rules` | `false` | Enable the extra rules Google's ingestion applies. |
| `output` | `gtfs-guru-report` | Directory for the generated reports. |
| `sarif` | `true` | Write `report.sarif.json`. |
| `upload-sarif` | `true` | Upload it to code scanning. Needs `security-events: write`. |
| `badge` | | Path for a shields.io endpoint descriptor. |
| `badge-svg` | | Path for a self-contained SVG badge. |
| `badge-label` | `GTFS` | Left-hand text on the badge. |
| `summary` | `true` | Write a result table to the job summary. |

## Outputs

| Output | Description |
| --- | --- |
| `errors` / `warnings` / `infos` | Notice counts by severity. |
| `valid` | `true` when there are no errors. |
| `passed` | `true` when the feed met `fail-on`. |
| `report-json` / `report-html` | Paths to the generated reports. |
| `sarif-file` / `badge-file` | Paths to the SARIF and badge files, empty when disabled. |
| `version` | Validator version that produced the report. |

Outputs are set even when the feed fails the threshold, so a later step can act
on the numbers:

```yaml
      - id: gtfs
        uses: abasis-ltd/gtfs.guru/action@v1
        with:
          feed: feed.zip
          fail-on: none

      - if: steps.gtfs.outputs.valid != 'true'
        run: echo "::warning::${{ steps.gtfs.outputs.errors }} errors in the feed"
```

## Keeping the reports

The reports are ordinary files in `output`, so upload them like any artifact:

```yaml
      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: gtfs-report
          path: gtfs-guru-report/
```

## Notes

- Runs on `ubuntu-*`, `macos-*` and `windows-*` runners, x86_64 and arm64.
- Code scanning upload is best-effort: private repositories without GitHub
  Advanced Security cannot accept SARIF, and that never masks the verdict.
- Exit status: the step fails only when the feed reaches the `fail-on`
  threshold, or when the run itself could not produce a verdict.
- The runner needs `python3` or `jq` to read the counts. Both ship with every
  GitHub-hosted runner.

[endpoint]: https://shields.io/badges/endpoint-badge
