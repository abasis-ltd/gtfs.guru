A forbidden field value is present for a prior-day booking rule in booking_rules.txt.

This test has booking_type=2 (prior day booking) but incorrectly includes
prior_notice_duration_min which is only valid for same-day booking (booking_type=1).

Expected notice: forbidden_prior_day_booking_field_value
