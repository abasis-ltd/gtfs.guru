use rayon::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{GtfsFeed, NoticeContainer, NoticeSeverity, ValidationNotice};

pub trait Validator: Send + Sync {
    fn name(&self) -> &'static str;
    fn validate(&self, feed: &GtfsFeed, notices: &mut NoticeContainer);
}

#[derive(Default)]
pub struct ValidatorRunner {
    validators: Vec<Box<dyn Validator>>,
}

impl ValidatorRunner {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
        }
    }

    pub fn register<V>(&mut self, validator: V)
    where
        V: Validator + 'static,
    {
        self.validators.push(Box::new(validator));
    }

    pub fn run(&self, feed: &GtfsFeed) -> NoticeContainer {
        let mut notices = NoticeContainer::new();
        self.run_with(feed, &mut notices);
        notices
    }

    pub fn run_with(&self, feed: &GtfsFeed, notices: &mut NoticeContainer) {
        // Capture thread-local context before parallel execution
        let captured_date = crate::validation_date();
        let captured_country = crate::validation_country_code();
        let captured_google_rules = crate::google_rules_enabled();

        let new_notices = self
            .validators
            .par_iter()
            .map(|validator| {
                // Propagate thread-local context to worker thread
                let _date_guard = crate::set_validation_date(Some(captured_date));
                let _country_guard = crate::set_validation_country_code(captured_country.clone());
                let _google_rules_guard = crate::set_google_rules_enabled(captured_google_rules);

                let mut local_notices = NoticeContainer::new();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    validator.validate(feed, &mut local_notices)
                }));

                if let Err(panic) = result {
                    local_notices.push(runtime_exception_in_validator_error_notice(
                        validator.name(),
                        panic_payload_message(&*panic),
                    ));
                }
                local_notices
            })
            .reduce(NoticeContainer::new, |mut a, b| {
                a.merge(b);
                a
            });

        notices.merge(new_notices);
    }

    pub fn is_empty(&self) -> bool {
        self.validators.is_empty()
    }
}

fn runtime_exception_in_validator_error_notice(
    validator: &str,
    message: String,
) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "runtime_exception_in_validator_error",
        NoticeSeverity::Error,
        "runtime exception while validating gtfs",
    );
    notice.insert_context_field("exception", "panic");
    notice.insert_context_field("message", message);
    notice.insert_context_field("validator", validator);
    notice.field_order = vec![
        "exception".to_string(),
        "message".to_string(),
        "validator".to_string(),
    ];
    notice
}

#[allow(dead_code)]
fn thread_execution_error_notice(message: &str) -> ValidationNotice {
    let mut notice = ValidationNotice::new(
        "thread_execution_error",
        NoticeSeverity::Error,
        "thread execution error",
    );
    notice.insert_context_field("exception", "thread_execution_error");
    notice.insert_context_field("message", message);
    notice.field_order = vec!["exception".to_string(), "message".to_string()];
    notice
}

fn panic_payload_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NoticeSeverity, ValidationNotice};

    struct TestValidator;

    impl Validator for TestValidator {
        fn name(&self) -> &'static str {
            "test_validator"
        }

        fn validate(&self, _feed: &GtfsFeed, notices: &mut NoticeContainer) {
            notices.push(ValidationNotice::new(
                "TEST_NOTICE",
                NoticeSeverity::Info,
                "validator ran",
            ));
        }
    }

    #[test]
    fn runs_registered_validators() {
        let mut runner = ValidatorRunner::new();
        runner.register(TestValidator);

        let feed = dummy_feed();
        let notices = runner.run(&feed);

        assert_eq!(notices.len(), 1);
        assert_eq!(notices.iter().next().unwrap().code, "TEST_NOTICE");
    }

    fn dummy_feed() -> GtfsFeed {
        GtfsFeed {
            agency: empty_table(),
            stops: empty_table(),
            routes: empty_table(),
            trips: empty_table(),
            stop_times: empty_table(),
            calendar: None,
            calendar_dates: None,
            fare_attributes: None,
            fare_rules: None,
            fare_media: None,
            fare_products: None,
            fare_leg_rules: None,
            fare_transfer_rules: None,
            fare_leg_join_rules: None,
            areas: None,
            stop_areas: None,
            timeframes: None,
            rider_categories: None,
            shapes: None,
            frequencies: None,
            transfers: None,
            location_groups: None,
            location_group_stops: None,
            locations: None,
            booking_rules: None,
            feed_info: None,
            attributions: None,
            levels: None,
            pathways: None,
            translations: None,
            networks: None,
            route_networks: None,
        }
    }

    fn empty_table<T>() -> crate::CsvTable<T> {
        crate::CsvTable {
            headers: Vec::new(),
            rows: Vec::new(),
            row_numbers: Vec::new(),
        }
    }
}
