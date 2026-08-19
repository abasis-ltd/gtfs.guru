#!/usr/bin/env python3
"""Watch the GTFS specification and the canonical validator for drift.

The specification (`google/transit`) and the canonical validator
(`MobilityData/gtfs-validator`) move independently of gtfs.guru releases. This
script compares what this build supports -- straight out of
`gtfs-guru spec-surface` -- against the published reference and the canonical
validator's `rules.json`, using `crates/gtfs_validator_core/spec_baseline.json`
as the accepted state.

It is quiet by design: with no drift it prints one line, writes nothing, and
touches no issue tracker. `docs/spec-watch.md` documents the workflow and the
protocol for moving the baseline.

Subcommands
    check            compare upstream against the baseline and report drift
    update-baseline  accept the current upstream state as the new baseline
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from typing import Any

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_BASELINE = ROOT / "crates" / "gtfs_validator_core" / "spec_baseline.json"
DEFAULT_REPORT_DIR = ROOT / "target" / "spec-watch"

GITHUB_API = "https://api.github.com"
RAW_CONTENT = "https://raw.githubusercontent.com"
USER_AGENT = "gtfs-guru-spec-watch"

LINEAR_API = "https://api.linear.app/graphql"
LINEAR_ISSUE_TITLE = "Spec watch: GTFS specification and canonical validator drift"
LINEAR_MARKER = "gtfs-guru-spec-watch"

# Every finding category, in report order. The value is the shape the category
# uses: a flat list of names, a mapping of file to names, or a mapping of
# "file:field" to enum values.
FINDING_CATEGORIES: list[tuple[str, str, str]] = [
    ("specFilesNotSupported", "list", "Specification files gtfs.guru does not model"),
    ("specFieldsNotSupported", "map", "Specification fields gtfs.guru does not accept"),
    ("fieldsNotInSpec", "map", "Fields gtfs.guru accepts that the specification omits"),
    ("requiredMismatches", "map", "Fields the specification requires that gtfs.guru does not"),
    ("enumValuesNotSupported", "values", "Enum values gtfs.guru rejects that the specification defines"),
    ("enumValuesNotInSpec", "values", "Enum values gtfs.guru accepts that the specification omits"),
    ("canonicalNoticesNotImplemented", "list", "Canonical notice codes gtfs.guru does not emit"),
    ("noticesNotInCanonical", "list", "Notice codes gtfs.guru emits that the canonical validator does not"),
]

# Specification paths whose changes can alter fields, enums, or rules. Realtime
# and translation directories move on their own cadence and are reported as
# context only.
STATIC_SPEC_PREFIXES = ("gtfs/spec/en/",)

# `0` (or empty) - **Stop**. A backticked integer introducing an option, which is
# how the reference spells out every enum domain.
ENUM_OPTION_RE = re.compile(
    r"`(-?\d+)`(?:\s*\(\s*or\s+empty\s*\)|\s+or\s+empty|\s*\(\s*or\s+blank\s*\))?\s*[-–—]\s"
)
SECTION_RE = re.compile(r"^###\s+(\S+?)\s*$")
FIELD_NAME_RE = re.compile(r"`([a-z][a-z0-9_]*)`")


class WatchError(RuntimeError):
    """A failure that should stop the run with a diagnostic, not a traceback."""


# ---------------------------------------------------------------------------
# HTTP


class _StripAuthOnCrossHostRedirect(urllib.request.HTTPRedirectHandler):
    """Drop `Authorization` when a redirect leaves the host it was meant for.

    `urllib` copies every header but `Content-Length`/`Content-Type` onto the
    redirected request, so a token sent to github.com would follow a release
    asset all the way to its CDN host. GitHub redirects release downloads to
    objects.githubusercontent.com, which has no business seeing the token.
    """

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        new_request = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new_request is None:
            return None
        if urllib.parse.urlsplit(newurl).netloc != urllib.parse.urlsplit(req.full_url).netloc:
            # Request normalises header names to Capitalised-Form.
            new_request.headers.pop("Authorization", None)
        return new_request


_OPENER = urllib.request.build_opener(_StripAuthOnCrossHostRedirect)


def http_get(url: str, token: str | None = None, accept: str | None = None) -> bytes:
    request = urllib.request.Request(url)
    request.add_header("User-Agent", USER_AGENT)
    if accept:
        request.add_header("Accept", accept)
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with _OPENER.open(request, timeout=60) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:400]
        raise WatchError(f"GET {url} failed with HTTP {error.code}: {detail}") from error
    except urllib.error.URLError as error:
        raise WatchError(f"GET {url} failed: {error.reason}") from error


def github_json(path: str, token: str | None) -> Any:
    url = path if path.startswith("http") else f"{GITHUB_API}{path}"
    return json.loads(http_get(url, token, accept="application/vnd.github+json"))


# ---------------------------------------------------------------------------
# The gtfs.guru side


def validator_cmd(gtfs_bin: str | None) -> list[str]:
    if gtfs_bin:
        return [gtfs_bin]
    return ["cargo", "run", "--release", "-q", "-p", "gtfs-guru", "--"]


def load_surface(surface_path: pathlib.Path | None) -> dict:
    """The files, fields, enum values, and notices this build supports."""
    if surface_path is not None:
        return json.loads(surface_path.read_text(encoding="utf-8"))
    cmd = validator_cmd(os.environ.get("GTFS_VALIDATOR_BIN")) + ["spec-surface"]
    try:
        completed = subprocess.run(
            cmd, cwd=ROOT, check=True, capture_output=True, text=True
        )
    except FileNotFoundError as error:
        raise WatchError(f"could not run {cmd[0]}: {error}") from error
    except subprocess.CalledProcessError as error:
        raise WatchError(
            f"{' '.join(cmd)} failed with exit {error.returncode}: {error.stderr.strip()[:400]}"
        ) from error
    return json.loads(completed.stdout)


# ---------------------------------------------------------------------------
# The specification side


def parse_spec_reference(text: str) -> dict[str, dict]:
    """Fields, presence, and enum domains per file, from the reference markdown.

    The reference lists every file as an `### <name>` section followed by a
    field table, so the tables are the specification's own machine-readable
    surface. Files with no field table (`locations.geojson` describes GeoJSON
    objects instead) end up with an empty field map, which the comparison then
    skips.
    """
    files: dict[str, dict] = {}
    current: dict | None = None
    for line in text.splitlines():
        section = SECTION_RE.match(line)
        if section:
            name = section.group(1)
            if name.endswith(".txt") or name.endswith(".geojson"):
                current = {"fields": {}}
                files[name] = current
            else:
                current = None
            continue
        if current is None or not line.lstrip().startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 4 or cells[0].startswith("---") or "Field Name" in cells[0]:
            continue
        name_match = FIELD_NAME_RE.search(cells[0])
        if not name_match:
            continue
        field_type = cells[1]
        presence = cells[2]
        description = cells[3] if len(cells) > 3 else ""
        entry = {
            "type": field_type,
            "presence": normalize_presence(presence),
            "enumValues": [],
        }
        if field_type == "Enum":
            values = sorted({int(value) for value in ENUM_OPTION_RE.findall(description)})
            entry["enumValues"] = values
        current["fields"][name_match.group(1)] = entry
    return files


def normalize_presence(cell: str) -> str:
    """`**Conditionally Required**` and friends, reduced to a bare word."""
    plain = cell.replace("*", "").strip()
    lowered = plain.lower()
    if lowered.startswith("conditionally required"):
        return "ConditionallyRequired"
    if lowered.startswith("conditionally forbidden"):
        return "ConditionallyForbidden"
    if lowered.startswith("required"):
        return "Required"
    if lowered.startswith("recommended"):
        return "Recommended"
    if lowered.startswith("forbidden"):
        return "Forbidden"
    if lowered.startswith("optional"):
        return "Optional"
    return plain or "Unknown"


# ---------------------------------------------------------------------------
# Upstream state


def fetch_spec_head(baseline: dict, token: str | None) -> dict:
    revision = baseline["specRevision"]
    repo = revision["repository"]
    path = revision["specPaths"][0]
    commits = github_json(
        f"/repos/{repo}/commits?path={path}&sha={revision['ref']}&per_page=1", token
    )
    if not commits:
        raise WatchError(f"no commits found for {repo}:{path}")
    head = commits[0]
    return {
        "repository": repo,
        "ref": revision["ref"],
        "commit": head["sha"],
        "committedAt": head["commit"]["committer"]["date"],
        "message": head["commit"]["message"].splitlines()[0],
        "url": head["html_url"],
        "specPaths": revision["specPaths"],
    }


def fetch_spec_text(head: dict, token: str | None) -> str:
    repo = head["repository"]
    path = head["specPaths"][0]
    return http_get(f"{RAW_CONTENT}/{repo}/{head['commit']}/{path}", token).decode("utf-8")


def fetch_merged_pulls(
    baseline: dict, token: str | None, max_pages: int, max_file_lookups: int
) -> tuple[list[dict], bool]:
    """Pull requests merged into the spec repository since the baseline commit.

    Sorted by merge time, newest first, and tagged with whether they touched the
    static specification, so a reviewer can tell a reference change from a
    realtime-only one without opening each pull request.

    Returns the list and whether the page cap was reached before the scan ran
    past the baseline, so the report can say the list may be incomplete rather
    than presenting a truncated list as the whole story.
    """
    repo = baseline["specRevision"]["repository"]
    since = baseline["specRevision"]["committedAt"]
    merged: list[dict] = []
    page_cap_reached = True
    for page in range(1, max_pages + 1):
        batch = github_json(
            f"/repos/{repo}/pulls?state=closed&sort=updated&direction=desc"
            f"&per_page=100&page={page}",
            token,
        )
        if not batch:
            page_cap_reached = False
            break
        # Sorted by update time, so once a whole page predates the baseline there
        # is nothing newer further back.
        if all(pull["updated_at"] <= since for pull in batch):
            page_cap_reached = False
            break
        for pull in batch:
            merged_at = pull.get("merged_at")
            if merged_at and merged_at > since:
                merged.append(
                    {
                        "number": pull["number"],
                        "title": pull["title"],
                        "url": pull["html_url"],
                        "mergedAt": merged_at,
                        "touchesStaticSpec": None,
                    }
                )
    merged.sort(key=lambda pull: pull["mergedAt"], reverse=True)
    for pull in merged[:max_file_lookups]:
        files = github_json(f"/repos/{repo}/pulls/{pull['number']}/files?per_page=100", token)
        pull["touchesStaticSpec"] = any(
            entry["filename"].startswith(STATIC_SPEC_PREFIXES) for entry in files
        )
    return merged, page_cap_reached


def fetch_canonical_release(baseline: dict, token: str | None) -> dict:
    repo = baseline["canonicalBaseline"]["repository"]
    releases = github_json(f"/repos/{repo}/releases?per_page=30", token)
    published = [
        release
        for release in releases
        if not release.get("draft") and not release.get("prerelease")
    ]
    if not published:
        raise WatchError(f"no published releases found for {repo}")
    published.sort(key=lambda release: release["published_at"], reverse=True)
    latest = published[0]
    asset_name = baseline["canonicalBaseline"]["rulesAsset"]
    rules_url = next(
        (
            asset["browser_download_url"]
            for asset in latest.get("assets", [])
            if asset["name"] == asset_name
        ),
        None,
    )
    if rules_url is None:
        raise WatchError(
            f"release {latest['tag_name']} of {repo} has no {asset_name} asset; "
            "the canonical notice list cannot be compared"
        )
    return {
        "repository": repo,
        "version": latest["tag_name"],
        "publishedAt": latest["published_at"],
        "url": latest["html_url"],
        "rulesAsset": asset_name,
        "rulesUrl": rules_url,
        "newerReleases": [
            {
                "version": release["tag_name"],
                "publishedAt": release["published_at"],
                "url": release["html_url"],
            }
            for release in published
            if release["published_at"] > baseline["canonicalBaseline"]["publishedAt"]
        ],
    }


def canonical_notice_codes(rules: dict) -> dict[str, str]:
    """Notice code to severity, from a canonical validator `rules.json`."""
    codes: dict[str, str] = {}
    for code, entry in rules.items():
        if isinstance(entry, dict) and entry.get("deprecated"):
            continue
        severity = entry.get("severityLevel", "UNKNOWN") if isinstance(entry, dict) else "UNKNOWN"
        codes[code] = severity
    return codes


# ---------------------------------------------------------------------------
# Comparison


def compare(surface: dict, spec: dict[str, dict], canonical: dict[str, str]) -> dict:
    """Every difference between this build and upstream, before acknowledgement."""
    findings: dict[str, Any] = {key: {} if shape != "list" else [] for key, shape, _ in FINDING_CATEGORIES}
    surface_files = surface["files"]

    for name, spec_file in sorted(spec.items()):
        surface_file = surface_files.get(name)
        if surface_file is None:
            findings["specFilesNotSupported"].append(name)
            continue
        if not surface_file["hasFieldSchema"] or not spec_file["fields"]:
            # `locations.geojson` has no column schema on either side.
            continue
        supported = set(surface_file["fields"])
        required = set(surface_file["requiredFields"])
        spec_fields = spec_file["fields"]

        missing = sorted(set(spec_fields) - supported)
        if missing:
            findings["specFieldsNotSupported"][name] = missing
        extra = sorted(supported - set(spec_fields))
        if extra:
            findings["fieldsNotInSpec"][name] = extra

        not_required = sorted(
            field
            for field, entry in spec_fields.items()
            if entry["presence"] == "Required" and field in supported and field not in required
        )
        if not_required:
            findings["requiredMismatches"][name] = not_required

        for field, entry in sorted(spec_fields.items()):
            spec_values = entry["enumValues"]
            if not spec_values or field not in supported:
                continue
            accepted = surface_file["enums"].get(field)
            if accepted is None:
                # The column exists but this build treats it as free-form, which
                # the field-level comparison cannot see.
                findings["enumValuesNotSupported"][f"{name}:{field}"] = spec_values
                continue
            unsupported = sorted(set(spec_values) - set(accepted))
            if unsupported:
                findings["enumValuesNotSupported"][f"{name}:{field}"] = unsupported
            undocumented = sorted(set(accepted) - set(spec_values))
            if undocumented:
                findings["enumValuesNotInSpec"][f"{name}:{field}"] = undocumented

    guru_notices = set(surface["notices"])
    findings["canonicalNoticesNotImplemented"] = sorted(set(canonical) - guru_notices)
    findings["noticesNotInCanonical"] = sorted(guru_notices - set(canonical))
    return findings


def acknowledged_shape() -> dict:
    return {key: [] if shape == "list" else {} for key, shape, _ in FINDING_CATEGORIES}


def subtract_acknowledged(findings: dict, acknowledged: dict) -> dict:
    """Only what the baseline has not already accepted."""
    fresh: dict[str, Any] = {}
    for key, shape, _ in FINDING_CATEGORIES:
        known = acknowledged.get(key) or ([] if shape == "list" else {})
        if shape == "list":
            remaining = [item for item in findings[key] if item not in known]
            if remaining:
                fresh[key] = remaining
            continue
        remaining_map: dict[str, list] = {}
        for holder, items in findings[key].items():
            known_items = known.get(holder) or []
            left = [item for item in items if item not in known_items]
            if left:
                remaining_map[holder] = left
        if remaining_map:
            fresh[key] = remaining_map
    return fresh


def count_findings(findings: dict) -> int:
    total = 0
    for value in findings.values():
        if isinstance(value, list):
            total += len(value)
        else:
            total += sum(len(items) for items in value.values())
    return total


# ---------------------------------------------------------------------------
# Reporting


def fingerprint(state: dict) -> str:
    """A stable identity for "this much drift, against this upstream state".

    Deliberately excludes wall-clock time and the merged pull request list, so a
    rerun with nothing new upstream produces the same value and the issue tracker
    stays untouched.
    """
    payload = {
        "specCommit": state["spec"]["commit"],
        "canonicalVersion": state["canonical"]["version"],
        "baselineSpecCommit": state["baseline"]["specRevision"]["commit"],
        "baselineCanonicalVersion": state["baseline"]["canonicalBaseline"]["version"],
        "findings": state["findings"],
    }
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()[:16]


def short(commit: str) -> str:
    return commit[:7]


def render_markdown(state: dict) -> str:
    baseline = state["baseline"]
    spec = state["spec"]
    canonical = state["canonical"]
    findings = state["findings"]
    reasons = state["reasons"]
    spec_repo = spec["repository"]

    total = count_findings(findings)
    headline = (
        f"`scripts/spec_watch.py` found {total} unacknowledged difference(s) between "
        f"gtfs.guru {state['validatorVersion']} and upstream."
        if total
        else "`scripts/spec_watch.py` found no unacknowledged field, enum, or notice "
        f"difference; gtfs.guru {state['validatorVersion']} is being reported because "
        "upstream moved and the change needs reading."
    )
    lines = [
        "# GTFS specification and canonical validator drift",
        "",
        headline,
        "",
        "## Revisions",
        "",
        "| Source | Baseline | Upstream |",
        "| --- | --- | --- |",
        # Links are derived from the revision itself rather than from whatever
        # URL the API happened to return, so both columns always point at what
        # the row names.
        "| GTFS specification (`{repo}`) | [`{base}`](https://github.com/{repo}/commit/{basefull}) ({basedate}) |"
        " [`{head}`](https://github.com/{repo}/commit/{headfull}) ({headdate}) |".format(
            repo=spec_repo,
            base=short(baseline["specRevision"]["commit"]),
            basefull=baseline["specRevision"]["commit"],
            basedate=baseline["specRevision"]["committedAt"][:10],
            head=short(spec["commit"]),
            headfull=spec["commit"],
            headdate=spec["committedAt"][:10],
        ),
        "| Canonical validator (`{repo}`) | [{base}](https://github.com/{repo}/releases/tag/{base}) ({basedate}) |"
        " [{head}](https://github.com/{repo}/releases/tag/{head}) ({headdate}) |".format(
            repo=canonical["repository"],
            base=baseline["canonicalBaseline"]["version"],
            basedate=baseline["canonicalBaseline"]["publishedAt"][:10],
            head=canonical["version"],
            headdate=canonical["publishedAt"][:10],
        ),
        "",
        f"Specification diff: https://github.com/{spec_repo}/compare/"
        f"{baseline['specRevision']['commit']}...{spec['commit']}",
        "",
        "## Why this report exists",
        "",
    ]
    lines.extend(f"- {reason}" for reason in reasons)
    lines.append("")

    static_pulls = [pull for pull in state["pulls"] if pull["touchesStaticSpec"] is True]
    unclassified_pulls = [pull for pull in state["pulls"] if pull["touchesStaticSpec"] is None]
    elsewhere_pulls = [pull for pull in state["pulls"] if pull["touchesStaticSpec"] is False]
    lines.extend(["## Specification pull requests merged since the baseline", ""])
    if static_pulls:
        for pull in static_pulls:
            lines.append(
                f"- [#{pull['number']}]({pull['url']}) {pull['title']} "
                f"(merged {pull['mergedAt'][:10]})"
            )
    elif unclassified_pulls:
        # "None" would be a claim the run cannot make while pull requests are
        # still unclassified.
        lines.append(
            "- None among the classified pull requests; "
            f"{len(unclassified_pulls)} were not classified (file lookup cap reached)."
        )
    else:
        lines.append("- None touching the static specification.")
    if elsewhere_pulls:
        lines.append(
            f"- Plus {len(elsewhere_pulls)} merged pull request(s) elsewhere in the repository."
        )
    if unclassified_pulls and static_pulls:
        lines.append(
            f"- Plus {len(unclassified_pulls)} merged pull request(s) not classified "
            "(file lookup cap reached); raise `--max-pull-file-lookups` to cover them."
        )
    if state.get("pullPageCapReached"):
        lines.append(
            "- The pull request scan stopped at `--max-pull-pages`; merged pull "
            "requests older than the ones listed may be missing."
        )
    lines.append("")

    if canonical["newerReleases"]:
        lines.extend(["## Canonical validator releases since the baseline", ""])
        for release in canonical["newerReleases"]:
            lines.append(
                f"- [{release['version']}](https://github.com/{canonical['repository']}"
                f"/releases/tag/{release['version']}) published {release['publishedAt'][:10]}"
            )
        lines.append("")

    lines.extend(["## Findings", ""])
    if not findings:
        lines.append("No field, enum, or notice differences beyond the acknowledged baseline.")
        lines.append("")
    for key, shape, title in FINDING_CATEGORIES:
        items = findings.get(key)
        if not items:
            continue
        lines.extend([f"### {title}", ""])
        if shape == "list":
            lines.extend(f"- `{item}`" for item in items)
        elif shape == "map":
            for holder, names in sorted(items.items()):
                lines.append(f"- `{holder}`: " + ", ".join(f"`{name}`" for name in names))
        else:
            for holder, values in sorted(items.items()):
                lines.append(f"- `{holder}`: " + ", ".join(str(value) for value in values))
        lines.append("")

    lines.extend(
        [
            "## Next steps",
            "",
            "1. Read the specification diff and the releases above.",
            "2. Implement the fields, enum values, or rules gtfs.guru is missing.",
            "3. Move the baseline once the gap is closed or consciously accepted:",
            "   `scripts/spec_watch.py update-baseline` (see `docs/spec-watch.md`).",
            "",
            f"<!-- {LINEAR_MARKER} fingerprint={state['fingerprint']} -->",
        ]
    )
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------
# Linear


def linear_request(api_key: str, query: str, variables: dict) -> dict:
    body = json.dumps({"query": query, "variables": variables}).encode("utf-8")
    request = urllib.request.Request(LINEAR_API, data=body, method="POST")
    request.add_header("Content-Type", "application/json")
    request.add_header("User-Agent", USER_AGENT)
    # Personal API keys go in unprefixed; OAuth access tokens keep their prefix.
    request.add_header(
        "Authorization", api_key if not api_key.startswith("lin_oauth") else f"Bearer {api_key}"
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            payload = json.loads(response.read())
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:400]
        raise WatchError(f"Linear API returned HTTP {error.code}: {detail}") from error
    except urllib.error.URLError as error:
        raise WatchError(f"Linear API unreachable: {error.reason}") from error
    if payload.get("errors"):
        raise WatchError(f"Linear API error: {json.dumps(payload['errors'])[:400]}")
    return payload["data"]


TEAM_QUERY = """
query($key: String!) {
  teams(filter: { key: { eq: $key } }) {
    nodes { id key name }
  }
}
"""

ISSUE_QUERY = """
query($teamId: ID!, $title: String!) {
  issues(filter: { team: { id: { eq: $teamId } }, title: { eq: $title } }, first: 50) {
    nodes { id identifier url title description updatedAt state { name type } }
  }
}
"""

ISSUE_CREATE = """
mutation($input: IssueCreateInput!) {
  issueCreate(input: $input) { success issue { id identifier url } }
}
"""

ISSUE_UPDATE = """
mutation($id: String!, $input: IssueUpdateInput!) {
  issueUpdate(id: $id, input: $input) { success issue { id identifier url } }
}
"""

COMMENT_CREATE = """
mutation($input: CommentCreateInput!) {
  commentCreate(input: $input) { success comment { id } }
}
"""

CLOSED_STATE_TYPES = {"completed", "canceled"}


def existing_fingerprint(description: str | None) -> str | None:
    if not description:
        return None
    match = re.search(rf"{LINEAR_MARKER} fingerprint=([0-9a-f]+)", description)
    return match.group(1) if match else None


def sync_linear(
    api_key: str, team_key: str, body: str, print_fingerprint: str, dry_run: bool
) -> str:
    """Create or update the single open spec-watch issue for the team.

    Identity is the issue title plus a fingerprint marker in the description: an
    unchanged fingerprint means an unchanged situation, so the run leaves the
    issue alone instead of stacking duplicates.
    """
    if dry_run:
        print(
            f"[dry-run] would sync Linear team {team_key}: title={LINEAR_ISSUE_TITLE!r} "
            f"fingerprint={print_fingerprint} body={len(body)} bytes"
        )
        return "dry-run"

    teams = linear_request(api_key, TEAM_QUERY, {"key": team_key})["teams"]["nodes"]
    if not teams:
        raise WatchError(f"no Linear team with key {team_key} is visible to this API key")
    team_id = teams[0]["id"]

    issues = linear_request(
        api_key, ISSUE_QUERY, {"teamId": team_id, "title": LINEAR_ISSUE_TITLE}
    )["issues"]["nodes"]
    open_issues = [
        issue
        for issue in issues
        if (issue.get("state") or {}).get("type") not in CLOSED_STATE_TYPES
    ]
    open_issues.sort(key=lambda issue: issue["updatedAt"], reverse=True)

    if not open_issues:
        data = linear_request(
            api_key,
            ISSUE_CREATE,
            {"input": {"teamId": team_id, "title": LINEAR_ISSUE_TITLE, "description": body}},
        )["issueCreate"]
        if not data["success"]:
            raise WatchError("Linear refused to create the issue")
        issue = data["issue"]
        print(f"created Linear issue {issue['identifier']} {issue['url']}")
        return "created"

    issue = open_issues[0]
    if existing_fingerprint(issue.get("description")) == print_fingerprint:
        print(f"Linear issue {issue['identifier']} already reports this drift; left untouched")
        return "unchanged"

    updated = linear_request(
        api_key, ISSUE_UPDATE, {"id": issue["id"], "input": {"description": body}}
    )["issueUpdate"]
    if not updated["success"]:
        raise WatchError(f"Linear refused to update issue {issue['identifier']}")
    linear_request(
        api_key,
        COMMENT_CREATE,
        {
            "input": {
                "issueId": issue["id"],
                "body": (
                    "Upstream moved again; the description above now reflects "
                    f"fingerprint `{print_fingerprint}`."
                ),
            }
        },
    )
    print(f"updated Linear issue {issue['identifier']} {issue['url']}")
    return "updated"


# ---------------------------------------------------------------------------
# Orchestration


def gather(args: argparse.Namespace, baseline: dict) -> dict:
    token = args.github_token or os.environ.get("GITHUB_TOKEN") or None
    surface = load_surface(args.surface)

    if args.spec_head_file:
        spec = json.loads(args.spec_head_file.read_text(encoding="utf-8"))
    else:
        spec = fetch_spec_head(baseline, token)

    if args.spec_file:
        spec_text = args.spec_file.read_text(encoding="utf-8")
    else:
        spec_text = fetch_spec_text(spec, token)

    if args.pulls_file:
        pulls = json.loads(args.pulls_file.read_text(encoding="utf-8"))
        pull_page_cap_reached = False
    else:
        pulls, pull_page_cap_reached = fetch_merged_pulls(
            baseline, token, args.max_pull_pages, args.max_pull_file_lookups
        )

    if args.release_file:
        canonical = json.loads(args.release_file.read_text(encoding="utf-8"))
    else:
        canonical = fetch_canonical_release(baseline, token)

    if args.rules_file:
        rules = json.loads(args.rules_file.read_text(encoding="utf-8"))
    else:
        # Release assets are public and served from a CDN host; the token is
        # not needed and must not be offered to it.
        rules = json.loads(http_get(canonical["rulesUrl"]))

    spec_files = parse_spec_reference(spec_text)
    if not spec_files:
        raise WatchError("the specification reference yielded no file sections; parser out of date")
    canonical_codes = canonical_notice_codes(rules)
    if not canonical_codes:
        raise WatchError("the canonical rules.json yielded no notice codes")

    all_findings = compare(surface, spec_files, canonical_codes)
    findings = subtract_acknowledged(all_findings, baseline.get("acknowledged") or {})

    reasons = []
    if findings:
        reasons.append(
            f"{count_findings(findings)} field, enum, or notice difference(s) are not in the baseline"
        )
    if spec["commit"] != baseline["specRevision"]["commit"]:
        reasons.append(
            f"the specification reference moved to `{short(spec['commit'])}` "
            f"({spec['committedAt'][:10]}): {spec['message']}"
        )
    if canonical["version"] != baseline["canonicalBaseline"]["version"]:
        reasons.append(
            f"the canonical validator released {canonical['version']} "
            f"({canonical['publishedAt'][:10]})"
        )

    state = {
        "baseline": baseline,
        "validatorVersion": surface["validatorVersion"],
        "spec": spec,
        "pulls": pulls,
        "pullPageCapReached": pull_page_cap_reached,
        "canonical": canonical,
        "allFindings": all_findings,
        "findings": findings,
        "reasons": reasons,
    }
    state["fingerprint"] = fingerprint(state)
    return state


def run_check(args: argparse.Namespace) -> int:
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    state = gather(args, baseline)

    if not state["reasons"]:
        print(
            "no drift: specification at "
            f"{short(state['spec']['commit'])} and canonical validator at "
            f"{state['canonical']['version']} match the baseline"
        )
        return 0

    body = render_markdown(state)
    args.report_dir.mkdir(parents=True, exist_ok=True)
    markdown_path = args.report_dir / "spec-drift.md"
    json_path = args.report_dir / "spec-drift.json"
    markdown_path.write_text(body, encoding="utf-8")
    json_path.write_text(
        json.dumps(
            {
                "fingerprint": state["fingerprint"],
                "reasons": state["reasons"],
                "baseline": {
                    "specRevision": baseline["specRevision"],
                    "canonicalBaseline": baseline["canonicalBaseline"],
                },
                "upstream": {"specRevision": state["spec"], "canonicalBaseline": state["canonical"]},
                "mergedPullRequests": state["pulls"],
                "mergedPullRequestsTruncated": state["pullPageCapReached"],
                "validatorVersion": state["validatorVersion"],
                "findings": state["findings"],
                "allDifferences": state["allFindings"],
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"drift detected (fingerprint {state['fingerprint']}):")
    for reason in state["reasons"]:
        print(f"  - {reason}")
    print(f"wrote {markdown_path.relative_to(ROOT) if markdown_path.is_relative_to(ROOT) else markdown_path}")
    print(f"wrote {json_path.relative_to(ROOT) if json_path.is_relative_to(ROOT) else json_path}")

    if args.state_file:
        previous = None
        if args.state_file.exists():
            previous = json.loads(args.state_file.read_text(encoding="utf-8")).get("fingerprint")
        if previous == state["fingerprint"]:
            print("unchanged since the last run; skipping the issue tracker")
            return 3 if args.fail_on_drift else 0
        args.state_file.parent.mkdir(parents=True, exist_ok=True)
        args.state_file.write_text(
            json.dumps({"fingerprint": state["fingerprint"]}, indent=2) + "\n", encoding="utf-8"
        )

    api_key = args.linear_api_key or os.environ.get("LINEAR_API_KEY") or ""
    if args.linear_dry_run:
        sync_linear(api_key, args.linear_team_key, body, state["fingerprint"], dry_run=True)
    elif api_key:
        sync_linear(api_key, args.linear_team_key, body, state["fingerprint"], dry_run=False)
    else:
        print("LINEAR_API_KEY is unset; skipping the issue tracker")

    return 3 if args.fail_on_drift else 0


def run_update_baseline(args: argparse.Namespace) -> int:
    baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
    state = gather(args, baseline)
    spec = state["spec"]
    canonical = state["canonical"]

    updated = dict(baseline)
    updated["specRevision"] = {
        "repository": spec["repository"],
        "ref": spec["ref"],
        "commit": spec["commit"],
        "committedAt": spec["committedAt"],
        "specPaths": spec["specPaths"],
    }
    updated["canonicalBaseline"] = {
        "repository": canonical["repository"],
        "version": canonical["version"],
        "publishedAt": canonical["publishedAt"],
        "rulesAsset": canonical["rulesAsset"],
    }
    acknowledged = acknowledged_shape()
    for key, shape, _ in FINDING_CATEGORIES:
        value = state["allFindings"][key]
        if shape == "list":
            acknowledged[key] = sorted(value)
        else:
            acknowledged[key] = {holder: sorted(items) for holder, items in sorted(value.items())}
    updated["acknowledged"] = acknowledged
    updated["updatedAt"] = args.updated_at or datetime.now(timezone.utc).strftime(
        "%Y-%m-%dT%H:%M:%SZ"
    )

    args.baseline.write_text(json.dumps(updated, indent=2) + "\n", encoding="utf-8")
    print(
        f"baseline now at {spec['repository']}@{short(spec['commit'])} and "
        f"{canonical['repository']}@{canonical['version']}, acknowledging "
        f"{count_findings(state['allFindings'])} known difference(s)"
    )
    print("rebuild so reports quote the new baseline: cargo build --release -p gtfs-guru")
    return 0


def add_shared_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--baseline",
        type=pathlib.Path,
        default=DEFAULT_BASELINE,
        help="baseline document to compare against (default: %(default)s)",
    )
    parser.add_argument(
        "--surface",
        type=pathlib.Path,
        help="precomputed `gtfs-guru spec-surface` JSON; runs the CLI when omitted",
    )
    parser.add_argument("--github-token", help="defaults to $GITHUB_TOKEN")
    parser.add_argument(
        "--max-pull-pages",
        type=int,
        default=3,
        help="pages of closed pull requests to scan (default: %(default)s)",
    )
    parser.add_argument(
        "--max-pull-file-lookups",
        type=int,
        default=40,
        help="merged pull requests to classify by touched files (default: %(default)s)",
    )
    fixtures = parser.add_argument_group(
        "offline fixtures", "replace a network fetch with a local file, for tests"
    )
    fixtures.add_argument("--spec-file", type=pathlib.Path, help="specification reference markdown")
    fixtures.add_argument("--spec-head-file", type=pathlib.Path, help="spec head commit JSON")
    fixtures.add_argument("--pulls-file", type=pathlib.Path, help="merged pull request list JSON")
    fixtures.add_argument("--release-file", type=pathlib.Path, help="canonical release JSON")
    fixtures.add_argument("--rules-file", type=pathlib.Path, help="canonical rules.json")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    subparsers = parser.add_subparsers(dest="command", required=True)

    check = subparsers.add_parser("check", help="report drift against the baseline")
    add_shared_arguments(check)
    check.add_argument(
        "--report-dir",
        type=pathlib.Path,
        default=DEFAULT_REPORT_DIR,
        help="where drift reports are written, only on drift (default: %(default)s)",
    )
    check.add_argument(
        "--state-file",
        type=pathlib.Path,
        help="remember the last fingerprint here to stay quiet across local reruns",
    )
    check.add_argument("--linear-api-key", help="defaults to $LINEAR_API_KEY")
    check.add_argument(
        "--linear-team-key", default="GTF", help="Linear team key (default: %(default)s)"
    )
    check.add_argument(
        "--linear-dry-run",
        action="store_true",
        help="print what would be sent to Linear instead of sending it",
    )
    check.add_argument(
        "--fail-on-drift",
        action="store_true",
        help="exit 3 when drift is found, for local use and tests",
    )
    check.set_defaults(func=run_check)

    update = subparsers.add_parser(
        "update-baseline", help="accept the current upstream state as the baseline"
    )
    add_shared_arguments(update)
    update.add_argument("--updated-at", help="timestamp to record; defaults to now, in UTC")
    update.set_defaults(func=run_update_baseline)

    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except WatchError as error:
        print(f"spec_watch: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
