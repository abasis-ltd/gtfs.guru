ERROR: stop_time_with_arrival_before_previous_departure_time
Description: Backwards time travel between stops - arrival at next stop is before departure from previous stop.
In this test case, stop_times.txt has:
- stop1 departure at 08:10:00
- stop2 arrival at 08:05:00 (before previous departure)
Expected error: stop_time_with_arrival_before_previous_departure_time for stop_times.txt
