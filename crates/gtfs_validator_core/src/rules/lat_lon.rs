use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_POINT_NEAR_ORIGIN: &str = "point_near_origin";
const CODE_POINT_NEAR_POLE: &str = "point_near_pole";

#[derive(Debug, Default)]
pub struct StopLatLonValidator;

impl Validator for StopLatLonValidator {
    fn name(&self) -> &'static str {
        "stop_lat_lon"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, stop) in feed.stops.rows.iter().enumerate() {
            let row_number = feed.stops.row_number(index);
            if let (Some(lat), Some(lon)) = (stop.stop_lat, stop.stop_lon) {
                check_point(
                    notices,
                    "stops.txt",
                    "stop_lat",
                    "stop_lon",
                    lat,
                    lon,
                    row_number,
                    Some(feed.pool.resolve(stop.stop_id).as_str()),
                );
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ShapeLatLonValidator;

impl Validator for ShapeLatLonValidator {
    fn name(&self) -> &'static str {
        "shape_lat_lon"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        if let Some(shapes) = &feed.shapes {
            for (index, shape) in shapes.rows.iter().enumerate() {
                let row_number = shapes.row_number(index);
                check_point(
                    notices,
                    "shapes.txt",
                    "shape_pt_lat",
                    "shape_pt_lon",
                    shape.shape_pt_lat,
                    shape.shape_pt_lon,
                    row_number,
                    None,
                );
            }
        }
    }
}

const CODE_NUMBER_OUT_OF_RANGE: &str = "number_out_of_range";

fn check_point(
    notices: &mut NoticeContainer,
    file: &str,
    lat_field: &str,
    lon_field: &str,
    lat: f64,
    lon: f64,
    row_number: u64,
    entity_id: Option<&str>,
) {
    if !(-90.0..=90.0).contains(&lat) {
        let mut notice = ValidationNotice::new(
            CODE_NUMBER_OUT_OF_RANGE,
            NoticeSeverity::Error,
            "latitude out of range [-90, 90]",
        );
        notice.insert_context_field("filename", file);
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("fieldName", lat_field);
        notice.insert_context_field("fieldValue", lat);
        notice.insert_context_field("fieldType", "float");
        if let Some(entity_id) = entity_id {
            notice.insert_context_field("entityId", entity_id);
        }
        notice.field_order = vec![
            "csvRowNumber".into(),
            "entityId".into(),
            "fieldName".into(),
            "fieldType".into(),
            "fieldValue".into(),
            "filename".into(),
        ];
        notices.push(notice);
    }

    if !(-180.0..=180.0).contains(&lon) {
        let mut notice = ValidationNotice::new(
            CODE_NUMBER_OUT_OF_RANGE,
            NoticeSeverity::Error,
            "longitude out of range [-180, 180]",
        );
        notice.insert_context_field("filename", file);
        notice.insert_context_field("csvRowNumber", row_number);
        notice.insert_context_field("fieldName", lon_field);
        notice.insert_context_field("fieldValue", lon);
        notice.insert_context_field("fieldType", "float");
        if let Some(entity_id) = entity_id {
            notice.insert_context_field("entityId", entity_id);
        }
        notice.field_order = vec![
            "csvRowNumber".into(),
            "entityId".into(),
            "fieldName".into(),
            "fieldType".into(),
            "fieldValue".into(),
            "filename".into(),
        ];
        notices.push(notice);
    }

    if lat.abs() <= 1.0 && lon.abs() <= 1.0 {
        let mut notice = ValidationNotice::new(
            CODE_POINT_NEAR_ORIGIN,
            NoticeSeverity::Error,
            "point near origin",
        );
        notice.insert_context_field("filename", file);
        notice.insert_context_field("csvRowNumber", row_number);
        if let Some(entity_id) = entity_id {
            notice.insert_context_field("entityId", entity_id);
        }
        notice.insert_context_field("latFieldName", lat_field);
        notice.insert_context_field("latFieldValue", lat);
        notice.insert_context_field("lonFieldName", lon_field);
        notice.insert_context_field("lonFieldValue", lon);
        notice.field_order = vec![
            "csvRowNumber".into(),
            "entityId".into(),
            "featureIndex".into(),
            "filename".into(),
            "latFieldName".into(),
            "latFieldValue".into(),
            "lonFieldName".into(),
            "lonFieldValue".into(),
        ];
        notices.push(notice);
    }

    if lat.abs() >= 89.0 {
        let mut notice = ValidationNotice::new(
            CODE_POINT_NEAR_POLE,
            NoticeSeverity::Error,
            "point near pole",
        );
        notice.insert_context_field("filename", file);
        notice.insert_context_field("csvRowNumber", row_number);
        if let Some(entity_id) = entity_id {
            notice.insert_context_field("entityId", entity_id);
        }
        notice.insert_context_field("latFieldName", lat_field);
        notice.insert_context_field("latFieldValue", lat);
        notice.insert_context_field("lonFieldName", lon_field);
        notice.insert_context_field("lonFieldValue", lon);
        notice.field_order = vec![
            "csvRowNumber".into(),
            "entityId".into(),
            "featureIndex".into(),
            "filename".into(),
            "latFieldName".into(),
            "latFieldValue".into(),
            "lonFieldName".into(),
            "lonFieldValue".into(),
        ];
        notices.push(notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;

    #[test]
    fn test_stop_lat_lon_out_of_range() {
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![],
            rows: vec![gtfs_guru_model::Stop {
                stop_id: feed.pool.intern("S1"),
                stop_lat: Some(95.0),  // Too high
                stop_lon: Some(200.0), // Too high
                ..Default::default()
            }],
            row_numbers: vec![1],
        };

        let mut notices = NoticeContainer::new();
        StopLatLonValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 3);
        let mut codes: Vec<_> = notices.iter().map(|n| n.code.as_str()).collect();
        codes.sort();
        assert_eq!(
            codes,
            vec![
                "number_out_of_range",
                "number_out_of_range",
                "point_near_pole"
            ]
        );
    }

    #[test]
    fn test_stop_lat_lon_near_origin() {
        let _guard = crate::validation_context::set_thorough_mode_enabled(true);
        let mut feed = GtfsFeed::default();
        feed.stops = CsvTable {
            headers: vec![],
            rows: vec![gtfs_guru_model::Stop {
                stop_id: feed.pool.intern("S1"),
                stop_lat: Some(0.5),
                stop_lon: Some(0.5),
                ..Default::default()
            }],
            row_numbers: vec![1],
        };

        let mut notices = NoticeContainer::new();
        StopLatLonValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "point_near_origin");
    }
}
