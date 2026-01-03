#![no_main]
use libfuzzer_sys::fuzz_target;
use gtfs_validator_core::{
    rules::TimeframeOverlapValidator, CsvTable, GtfsFeed, NoticeContainer, Validator,
};
use gtfs_model::{GtfsTime, Timeframe};
use arbitrary::Arbitrary;

#[derive(Debug, Arbitrary)]
struct FuzzData {
    timeframes: Vec<TimeframeData>,
}

#[derive(Debug, Arbitrary)]
struct TimeframeData {
    timeframe_group_id: Option<String>,
    start_time: String,
    end_time: String,
    service_id: String,
}

fuzz_target!(|data: FuzzData| {
    let mut feed = GtfsFeed::default();

    let timeframes: Vec<Timeframe> = data
        .timeframes
        .into_iter()
        .filter_map(|t| {
            Some(Timeframe {
                timeframe_group_id: t.timeframe_group_id,
                start_time: GtfsTime::parse(&t.start_time).ok(),
                end_time: GtfsTime::parse(&t.end_time).ok(),
                service_id: t.service_id,
            })
        })
        .collect();

    if !timeframes.is_empty() {
        let len = timeframes.len();
        feed.timeframes = Some(CsvTable {
            headers: vec![],
            rows: timeframes,
            row_numbers: vec![0; len], // Dummy row numbers
        });
    }

    let mut notices = NoticeContainer::new();
    TimeframeOverlapValidator.validate(&feed, &mut notices);
});
