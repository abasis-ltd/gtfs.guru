#!/usr/bin/env python3
"""Runtime smoke test for an installed gtfs-guru wheel."""

import asyncio
import json
import os
import tempfile
from pathlib import Path

import gtfs_guru


def main():
    expected_version = os.environ.get("EXPECTED_VERSION", "")
    if expected_version.startswith("v"):
        expected_version = expected_version[1:]
    if expected_version:
        assert gtfs_guru.version() == expected_version
    assert gtfs_guru.__version__ == gtfs_guru.version()

    codes = gtfs_guru.notice_codes()
    schema = gtfs_guru.notice_schema()
    assert isinstance(codes, list) and codes
    assert isinstance(schema, dict)
    assert set(codes) == set(schema)
    assert "missing_required_file" in codes
    assert "code_a" not in codes and "code_b" not in codes

    with tempfile.TemporaryDirectory() as feed_dir, tempfile.TemporaryDirectory() as report_dir:
        result = gtfs_guru.validate(feed_dir)
        assert isinstance(result, gtfs_guru.ValidationResult)
        assert result.error_count > 0
        assert not result.is_valid
        assert len(result.errors()) == result.error_count
        assert len(result.warnings()) == result.warning_count
        assert len(result.infos()) == result.info_count
        assert result.by_code("missing_required_file")

        notice = result.notices[0]
        assert isinstance(notice, gtfs_guru.Notice)
        assert isinstance(notice.context(), dict)
        assert notice.get("definitely_missing") is None

        payload = result.to_dict()
        assert isinstance(payload, dict)
        assert json.loads(result.to_json()) == payload

        json_path = Path(report_dir) / "report.json"
        html_path = Path(report_dir) / "report.html"
        result.save_json(str(json_path))
        result.save_html(str(html_path))
        assert json.loads(json_path.read_text(encoding="utf-8")) == payload
        assert "GTFS Validation Report" in html_path.read_text(encoding="utf-8")

        progress_stages = []

        def on_progress(info):
            assert isinstance(info, gtfs_guru.ProgressInfo)
            progress_stages.append(info.stage)

        async def validate_async():
            async_result = await gtfs_guru.validate_async(
                feed_dir,
                on_progress=on_progress,
            )
            assert isinstance(async_result, gtfs_guru.ValidationResult)
            assert async_result.error_count == result.error_count

        asyncio.run(validate_async())
        assert progress_stages == ["loading", "validating", "finalizing", "complete"]

    print(
        "Python wheel smoke test passed:",
        gtfs_guru.version(),
        f"({len(codes)} notice codes)",
    )


if __name__ == "__main__":
    main()
