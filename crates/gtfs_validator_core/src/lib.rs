#![allow(clippy::too_many_arguments)]
#![allow(clippy::useless_vec)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::manual_map)]
#![allow(clippy::unnecessary_get_then_check)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::type_complexity)]
#![allow(clippy::option_as_ref_deref)]
#![allow(clippy::needless_return)]
#![allow(clippy::result_large_err)]
#![allow(clippy::manual_pattern_char_comparison)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::manual_range_patterns)]
#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::field_reassign_with_default)]
pub mod csv_reader;
mod csv_schema;
mod csv_validation;
pub mod engine;
pub mod feed;
pub mod geojson;
pub mod input;
pub mod notice;
pub mod notice_schema;
pub mod progress;
pub mod rules;
pub mod string_pool;
mod table_status;
pub mod timing;
mod validation_context;
pub mod validator;

pub use csv_reader::{read_csv_from_reader, CsvParseError, CsvTable};
pub use engine::{
    validate_bytes, validate_bytes_reader, validate_input, validate_input_and_progress,
    ValidationOutcome,
};
pub use feed::GtfsFeed;
pub use input::{
    collect_input_notices, GtfsBytesReader, GtfsInput, GtfsInputError, GtfsInputReader,
    GtfsInputSource,
};
pub use notice::{Fix, FixOperation, FixSafety, NoticeContainer, NoticeSeverity, ValidationNotice};
pub use notice_schema::build_notice_schema_map;
pub use progress::{NoOpProgressHandler, ProgressHandler};
pub use rules::default_runner;
pub use string_pool::StringPool;
pub use table_status::TableStatus;
pub use timing::{TimingCategory, TimingCollector, TimingRecord, TimingSummary};
pub use validation_context::{
    google_rules_enabled, set_google_rules_enabled, set_thorough_mode_enabled,
    set_validation_country_code, set_validation_date, thorough_mode_enabled,
    validation_country_code, validation_date, ThoroughModeGuard, ValidationContextState,
    ValidationCountryCodeGuard, ValidationDateGuard, ValidationGoogleRulesGuard,
};
pub use validator::{Validator, ValidatorRunner};
