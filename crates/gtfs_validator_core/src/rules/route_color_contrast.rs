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
                notice.insert_context_field("routeId", route.route_id.as_str());
                notice.insert_context_field("csvRowNumber", row_number);
                notice.insert_context_field("routeColor", route_color);
                notice.insert_context_field("routeTextColor", route_text_color);
                notice.field_order = vec![
                    "csvRowNumber".to_string(),
                    "routeColor".to_string(),
                    "routeId".to_string(),
                    "routeTextColor".to_string(),
                ];
                notices.push(notice);
            }
        }
    }
}

