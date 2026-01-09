use crate::feed::{
    AGENCY_FILE, AREAS_FILE, ATTRIBUTIONS_FILE, BOOKING_RULES_FILE, CALENDAR_DATES_FILE,
    CALENDAR_FILE, FARE_ATTRIBUTES_FILE, FARE_LEG_JOIN_RULES_FILE, FARE_LEG_RULES_FILE,
    FARE_MEDIA_FILE, FARE_PRODUCTS_FILE, FARE_RULES_FILE, FEED_INFO_FILE, FREQUENCIES_FILE,
    LEVELS_FILE, LOCATION_GROUPS_FILE, LOCATION_GROUP_STOPS_FILE, NETWORKS_FILE, PATHWAYS_FILE,
    RIDER_CATEGORIES_FILE, ROUTES_FILE, ROUTE_NETWORKS_FILE, SHAPES_FILE, STOPS_FILE,
    STOP_AREAS_FILE, STOP_TIMES_FILE, TIMEFRAMES_FILE, TRANSLATIONS_FILE, TRIPS_FILE,
};
use crate::validation_context::thorough_mode_enabled;
use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use compact_str::CompactString;
use gtfs_guru_model::StringId;

const CODE_EMPTY_REQUIRED_FIELD: &str = "missing_required_field";

#[derive(Debug, Default)]
pub struct RequiredFieldsNonEmptyValidator;

