ERROR: wrong_parent_location_type
Description: Incorrect type of the parent location. A stop (location_type=0) should have a station (location_type=1) as parent, not another stop.
In this test case, stops.txt has stop2 with location_type=0 and parent_station=stop1 which is also a stop (location_type=0).
Expected error: wrong_parent_location_type for stops.txt
