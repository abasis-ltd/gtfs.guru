use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};

const CODE_ROUTE_COLOR_CONTRAST: &str = "route_color_contrast";
const MAX_ROUTE_COLOR_LUMA_DIFFERENCE: i32 = 72;

#[derive(Debug, Default)]
pub struct RouteColorContrastValidator;

impl Validator for RouteColorContrastValidator {
    fn name(&self) -> &'static str {
        "route_color_contrast"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        for (index, route) in feed.routes.rows.iter().enumerate() {
            let row_number = feed.routes.row_number(index);
            let (Some(route_color), Some(route_text_color)) =
                (route.route_color, route.route_text_color)
            else {
                continue;
            };

            let diff = (route_color.rec601_luma() - route_text_color.rec601_luma()).abs();
            if diff < MAX_ROUTE_COLOR_LUMA_DIFFERENCE {
                let mut notice = ValidationNotice::new(
                    CODE_ROUTE_COLOR_CONTRAST,
                    NoticeSeverity::Warning,
                    "route_color and route_text_color have insufficient contrast",
                );
                notice.insert_context_field("routeId", feed.pool.resolve(route.route_id).as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("routeColor", route_color);
                notice.insert_context_field("routeTextColor", route_text_color);
                notice.field_order = vec![
                    "csvRowNumber".into(),
                    "routeColor".into(),
                    "routeId".into(),
                    "routeTextColor".into(),
                ];
                notices.push(notice);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{GtfsColor, Route};

    #[test]
    fn detects_poor_contrast() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_color".into(),
                "route_text_color".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_color: Some(GtfsColor::new(255, 255, 255)), // White
                route_text_color: Some(GtfsColor::new(200, 200, 200)), // Light Grey (poor contrast)
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RouteColorContrastValidator.validate(&feed, &mut notices);

        assert!(notices.iter().any(|n| n.code == CODE_ROUTE_COLOR_CONTRAST));
    }

    #[test]
    fn passes_good_contrast() {
        let mut feed = GtfsFeed::default();
        feed.routes = CsvTable {
            headers: vec![
                "route_id".into(),
                "route_color".into(),
                "route_text_color".into(),
            ],
            rows: vec![Route {
                route_id: feed.pool.intern("R1"),
                route_color: Some(GtfsColor::new(255, 255, 255)), // White
                route_text_color: Some(GtfsColor::new(0, 0, 0)),  // Black (good contrast)
                ..Default::default()
            }],
            row_numbers: vec![2],
        };

        let mut notices = NoticeContainer::new();
        RouteColorContrastValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
