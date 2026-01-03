ERROR: station_with_parent_station
Description: Field parent_station must be empty when location_type is 1 (station).
In this test case, stops.txt has station2 with location_type=1 and parent_station=station1.
Expected error: station_with_parent_station for stops.txt
