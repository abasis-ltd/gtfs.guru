#!/usr/bin/env python3
"""Build the demo GTFS feed served by the website's "try an example" button.

The feed is a small, plausible two-route network with a handful of deliberate
mistakes, so a first-time visitor sees what a real report looks like instead of
an empty "no issues found" panel.

The archive is byte-for-byte reproducible: entries are written in a fixed order
with a fixed timestamp, so rebuilding it without editing the data leaves the
checked-in file unchanged.

Usage:
    python3 scripts/build_demo_feed.py [--check]

--check rebuilds into memory and fails if the checked-in copies are stale.
"""

from __future__ import annotations

import argparse
import sys
import zipfile
from io import BytesIO
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# The website lives in the repo twice: the root copy is what gets deployed, and
# the crate copy is embedded into the axum binary. Both need the demo feed.
OUTPUT_PATHS = (
    REPO_ROOT / "website" / "demo" / "gtfs-guru-demo.zip",
    REPO_ROOT / "crates" / "gtfs_validator_web" / "website" / "demo" / "gtfs-guru-demo.zip",
)

# Fixed DOS timestamp (1980-01-01 00:00:00), the earliest a zip can represent.
FIXED_DATE_TIME = (1980, 1, 1, 0, 0, 0)

# A service window wide enough that the demo does not start reporting an
# expired calendar a few months after it is generated.
SERVICE_START = "20260101"
SERVICE_END = "20351231"

FILES: dict[str, str] = {}

FILES["agency.txt"] = """\
agency_id,agency_name,agency_url,agency_timezone,agency_lang,agency_phone
riverside,Riverside Transit,https://example.com/riverside,America/New_York,en,+1-555-0100
"""

# stop3 has no stop_name (a required field), and stop6 repeats stop5's id.
FILES["stops.txt"] = """\
stop_id,stop_name,stop_desc,stop_lat,stop_lon,location_type,parent_station,wheelchair_boarding
stop1,Central Station,Main interchange,40.7128,-74.0060,0,,1
stop2,River Park,,40.7180,-74.0035,0,,1
stop3,,,40.7225,-74.0010,0,,0
stop4,Market Square,Market Square,40.7270,-73.9985,0,,0
stop5,Hillcrest Avenue,,40.7315,-73.9960,0,,0
stop5,Hillcrest Ave,,40.7316,-73.9959,0,,0
stop7,Airport Terminal,,40.7360,-73.9935,0,,1
stop8,Depot (not in service),,40.6900,-74.0400,0,,0
"""

# route2 uses the same colour for background and text, which no rider can read.
FILES["routes.txt"] = """\
route_id,agency_id,route_short_name,route_long_name,route_desc,route_type,route_color,route_text_color
route1,riverside,1,Central - Airport,Every 30 minutes,3,1E88E5,FFFFFF
route2,riverside,2,Market Loop,Weekdays only,3,FFDD00,FFEE00
"""

FILES["calendar.txt"] = f"""\
service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date
weekday,1,1,1,1,1,0,0,{SERVICE_START},{SERVICE_END}
weekend,0,0,0,0,0,1,1,{SERVICE_START},{SERVICE_END}
"""

FILES["calendar_dates.txt"] = """\
service_id,date,exception_type
weekday,20260704,2
weekend,20260704,1
"""

# trip4 references "holiday", a service_id that calendar.txt never defines.
FILES["trips.txt"] = """\
route_id,service_id,trip_id,trip_headsign,direction_id,block_id,shape_id,wheelchair_accessible
route1,weekday,trip1,Airport,0,block1,shape1,1
route1,weekday,trip2,Central Station,1,block1,shape1,1
route1,weekend,trip3,Airport,0,block2,shape1,1
route2,holiday,trip4,Market Loop,0,block3,shape2,0
route2,weekday,trip5,Market Loop,0,block3,shape2,0
"""

