ERROR: foreign_key_violation
Description: Foreign key referenced from a given row cannot be found in the parent table.
In this test case, trips.txt references route_id "nonexistent_route" which does not exist in routes.txt.
Expected error: foreign_key_violation for trips.txt (route_id)
