#!/usr/bin/env python3
"""Refresh the notice metadata snapshot used by GTFS Guru.

MobilityData publishes the canonical validator metadata as JSON.  The snapshot
is committed so normal Rust builds remain hermetic.  GTFS Guru-only notices are
defined below and merged into the same schema shape.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import urllib.request


ROOT = pathlib.Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "crates" / "gtfs_validator_core" / "notice_metadata.json"
DEFAULT_SOURCE = "https://gtfs-validator.mobilitydata.org/rules.json"


def references(*files: str, urls: tuple[tuple[str, str], ...] = ()) -> dict:
    return {
        "fileReferences": list(files),
        "bestPracticesFileReferences": [],
        "sectionReferences": [],
        "urlReferences": [{"label": label, "url": url} for label, url in urls],
    }


GTFS_GURU_NOTICES = {
    "agency_phone_invalid": {
        "shortSummary": "An agency phone number appears to be invalid.",
        "description": (
            "The `agency_phone` value does not look like a usable public phone "
            "number. This optional Google compatibility rule is enabled with "
            "`--google-rules`."
        ),
        "references": references("agency.txt"),
    },
    "duplicate_trip": {
        "shortSummary": "Two or more trips have the same route, service, and stop times.",
        "description": (
            "Trips with identical route, service, and stop-time signatures duplicate "
            "the same scheduled journey. This optional Google compatibility rule is "
            "enabled with `--google-rules`."
        ),
        "references": references("trips.txt", "stop_times.txt"),
    },
    "google_ic_price_check": {
        "shortSummary": "The Google Transit `ic_price` extension has an invalid value.",
        "description": (
            "The `ic_price` value must be `-1` or a non-negative discounted fare. "
            "This optional compatibility rule is enabled with `--google-rules`."
        ),
        "references": references("fare_attributes.txt"),
    },
    "google_transfer_type_check": {
        "shortSummary": "Google Transit ignores this transfer type.",
        "description": (
            "Google Transit ignores `transfer_type` values 4 and 5. This is a "
            "consumer-compatibility warning rather than a GTFS specification violation."
        ),
        "references": references("transfers.txt"),
    },
    "headway_too_large": {
        "shortSummary": "A frequency headway is longer than one hour.",
        "description": (
            "A `frequencies.txt` row has `headway_secs` greater than 3600. Confirm "
            "that the value is expressed in seconds and that frequency-based service "
            "is the intended model."
        ),
        "references": references("frequencies.txt"),
    },
    "service_never_active": {
        "shortSummary": "A calendar service is never active.",
        "description": (
            "Every weekday flag is zero and `calendar_dates.txt` does not add any "
            "service dates for this `service_id`, so trips using it never run."
        ),
        "references": references("calendar.txt", "calendar_dates.txt"),
    },
    "stop_headsign_invalid_char": {
        "shortSummary": "A stop headsign contains a consumer-incompatible character.",
        "description": (
            "The `stop_headsign` contains one of `!`, `$`, `%`, `\\`, `*`, `=`, or "
            "`_`. This optional Google compatibility rule is enabled with "
            "`--google-rules`."
        ),
        "references": references("stop_times.txt"),
    },
    "too_many_days_without_service": {
        "shortSummary": "The feed has a gap of at least 14 days without service.",
        "description": (
            "No active service was found between two dates separated by 14 days or "
            "more. Confirm that calendar coverage is continuous for the publication "
            "period."
        ),
        "references": references(
            "calendar.txt",
            "calendar_dates.txt",
            urls=(
                (
                    "Google Transit static feed warnings",
                    "https://developers.google.com/transit/gtfs/guides/"
                    "static-errors-warnings#too_many_days_without_service_1",
                ),
            ),
        ),
    },
    "unused_agency": {
        "shortSummary": "An agency is not referenced by any route.",
        "description": (
            "In a multi-agency feed, this `agency.txt` row is not referenced by "
            "`routes.agency_id`. The thorough validator mode reports this warning."
        ),
        "references": references("agency.txt", "routes.txt"),
    },
    "unused_route": {
        "shortSummary": "A route has no trips.",
        "description": (
            "The `route_id` exists in `routes.txt` but is not referenced by any "
            "`trips.txt` row. The thorough validator mode reports this warning."
        ),
        "references": references("routes.txt", "trips.txt"),
    },
    "unused_stop": {
        "shortSummary": "A stop is not used by any trip.",
        "description": (
            "The stop is not referenced by `stop_times.txt` and is not an ancestor "
            "of a referenced stop. The thorough validator mode reports this warning."
        ),
        "references": references("stops.txt", "stop_times.txt"),
    },
}


def load_source(source: str) -> dict:
    path = pathlib.Path(source)
    if path.exists():
        return json.loads(path.read_text(encoding="utf-8"))
    request = urllib.request.Request(
        source,
        headers={"User-Agent": "gtfs-guru-metadata-updater/1.0"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def normalized_entry(entry: dict) -> dict:
    result = {
        key: entry[key]
        for key in (
            "shortSummary",
            "description",
            "references",
            "properties",
            "deprecated",
            "deprecationReason",
            "deprecationVersion",
            "replacementNoticeCodes",
        )
        if key in entry
    }
    if not result.get("description") and result.get("shortSummary"):
        result["description"] = result["shortSummary"]
    return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--source",
        default=DEFAULT_SOURCE,
        help="rules.json URL or local path",
    )
    args = parser.parse_args()

    metadata = {
        code: normalized_entry(entry)
        for code, entry in load_source(args.source).items()
    }
    for code, override in GTFS_GURU_NOTICES.items():
        metadata[code] = {
            **override,
            "properties": {},
            "deprecated": False,
        }

    OUTPUT.write_text(
        json.dumps(metadata, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"Wrote {len(metadata)} notice metadata entries to {OUTPUT}")


if __name__ == "__main__":
    main()