# trip2 departs stop2 before it arrives there, and trip5's second stop is ten
# minutes earlier than its first. Every other stop is served so that the only
# stop nothing calls at is stop8.
FILES["stop_times.txt"] = """\
trip_id,arrival_time,departure_time,stop_id,stop_sequence,pickup_type,drop_off_type,timepoint
trip1,08:00:00,08:00:00,stop1,1,0,0,1
trip1,08:04:00,08:04:00,stop2,2,0,0,0
trip1,08:08:00,08:08:00,stop3,3,0,0,0
trip1,08:12:00,08:12:00,stop4,4,0,0,0
trip1,08:17:00,08:17:00,stop5,5,0,0,0
trip1,08:22:00,08:22:00,stop7,6,0,0,1
trip2,09:00:00,09:00:00,stop7,1,0,0,1
trip2,09:05:00,09:05:00,stop5,2,0,0,0
trip2,09:09:00,09:09:00,stop4,3,0,0,0
trip2,09:13:00,09:13:00,stop3,4,0,0,0
trip2,09:18:00,09:15:00,stop2,5,0,0,0
trip2,09:25:00,09:25:00,stop1,6,0,0,1
trip3,10:30:00,10:30:00,stop1,1,0,0,1
trip3,10:45:00,10:45:00,stop4,2,0,0,0
trip3,10:58:00,10:58:00,stop7,3,0,0,1
trip4,12:00:00,12:00:00,stop1,1,0,0,1
trip4,12:12:00,12:12:00,stop4,2,0,0,1
trip5,17:00:00,17:00:00,stop1,1,0,0,1
trip5,16:50:00,16:50:00,stop4,2,0,0,1
"""

FILES["shapes.txt"] = """\
shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence,shape_dist_traveled
shape1,40.7128,-74.0060,1,0
shape1,40.7180,-74.0035,2,620
shape1,40.7270,-73.9985,3,1730
shape1,40.7360,-73.9935,4,2840
shape2,40.7128,-74.0060,1,0
shape2,40.7270,-73.9985,2,1730
"""

FILES["feed_info.txt"] = f"""\
feed_publisher_name,feed_publisher_url,feed_lang,feed_start_date,feed_end_date,feed_version,feed_contact_email
GTFS Guru demo,https://gtfs.guru/,en,{SERVICE_START},{SERVICE_END},demo-1,demo@example.com
"""

FILES["README.txt"] = """\
This is a demo GTFS feed for gtfs.guru. It is not real transit data.

It contains deliberate mistakes so the validator has something to report:

  * stops.txt   stop3 has no stop_name
  * stops.txt   stop5 appears twice with different names
  * stops.txt   stop8 is never served by any trip
  * stops.txt   stop4 repeats its name in stop_desc
  * trips.txt   trip4 uses a service_id that no calendar defines
  * stop_times  trip2 departs stop2 three minutes before it arrives
  * stop_times  trip5 reaches its second stop ten minutes before its first
  * trips.txt   trip2 is the return trip but reuses the outbound shape
  * routes.txt  route2 prints yellow text on a yellow background

Do not use this feed for anything other than trying out a validator.
"""


def build_archive() -> bytes:
    buffer = BytesIO()
    # No compression metadata varies between runs, and ZIP_DEFLATED output is
    # stable for a given zlib level, so the archive hashes the same every time.
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for name in sorted(FILES):
            info = zipfile.ZipInfo(name, date_time=FIXED_DATE_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            archive.writestr(info, FILES[name])
    return buffer.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the checked-in archives match this script instead of writing them",
    )
    args = parser.parse_args()

    archive = build_archive()

    if args.check:
        stale = [
            path
            for path in OUTPUT_PATHS
            if not path.exists() or path.read_bytes() != archive
        ]
        if stale:
            for path in stale:
                print(f"stale: {path.relative_to(REPO_ROOT)}", file=sys.stderr)
            print("Run: python3 scripts/build_demo_feed.py", file=sys.stderr)
            return 1
        print(f"Demo feed is up to date ({len(archive)} bytes)")
        return 0

    for path in OUTPUT_PATHS:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(archive)
        print(f"wrote {path.relative_to(REPO_ROOT)} ({len(archive)} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