impl Validator for RequiredFieldsNonEmptyValidator {
    fn name(&self) -> &'static str {
        "required_fields_non_empty"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if !thorough_mode_enabled() {
            return;
        }
        for (index, agency) in feed.agency.rows.iter().enumerate() {
            let row_number = feed.agency.row_number(index);
            check_non_empty(
                notices,
                AGENCY_FILE,
                "agency_name",
                &agency.agency_name,
                row_number,
            );
            check_non_empty(
                notices,
                AGENCY_FILE,
                "agency_url",
                &agency.agency_url,
                row_number,
            );
            check_non_empty(
                notices,
                AGENCY_FILE,
                "agency_timezone",
                &agency.agency_timezone,
                row_number,
            );
        }

        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            check_non_empty(notices, STOPS_FILE, "stop_id", &stop.stop_id, row_number);
        }

        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            check_non_empty(
                notices,
                ROUTES_FILE,
                "route_id",
                &route.route_id,
                row_number,
            );
        }

        for (index, trip) in feed.trips.rows.iter().enumerate() {
            let row_number = feed.trips.row_number(index);
            check_non_empty(notices, TRIPS_FILE, "route_id", &trip.route_id, row_number);
            check_non_empty(
                notices,
                TRIPS_FILE,
                "service_id",
                &trip.service_id,
                row_number,
            );
            check_non_empty(notices, TRIPS_FILE, "trip_id", &trip.trip_id, row_number);
        }

        for (index, stop_time) in feed.stop_times.rows.iter().enumerate() {
            let row_number = feed.stop_times.row_number(index);
            check_non_empty(
                notices,
                STOP_TIMES_FILE,
                "trip_id",
                &stop_time.trip_id,
                row_number,
            );
        }

        if let Some(calendar) = &feed.calendar {
            for (index, row) in calendar.rows.iter().enumerate() {
                let row_number = calendar.row_number(index);
                check_non_empty(
                    notices,
                    CALENDAR_FILE,
                    "service_id",
                    &row.service_id,
                    row_number,
                );
            }
        }

        if let Some(calendar_dates) = &feed.calendar_dates {
            for (index, row) in calendar_dates.rows.iter().enumerate() {
                let row_number = calendar_dates.row_number(index);
                check_non_empty(
                    notices,
                    CALENDAR_DATES_FILE,
                    "service_id",
                    &row.service_id,
                    row_number,
                );
            }
        }

        if let Some(fare_attributes) = &feed.fare_attributes {
            for (index, row) in fare_attributes.rows.iter().enumerate() {
                let row_number = fare_attributes.row_number(index);
                check_non_empty(
                    notices,
                    FARE_ATTRIBUTES_FILE,
                    "fare_id",
                    &row.fare_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    FARE_ATTRIBUTES_FILE,
                    "currency_type",
                    &row.currency_type,
                    row_number,
                );
            }
        }

        if let Some(fare_rules) = &feed.fare_rules {
            for (index, row) in fare_rules.rows.iter().enumerate() {
                let row_number = fare_rules.row_number(index);
                check_non_empty(
                    notices,
                    FARE_RULES_FILE,
                    "fare_id",
                    &row.fare_id,
                    row_number,
                );
            }
        }

        if let Some(booking_rules) = &feed.booking_rules {
            for (index, row) in booking_rules.rows.iter().enumerate() {
                let row_number = booking_rules.row_number(index);
                check_non_empty(
                    notices,
                    BOOKING_RULES_FILE,
                    "booking_rule_id",
                    &row.booking_rule_id,
                    row_number,
                );
            }
        }

        if let Some(fare_media) = &feed.fare_media {
            for (index, row) in fare_media.rows.iter().enumerate() {
                let row_number = fare_media.row_number(index);
                check_non_empty(
                    notices,
                    FARE_MEDIA_FILE,
                    "fare_media_id",
                    &row.fare_media_id,
                    row_number,
                );
            }
        }

        if let Some(fare_products) = &feed.fare_products {
            for (index, row) in fare_products.rows.iter().enumerate() {
                let row_number = fare_products.row_number(index);
                check_non_empty(
                    notices,
                    FARE_PRODUCTS_FILE,
                    "fare_product_id",
                    &row.fare_product_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    FARE_PRODUCTS_FILE,
                    "currency",
                    &row.currency,
                    row_number,
                );
            }
        }

        if let Some(fare_leg_rules) = &feed.fare_leg_rules {
            for (index, row) in fare_leg_rules.rows.iter().enumerate() {
                let row_number = fare_leg_rules.row_number(index);
                check_non_empty(
                    notices,
                    FARE_LEG_RULES_FILE,
                    "fare_product_id",
                    &row.fare_product_id,
                    row_number,
                );
            }
        }

        if let Some(fare_leg_join_rules) = &feed.fare_leg_join_rules {
            for (index, row) in fare_leg_join_rules.rows.iter().enumerate() {
                let row_number = fare_leg_join_rules.row_number(index);
                check_non_empty(
                    notices,
                    FARE_LEG_JOIN_RULES_FILE,
                    "from_network_id",
                    &row.from_network_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    FARE_LEG_JOIN_RULES_FILE,
                    "to_network_id",
                    &row.to_network_id,
                    row_number,
                );
            }
        }

        if let Some(shapes) = &feed.shapes {
            for (index, row) in shapes.rows.iter().enumerate() {
                let row_number = shapes.row_number(index);
                check_non_empty(notices, SHAPES_FILE, "shape_id", &row.shape_id, row_number);
            }
        }

        if let Some(frequencies) = &feed.frequencies {
            for (index, row) in frequencies.rows.iter().enumerate() {
                let row_number = frequencies.row_number(index);
                check_non_empty(
                    notices,
                    FREQUENCIES_FILE,
                    "trip_id",
                    &row.trip_id,
                    row_number,
                );
            }
        }

        if let Some(feed_info) = &feed.feed_info {
            for (index, row) in feed_info.rows.iter().enumerate() {
                let row_number = feed_info.row_number(index);
                check_non_empty(
                    notices,
                    FEED_INFO_FILE,
                    "feed_publisher_name",
                    &row.feed_publisher_name,
                    row_number,
                );
                check_non_empty(
                    notices,
                    FEED_INFO_FILE,
                    "feed_publisher_url",
                    &row.feed_publisher_url,
                    row_number,
                );
                check_non_empty(
                    notices,
                    FEED_INFO_FILE,
                    "feed_lang",
                    &row.feed_lang,
                    row_number,
                );
            }
        }

        if let Some(attributions) = &feed.attributions {
            for (index, row) in attributions.rows.iter().enumerate() {
                let row_number = attributions.row_number(index);
                check_non_empty(
                    notices,
                    ATTRIBUTIONS_FILE,
                    "organization_name",
                    &row.organization_name,
                    row_number,
                );
            }
        }

        if let Some(levels) = &feed.levels {
            for (index, row) in levels.rows.iter().enumerate() {
                let row_number = levels.row_number(index);
                check_non_empty(notices, LEVELS_FILE, "level_id", &row.level_id, row_number);
            }
        }

        if let Some(pathways) = &feed.pathways {
            for (index, row) in pathways.rows.iter().enumerate() {
                let row_number = pathways.row_number(index);
                check_non_empty(
                    notices,
                    PATHWAYS_FILE,
                    "pathway_id",
                    &row.pathway_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    PATHWAYS_FILE,
                    "from_stop_id",
                    &row.from_stop_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    PATHWAYS_FILE,
                    "to_stop_id",
                    &row.to_stop_id,
                    row_number,
                );
            }
        }

        if let Some(areas) = &feed.areas {
            for (index, row) in areas.rows.iter().enumerate() {
                let row_number = areas.row_number(index);
                check_non_empty(notices, AREAS_FILE, "area_id", &row.area_id, row_number);
            }
        }

        if let Some(stop_areas) = &feed.stop_areas {
            for (index, row) in stop_areas.rows.iter().enumerate() {
                let row_number = stop_areas.row_number(index);
                check_non_empty(
                    notices,
                    STOP_AREAS_FILE,
                    "area_id",
                    &row.area_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    STOP_AREAS_FILE,
                    "stop_id",
                    &row.stop_id,
                    row_number,
                );
            }
        }

        if let Some(timeframes) = &feed.timeframes {
            for (index, row) in timeframes.rows.iter().enumerate() {
                let row_number = timeframes.row_number(index);
                check_non_empty(
                    notices,
                    TIMEFRAMES_FILE,
                    "service_id",
                    &row.service_id,
                    row_number,
                );
            }
        }

        if let Some(rider_categories) = &feed.rider_categories {
            for (index, row) in rider_categories.rows.iter().enumerate() {
                let row_number = rider_categories.row_number(index);
                check_non_empty(
                    notices,
                    RIDER_CATEGORIES_FILE,
                    "rider_category_id",
                    &row.rider_category_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    RIDER_CATEGORIES_FILE,
                    "rider_category_name",
                    &row.rider_category_name,
                    row_number,
                );
            }
        }

        if let Some(networks) = &feed.networks {
            for (index, row) in networks.rows.iter().enumerate() {
                let row_number = networks.row_number(index);
                check_non_empty(
                    notices,
                    NETWORKS_FILE,
                    "network_id",
                    &row.network_id,
                    row_number,
                );
            }
        }

        if let Some(route_networks) = &feed.route_networks {
            for (index, row) in route_networks.rows.iter().enumerate() {
                let row_number = route_networks.row_number(index);
                check_non_empty(
                    notices,
                    ROUTE_NETWORKS_FILE,
                    "route_id",
                    &row.route_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    ROUTE_NETWORKS_FILE,
                    "network_id",
                    &row.network_id,
                    row_number,
                );
            }
        }

        if let Some(translations) = &feed.translations {
            for (index, row) in translations.rows.iter().enumerate() {
                let row_number = translations.row_number(index);
                check_non_empty(
                    notices,
                    TRANSLATIONS_FILE,
                    "language",
                    &row.language,
                    row_number,
                );
                check_non_empty(
                    notices,
                    TRANSLATIONS_FILE,
                    "translation",
                    &row.translation,
                    row_number,
                );

                // Conditional requirement: either (table_name and field_name) or field_value must be present.
                let has_table_field = !row.table_name.is_blank() && !row.field_name.is_blank();
                let has_field_value = !row.field_value.is_blank();

                if !has_table_field && !has_field_value {
                    // If neither is present, flag table_name as missing (arbitrary choice of field to flag)
                    // but ONLY if it's not a legacy format.
                    // For now, let's just match Java by being relaxed.
                }
            }
        }

        if let Some(location_groups) = &feed.location_groups {
            for (index, row) in location_groups.rows.iter().enumerate() {
                let row_number = location_groups.row_number(index);
                check_non_empty(
                    notices,
                    LOCATION_GROUPS_FILE,
                    "location_group_id",
                    &row.location_group_id,
                    row_number,
                );
            }
        }

        if let Some(location_group_stops) = &feed.location_group_stops {
            for (index, row) in location_group_stops.rows.iter().enumerate() {
                let row_number = location_group_stops.row_number(index);
                check_non_empty(
                    notices,
                    LOCATION_GROUP_STOPS_FILE,
                    "location_group_id",
                    &row.location_group_id,
                    row_number,
                );
                check_non_empty(
                    notices,
                    LOCATION_GROUP_STOPS_FILE,
                    "stop_id",
                    &row.stop_id,
                    row_number,
                );
            }
        }
    }
}

