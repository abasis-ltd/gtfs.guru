ERROR: location_without_parent_station
Description: Entrance (location_type=2), generic node (location_type=3), or boarding area (location_type=4) must have parent_station field.
In this test case, stops.txt has entrance1 with location_type=2 but no parent_station.
Expected error: location_without_parent_station for stops.txt
