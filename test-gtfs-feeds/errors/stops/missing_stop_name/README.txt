ERROR: missing_stop_name
Description: stop_name is required for location_type equal to 0 (stop), 1 (station), or 2 (entrance).
In this test case, stops.txt has stop1 with location_type=0 but empty stop_name.
Expected error: missing_stop_name for stops.txt
