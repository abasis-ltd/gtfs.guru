ERROR: stop_time_with_only_arrival_or_departure_time
Description: Missing arrival_time or departure_time - if one is provided, both should be.
In this test case, stop_times.txt has stop2 with arrival_time but no departure_time.
Expected error: stop_time_with_only_arrival_or_departure_time for stop_times.txt
