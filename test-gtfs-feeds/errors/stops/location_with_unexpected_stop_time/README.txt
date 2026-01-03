ERROR: location_with_unexpected_stop_time
Description: Referenced locations in stop_times.txt must be stops/platforms (location_type=0), not stations or entrances.
In this test case, stop_times.txt references station1 which is a station (location_type=1).
Expected error: location_with_unexpected_stop_time for stop_times.txt
