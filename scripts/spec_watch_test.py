#!/usr/bin/env python3
"""Offline tests for `scripts/spec_watch.py`.

The fixtures in `spec_watch_fixtures/` are real snapshots -- two sections of the
GTFS reference, the matching slice of `gtfs-guru spec-surface`, and three notices
from the canonical validator's `rules.json` -- trimmed to a size a test can
reason about. Each test copies them, injects a change the watcher is supposed to
notice, and checks that it does.

Run with `python3 scripts/spec_watch_test.py` (or any unittest runner). No
network, no cargo build.
"""

from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "spec_watch.py"
FIXTURES = ROOT / "scripts" / "spec_watch_fixtures"

NEW_FIELD_ROW = (
    "|  `agency_test_field` | Text | Optional | A field the specification gained "
    "after the baseline. |"
)
NEW_ENUM_OPTION = "<br>`5` - **Test Location**. A location type added after the baseline."
NEW_NOTICE_CODE = "test_injected_notice"


class SpecWatchCase(unittest.TestCase):
    def setUp(self) -> None:
        self.workdir = pathlib.Path(tempfile.mkdtemp(prefix="spec-watch-test-"))
        self.addCleanup(shutil.rmtree, self.workdir, True)
        for name in (
            "baseline.json",
            "surface.json",
            "reference.md",
            "spec_head.json",
            "pulls.json",
            "release.json",
            "rules.json",
        ):
            shutil.copy(FIXTURES / name, self.workdir / name)
        self.reports = self.workdir / "reports"

    # -- fixture mutation -------------------------------------------------

    def add_spec_field(self) -> None:
        """A new column in the reference's agency.txt table."""
        path = self.workdir / "reference.md"
        lines = path.read_text(encoding="utf-8").splitlines()
        last_agency_row = max(
            index for index, line in enumerate(lines) if line.startswith("|  `agency_")
        )
        lines.insert(last_agency_row + 1, NEW_FIELD_ROW)
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def add_spec_enum_value(self) -> None:
        """A new option in the reference's stops.location_type domain."""
        path = self.workdir / "reference.md"
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()
        for index, line in enumerate(lines):
            if line.startswith("|  `location_type`"):
                head, _, tail = line.rpartition("|")
                lines[index] = f"{head}{NEW_ENUM_OPTION}{tail}"
                break
        else:  # pragma: no cover - guards the fixture, not the watcher
            self.fail("the reference fixture no longer has a location_type row")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")

    def add_canonical_notice(self) -> None:
        """A new rule in the canonical validator's published notice list."""
        path = self.workdir / "rules.json"
        rules = json.loads(path.read_text(encoding="utf-8"))
        rules[NEW_NOTICE_CODE] = {
            "code": NEW_NOTICE_CODE,
            "severityLevel": "ERROR",
            "type": "object",
            "shortSummary": "A rule the canonical validator gained after the baseline.",
            "properties": {},
            "deprecated": False,
        }
        path.write_text(json.dumps(rules, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    # -- running ----------------------------------------------------------

    def check(self, *extra: str) -> subprocess.CompletedProcess[str]:
        cmd = [
            sys.executable,
            str(SCRIPT),
            "check",
            "--baseline",
            str(self.workdir / "baseline.json"),
            "--surface",
            str(self.workdir / "surface.json"),
            "--spec-file",
            str(self.workdir / "reference.md"),
            "--spec-head-file",
            str(self.workdir / "spec_head.json"),
            "--pulls-file",
            str(self.workdir / "pulls.json"),
            "--release-file",
            str(self.workdir / "release.json"),
            "--rules-file",
            str(self.workdir / "rules.json"),
            "--report-dir",
            str(self.reports),
            *extra,
        ]
        return subprocess.run(cmd, capture_output=True, text=True, env={"PATH": "/usr/bin:/bin"})

    def report(self) -> dict:
        return json.loads((self.reports / "spec-drift.json").read_text(encoding="utf-8"))

    # -- tests ------------------------------------------------------------

    def test_aligned_fixtures_stay_quiet(self) -> None:
        result = self.check()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no drift", result.stdout)
        self.assertFalse(
            self.reports.exists(), "a run with no drift must not write a report directory"
        )

    def test_detects_a_new_specification_field(self) -> None:
        self.add_spec_field()

        result = self.check("--fail-on-drift")

        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertEqual(
            self.report()["findings"]["specFieldsNotSupported"],
            {"agency.txt": ["agency_test_field"]},
        )
        markdown = (self.reports / "spec-drift.md").read_text(encoding="utf-8")
        self.assertIn("agency_test_field", markdown)

    def test_detects_a_new_specification_enum_value(self) -> None:
        self.add_spec_enum_value()

        result = self.check("--fail-on-drift")

        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertEqual(
            self.report()["findings"]["enumValuesNotSupported"],
            {"stops.txt:location_type": [5]},
        )

    def test_detects_a_new_canonical_rule(self) -> None:
        self.add_canonical_notice()

        result = self.check("--fail-on-drift")

        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertEqual(
            self.report()["findings"]["canonicalNoticesNotImplemented"], [NEW_NOTICE_CODE]
        )

    def test_reports_a_moved_specification_revision(self) -> None:
        head = json.loads((self.workdir / "spec_head.json").read_text(encoding="utf-8"))
        head["commit"] = "0" * 40
        head["committedAt"] = "2026-09-01T00:00:00Z"
        head["message"] = "Add a field after the baseline"
        (self.workdir / "spec_head.json").write_text(json.dumps(head) + "\n", encoding="utf-8")

        result = self.check("--fail-on-drift")

        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        report = self.report()
        self.assertEqual(report["findings"], {})
        self.assertTrue(
            any("reference moved" in reason for reason in report["reasons"]), report["reasons"]
        )

    def test_reports_a_new_canonical_release(self) -> None:
        release = json.loads((self.workdir / "release.json").read_text(encoding="utf-8"))
        release["version"] = "v8.1.0"
        release["publishedAt"] = "2026-09-01T00:00:00Z"
        (self.workdir / "release.json").write_text(json.dumps(release) + "\n", encoding="utf-8")

        result = self.check("--fail-on-drift")

        self.assertEqual(result.returncode, 3, result.stdout + result.stderr)
        self.assertTrue(
            any("released v8.1.0" in reason for reason in self.report()["reasons"]),
            self.report()["reasons"],
        )

    def test_a_second_run_repeats_itself_instead_of_duplicating(self) -> None:
        self.add_spec_field()
        self.add_canonical_notice()
        state = self.workdir / "state.json"

        first = self.check("--state-file", str(state))
        first_markdown = (self.reports / "spec-drift.md").read_text(encoding="utf-8")
        first_json = (self.reports / "spec-drift.json").read_text(encoding="utf-8")

        second = self.check("--state-file", str(state))

        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertNotIn("unchanged since the last run", first.stdout)
        self.assertIn("unchanged since the last run", second.stdout)
        self.assertEqual(
            first_markdown, (self.reports / "spec-drift.md").read_text(encoding="utf-8")
        )
        self.assertEqual(first_json, (self.reports / "spec-drift.json").read_text(encoding="utf-8"))
        # The same drift must fingerprint the same way, or issue dedup breaks.
        self.assertEqual(
            json.loads(state.read_text(encoding="utf-8"))["fingerprint"],
            self.report()["fingerprint"],
        )

    def test_an_acknowledged_difference_stays_quiet(self) -> None:
        self.add_spec_field()
        baseline_path = self.workdir / "baseline.json"
        baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
        baseline["acknowledged"]["specFieldsNotSupported"] = {"agency.txt": ["agency_test_field"]}
        baseline_path.write_text(json.dumps(baseline, indent=2) + "\n", encoding="utf-8")

        result = self.check()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("no drift", result.stdout)
        self.assertFalse(self.reports.exists())

    def test_unclassified_pulls_are_not_reported_as_none(self) -> None:
        """The report must not claim "None" while classification is pending.

        Pull requests past `--max-pull-file-lookups` carry `touchesStaticSpec:
        null`. Reading that as "does not touch the static spec" turns a cap into
        a false all-clear.
        """
        pulls = json.loads((self.workdir / "pulls.json").read_text(encoding="utf-8"))
        for pull in pulls:
            pull["touchesStaticSpec"] = None
        (self.workdir / "pulls.json").write_text(json.dumps(pulls), encoding="utf-8")
        # Move the revision so a report is written at all; the findings are
        # beside the point here.
        head = json.loads((self.workdir / "spec_head.json").read_text(encoding="utf-8"))
        head["commit"] = "0" * 40
        head["committedAt"] = "2026-09-01T00:00:00Z"
        (self.workdir / "spec_head.json").write_text(json.dumps(head) + "\n", encoding="utf-8")

        result = self.check()

        self.assertEqual(result.returncode, 0, result.stderr)
        body = (self.reports / "spec-drift.md").read_text(encoding="utf-8")
        self.assertNotIn("None touching the static specification.", body)
        self.assertIn("were not classified", body)

    def test_linear_dry_run_describes_the_payload_without_sending_it(self) -> None:
        self.add_spec_field()

        result = self.check("--linear-dry-run", "--linear-team-key", "GTF")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[dry-run] would sync Linear team GTF", result.stdout)


class RedirectHandlerCase(unittest.TestCase):
    """`Authorization` must not follow a redirect off the host it was sent to.

    GitHub redirects release-asset downloads to a CDN host; `urllib` would
    otherwise carry the token along.
    """

    @classmethod
    def setUpClass(cls) -> None:
        sys.path.insert(0, str(ROOT / "scripts"))
        import spec_watch  # noqa: PLC0415 - deliberate late import

        cls.spec_watch = spec_watch

    def redirected(self, from_url: str, to_url: str):
        import urllib.request  # noqa: PLC0415 - deliberate late import

        request = urllib.request.Request(from_url)
        request.add_header("Authorization", "Bearer secret-token")
        request.add_header("User-Agent", "gtfs-guru-spec-watch")
        handler = self.spec_watch._StripAuthOnCrossHostRedirect()
        return handler.redirect_request(request, None, 302, "Found", {}, to_url)

    def test_authorization_is_dropped_when_the_host_changes(self) -> None:
        new_request = self.redirected(
            "https://github.com/o/r/releases/download/v1/rules.json",
            "https://objects.githubusercontent.com/blob/abc",
        )
        self.assertIsNone(new_request.get_header("Authorization"))
        self.assertEqual(new_request.get_header("User-agent"), "gtfs-guru-spec-watch")

    def test_authorization_survives_a_same_host_redirect(self) -> None:
        new_request = self.redirected(
            "https://api.github.com/repos/o/r/releases",
            "https://api.github.com/repositories/1/releases",
        )
        self.assertEqual(new_request.get_header("Authorization"), "Bearer secret-token")


class SpecParserCase(unittest.TestCase):
    """The reference parser, exercised against the committed snapshot."""

    @classmethod
    def setUpClass(cls) -> None:
        sys.path.insert(0, str(ROOT / "scripts"))
        import spec_watch  # noqa: PLC0415 - deliberate late import

        cls.spec_watch = spec_watch
        cls.spec = spec_watch.parse_spec_reference(
            (FIXTURES / "reference.md").read_text(encoding="utf-8")
        )

    def test_reads_fields_presence_and_enum_domains(self) -> None:
        agency = self.spec["agency.txt"]["fields"]
        self.assertEqual(agency["agency_name"]["presence"], "Required")
        self.assertEqual(agency["agency_id"]["presence"], "ConditionallyRequired")
        self.assertEqual(agency["cemv_support"]["enumValues"], [0, 1, 2])

        stops = self.spec["stops.txt"]["fields"]
        self.assertEqual(stops["location_type"]["enumValues"], [0, 1, 2, 3, 4])
        self.assertEqual(stops["stop_access"]["enumValues"], [0, 1])
        self.assertEqual(stops["stop_lat"]["presence"], "ConditionallyRequired")
        # Referenced enum values in prose must not be read as options.
        self.assertEqual(stops["parent_station"]["enumValues"], [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