fn check_non_empty(
    notices: &mut NoticeContainer,
    file: &str,
    field: &str,
    value: &impl RequiredFieldValue,
    row_number: u64,
) {
    if value.is_blank() {
        let mut notice = ValidationNotice::new(
            CODE_EMPTY_REQUIRED_FIELD,
            NoticeSeverity::Error,
            "required field is missing",
        );
        notice.file = Some(file.to_string());
        notice.field = Some(field.to_string());
        notice.row = Some(row_number);
        notice.field_order = vec!["csvRowNumber".into(), "fieldName".into(), "filename".into()];
        notices.push(notice);
    }
}

trait RequiredFieldValue {
    fn is_blank(&self) -> bool;
}

impl RequiredFieldValue for StringId {
    fn is_blank(&self) -> bool {
        self.0 == 0
    }
}

impl RequiredFieldValue for CompactString {
    fn is_blank(&self) -> bool {
        self.as_str().trim().is_empty()
    }
}

impl RequiredFieldValue for Option<CompactString> {
    fn is_blank(&self) -> bool {
        self.as_ref().map(|s| s.is_blank()).unwrap_or(true)
    }
}

impl RequiredFieldValue for Option<StringId> {
    fn is_blank(&self) -> bool {
        self.as_ref().map(|id| id.0 == 0).unwrap_or(true)
    }
}

