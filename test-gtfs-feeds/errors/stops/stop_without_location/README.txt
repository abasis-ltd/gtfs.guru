ERROR: stop_without_location
Description: stop_lat and/or stop_lon is missing for location_type 0 (stop), 1 (station), or 2 (entrance).
In this test case, stops.txt has stop1 with location_type=0 but missing stop_lat.
Expected error: stop_without_location for stops.txt
