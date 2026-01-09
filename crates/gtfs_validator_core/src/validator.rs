#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::progress::ProgressHandler;
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
        self.run_with_progress(feed, notices, None)
    }

    /// Run all validators and collect detailed timing information
    pub fn run_with_timing(
        &self,
        feed: &GtfsFeed,
        notices: &mut NoticeContainer,
        timing: &crate::timing::TimingCollector,
    ) {
        self.run_with_progress_and_timing(feed, notices, None, Some(timing))
    }

    pub fn run_with_progress(
        &self,
        feed: &GtfsFeed,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
    ) {
        self.run_with_progress_and_timing(feed, notices, progress, None);
    }

    /// Run validators with optional progress tracking and timing collection
    pub fn run_with_progress_and_timing(
        &self,
        feed: &GtfsFeed,
        notices: &mut NoticeContainer,
        progress: Option<&dyn ProgressHandler>,
        timing: Option<&crate::timing::TimingCollector>,
    ) {
        // Capture thread-local context before execution
        let captured_date = crate::validation_date();
        let captured_country = crate::validation_country_code();
        let captured_google_rules = crate::google_rules_enabled();
        let captured_thorough = crate::thorough_mode_enabled();

        if let Some(p) = progress {
            p.set_total_validators(self.validators.len());
        }

        #[cfg(feature = "parallel")]
        let new_notices = self.run_parallel(
            feed,
            captured_date,
            captured_country,
            captured_google_rules,
            captured_thorough,
            progress,
            timing,
        );

        #[cfg(not(feature = "parallel"))]
        let new_notices = self.run_sequential(
            feed,
            captured_date,
            captured_country,
            captured_google_rules,
            captured_thorough,
            progress,
            timing,
        );

        notices.merge(new_notices);
    }

    #[cfg(feature = "parallel")]
    fn run_parallel(
        &self,
        feed: &GtfsFeed,
        captured_date: chrono::NaiveDate,
        captured_country: Option<String>,
        captured_google_rules: bool,
        captured_thorough: bool,
        progress: Option<&dyn ProgressHandler>,
        timing: Option<&crate::timing::TimingCollector>,
    ) -> NoticeContainer {
        self.validators
            .par_iter()
            .map(|validator| {
                // Propagate thread-local context to worker thread
                let _date_guard = crate::set_validation_date(Some(captured_date));
                let _country_guard = crate::set_validation_country_code(captured_country.clone());
                let _google_rules_guard = crate::set_google_rules_enabled(captured_google_rules);
                let _thorough_guard = crate::set_thorough_mode_enabled(captured_thorough);

                if let Some(p) = progress {
                    p.on_start_validation(validator.name());
                }

                let start = std::time::Instant::now();
                let res = self.run_single_validator(validator.as_ref(), feed);
                let elapsed = start.elapsed();

                if let Some(t) = timing {
                    t.record(
                        validator.name(),
                        elapsed,
                        crate::timing::TimingCategory::Validation,
                    );
                }

                if let Some(p) = progress {
                    p.on_finish_validation(validator.name());
                    p.increment_validator_progress();
                }

                res
            })
            .reduce(NoticeContainer::new, |mut a, b| {
                a.merge(b);
                a
            })
    }

    #[cfg(not(feature = "parallel"))]
    fn run_sequential(
        &self,
        feed: &GtfsFeed,
        captured_date: chrono::NaiveDate,
        captured_country: Option<String>,
        captured_google_rules: bool,
        captured_thorough: bool,
        progress: Option<&dyn ProgressHandler>,
        timing: Option<&crate::timing::TimingCollector>,
    ) -> NoticeContainer {
        // Set context once for sequential execution
        let _date_guard = crate::set_validation_date(Some(captured_date));
        let _country_guard = crate::set_validation_country_code(captured_country);
        let _google_rules_guard = crate::set_google_rules_enabled(captured_google_rules);
        let _thorough_guard = crate::set_thorough_mode_enabled(captured_thorough);

        self.validators
            .iter()
            .map(|validator| {
                if let Some(p) = progress {
                    p.on_start_validation(validator.name());
                }

                #[cfg(not(target_arch = "wasm32"))]
                let start = std::time::Instant::now();
                let res = self.run_single_validator(validator.as_ref(), feed);
                #[cfg(not(target_arch = "wasm32"))]
                let elapsed = start.elapsed();
                #[cfg(target_arch = "wasm32")]
                let elapsed = std::time::Duration::from_secs(0);

                if let Some(t) = timing {
                    t.record(
                        validator.name(),
                        elapsed,
                        crate::timing::TimingCategory::Validation,
                    );
                }

                if let Some(p) = progress {
                    p.on_finish_validation(validator.name());
                    p.increment_validator_progress();
                }
                res
            })
            .fold(NoticeContainer::new(), |mut a, b| {
                a.merge(b);
                a
            })
    }

    fn run_single_validator(&self, validator: &dyn Validator, feed: &GtfsFeed) -> NoticeContainer {
        let mut local_notices = NoticeContainer::new();
        #[cfg(not(target_arch = "wasm32"))]
        let start = std::time::Instant::now();

        // Set resolver hook for StringId serialization
        let pool = feed.pool.clone();
        gtfs_guru_model::set_thread_local_resolver(move |id| pool.resolve(id));

        let result = catch_unwind(AssertUnwindSafe(|| {
            validator.validate(feed, &mut local_notices)
        }));

        // Clear hooks after validation
        gtfs_guru_model::clear_thread_local_hooks();

        #[cfg(not(target_arch = "wasm32"))]
        let elapsed = start.elapsed();
        #[cfg(target_arch = "wasm32")]
        let elapsed = std::time::Duration::from_secs(0);

        if elapsed.as_millis() > 500 {
            eprintln!("[PERF] Validator {} took: {:?}", validator.name(), elapsed);
        }

        if let Err(panic) = result {
            local_notices.push(runtime_exception_in_validator_error_notice(
                validator.name(),
                panic_payload_message(&*panic),
            ));
        }
        local_notices
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
    notice.field_order = vec!["exception".into(), "message".into(), "validator".into()];
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
    notice.field_order = vec!["exception".into(), "message".into()];
    notice
}

fn panic_payload_message(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "panic".into()
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
            stop_times_by_trip: std::collections::HashMap::new(),
            route_networks: None,
            pool: crate::StringPool::new(),
            table_statuses: std::collections::HashMap::new(),
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