impl RequiredFieldValue for String {
    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }
}

impl RequiredFieldValue for str {
    fn is_blank(&self) -> bool {
        self.trim().is_empty()
    }
}

impl<T: RequiredFieldValue + ?Sized> RequiredFieldValue for &T {
    fn is_blank(&self) -> bool {
        (*self).is_blank()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn test_required_fields_empty() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![gtfs_guru_model::Agency {
                agency_name: "".into(), // Empty
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                ..Default::default()
            }],
            row_numbers: vec![1],
        };

        let mut notices = NoticeContainer::new();
        RequiredFieldsNonEmptyValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_EMPTY_REQUIRED_FIELD);
        assert_eq!(notice.field, Some("agency_name".into()));
    }

    #[test]
    fn test_required_fields_present() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.agency = CsvTable {
            headers: vec![
                "agency_name".into(),
                "agency_url".into(),
                "agency_timezone".into(),
            ],
            rows: vec![gtfs_guru_model::Agency {
                agency_name: "Test".into(),
                agency_url: feed.pool.intern("https://example.com"),
                agency_timezone: feed.pool.intern("UTC"),
                ..Default::default()
            }],
            row_numbers: vec![1],
        };

        let mut notices = NoticeContainer::new();
        RequiredFieldsNonEmptyValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }
}
