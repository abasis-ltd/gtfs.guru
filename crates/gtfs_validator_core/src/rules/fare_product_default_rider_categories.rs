use std::collections::{HashMap, HashSet};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice, Validator};
use gtfs_guru_model::{RiderFareCategory, StringId};

const CODE_MULTIPLE_DEFAULT_RIDER_CATEGORIES: &str =
    "fare_product_with_multiple_default_rider_categories";

/// A fare product may be sold to several rider categories, but at most one of
/// them may be its default. Several default categories across different
/// products are fine; the check is per fare_product_id.
#[derive(Debug, Default)]
pub struct FareProductDefaultRiderCategoriesValidator;

impl Validator for FareProductDefaultRiderCategoriesValidator {
    fn name(&self) -> &'static str {
        "fare_product_default_rider_categories"
    }

    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        let (Some(fare_products), Some(rider_categories)) =
            (&feed.fare_products, &feed.rider_categories)
        else {
            return;
        };

        // The first row of a repeated rider_category_id wins, as in the
        // canonical id index.
        let mut category_is_default: HashMap<StringId, bool> = HashMap::new();
        for category in &rider_categories.rows {
            if category.rider_category_id.0 == 0 {
                continue;
            }
            category_is_default
                .entry(category.rider_category_id)
                .or_insert(matches!(
                    category.is_default_fare_category,
                    Some(RiderFareCategory::IsDefault)
                ));
        }
        if !category_is_default.values().any(|is_default| *is_default) {
            return;
        }

        let mut seen_default: HashMap<StringId, Vec<(StringId, u64)>> = HashMap::new();
        let mut flagged: HashSet<StringId> = HashSet::new();

        for (index, fare_product) in fare_products.rows.iter().enumerate() {
            let row_number = fare_products.row_number(index);
            let fare_product_id = fare_product.fare_product_id;
            let Some(rider_category_id) = fare_product.rider_category_id.filter(|id| id.0 != 0)
            else {
                continue;
            };
            if category_is_default.get(&rider_category_id) != Some(&true) {
                continue;
            }

            let entry = seen_default.entry(fare_product_id).or_default();
            if entry
                .iter()
                .any(|(existing_id, _)| *existing_id == rider_category_id)
            {
                continue;
            }
            entry.push((rider_category_id, row_number));
            if entry.len() == 2 && flagged.insert(fare_product_id) {
                let (rider_category_id1, row_number1) = entry[0];
                let (rider_category_id2, row_number2) = entry[1];
                let fare_product_id_value = feed.pool.resolve(fare_product_id);
                let rider_category_id1_value = feed.pool.resolve(rider_category_id1);
                let rider_category_id2_value = feed.pool.resolve(rider_category_id2);
                notices.push(multiple_default_categories_notice(
                    row_number1,
                    row_number2,
                    fare_product_id_value.as_str(),
                    rider_category_id1_value.as_str(),
                    rider_category_id2_value.as_str(),
                ));
            }
        }
    }
}

