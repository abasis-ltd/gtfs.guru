ERROR: overlapping_frequency
Description: Trip frequencies must not overlap in time.
In this test case, frequencies.txt has trip1 with overlapping time ranges:
- 08:00:00 - 10:00:00
- 09:00:00 - 11:00:00 (overlaps with first)
Expected error: overlapping_frequency for frequencies.txt
