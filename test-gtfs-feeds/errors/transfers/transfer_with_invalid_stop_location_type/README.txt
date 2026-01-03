ERROR: transfer_with_invalid_stop_location_type
Description: Stop references in transfers.txt must be stops (location_type=0) or stations (location_type=1), not entrances.
In this test case, transfers.txt references entrance1 which has location_type=2 (entrance).
Expected error: transfer_with_invalid_stop_location_type for transfers.txt
