ERROR: block_trips_with_overlapping_stop_times
Description: Trips sharing a block_id have overlapping stop times.
In this test case:
- trip1 runs 08:00-08:20 on block1
- trip2 runs 08:15-08:35 on block1 (starts before trip1 ends)
Expected error: block_trips_with_overlapping_stop_times for trips.txt/stop_times.txt