fn multiple_default_categories_notice(
    row_number1: u64,
    row_number2: u64,
    fare_product_id: &str,
    rider_category_id1: &str,
    rider_category_id2: &str,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        CODE_MULTIPLE_DEFAULT_RIDER_CATEGORIES,
        NoticeSeverity::Error,
        "fare_product has multiple default rider categories",
    );
    notice.insert_context_field("fareProductId", fare_product_id);
    notice.insert_context_field("csvRowNumber1", row_number1);
    notice.insert_context_field("csvRowNumber2", row_number2);
    notice.insert_context_field("riderCategoryId1", rider_category_id1);
    notice.insert_context_field("riderCategoryId2", rider_category_id2);
    notice.field_order = vec![
        "fareProductId".into(),
        "csvRowNumber1".into(),
        "csvRowNumber2".into(),
        "riderCategoryId1".into(),
        "riderCategoryId2".into(),
    ];
    notice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CsvTable;
    use gtfs_guru_model::{FareProduct, RiderCategory, RiderFareCategory};

    #[test]
    fn several_defaults_across_products_are_fine() {
        let mut feed = GtfsFeed::default();
        feed.rider_categories = Some(CsvTable {
            headers: vec!["rider_category_id".into(), "is_default_category".into()],
            rows: vec![
                RiderCategory {
                    rider_category_id: feed.pool.intern("C1"),
                    is_default_fare_category: Some(RiderFareCategory::IsDefault),
                    ..Default::default()
                },
                RiderCategory {
                    rider_category_id: feed.pool.intern("C2"),
                    is_default_fare_category: Some(RiderFareCategory::IsDefault),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.fare_products = Some(CsvTable {
            headers: vec!["fare_product_id".into()],
            rows: vec![FareProduct {
                fare_product_id: feed.pool.intern("P1"),
                ..Default::default()
            }],
            row_numbers: vec![2],
        });

        let mut notices = NoticeContainer::new();
        FareProductDefaultRiderCategoriesValidator.validate(&feed, &mut notices);

        assert!(notices.is_empty());
    }

    #[test]
    fn detects_multiple_defaults_for_one_product() {
        let mut feed = GtfsFeed::default();
        feed.rider_categories = Some(CsvTable {
            headers: vec!["rider_category_id".into(), "is_default_category".into()],
            rows: vec![
                RiderCategory {
                    rider_category_id: feed.pool.intern("C1"),
                    is_default_fare_category: Some(RiderFareCategory::IsDefault),
                    ..Default::default()
                },
                RiderCategory {
                    rider_category_id: feed.pool.intern("C2"),
                    is_default_fare_category: Some(RiderFareCategory::IsDefault),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.fare_products = Some(CsvTable {
            headers: vec!["fare_product_id".into(), "rider_category_id".into()],
            rows: vec![
                FareProduct {
                    fare_product_id: feed.pool.intern("P1"),
                    rider_category_id: Some(feed.pool.intern("C1")),
                    ..Default::default()
                },
                FareProduct {
                    fare_product_id: feed.pool.intern("P1"),
                    rider_category_id: Some(feed.pool.intern("C2")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        FareProductDefaultRiderCategoriesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 1);
        let notice = notices.iter().next().unwrap();
        assert_eq!(notice.code, CODE_MULTIPLE_DEFAULT_RIDER_CATEGORIES);
        assert_eq!(notice.context["fareProductId"], "P1");
        assert_eq!(notice.context["csvRowNumber1"], 2);
        assert_eq!(notice.context["csvRowNumber2"], 3);
        assert_eq!(notice.context["riderCategoryId1"], "C1");
        assert_eq!(notice.context["riderCategoryId2"], "C2");
    }

    #[test]
    fn passes_single_default() {
        let mut feed = GtfsFeed::default();
        feed.rider_categories = Some(CsvTable {
            headers: vec!["rider_category_id".into(), "is_default_category".into()],
            rows: vec![
                RiderCategory {
                    rider_category_id: feed.pool.intern("C1"),
                    is_default_fare_category: Some(RiderFareCategory::IsDefault),
                    ..Default::default()
                },
                RiderCategory {
                    rider_category_id: feed.pool.intern("C2"),
                    is_default_fare_category: Some(RiderFareCategory::NotDefault),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });
        feed.fare_products = Some(CsvTable {
            headers: vec!["fare_product_id".into(), "rider_category_id".into()],
            rows: vec![
                FareProduct {
                    fare_product_id: feed.pool.intern("P1"),
                    rider_category_id: Some(feed.pool.intern("C1")),
                    ..Default::default()
                },
                FareProduct {
                    fare_product_id: feed.pool.intern("P1"),
                    rider_category_id: Some(feed.pool.intern("C2")),
                    ..Default::default()
                },
            ],
            row_numbers: vec![2, 3],
        });

        let mut notices = NoticeContainer::new();
        FareProductDefaultRiderCategoriesValidator.validate(&feed, &mut notices);

        assert_eq!(notices.len(), 0);
    }
}
