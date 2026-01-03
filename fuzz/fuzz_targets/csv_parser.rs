#![no_main]
use libfuzzer_sys::fuzz_target;
use gtfs_validator_core::csv_reader::read_csv_from_reader;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Row {
    a: String,
    b: String,
    c: String,
}

fuzz_target!(|data: &[u8]| {
    let _ = read_csv_from_reader::<Row, _>(data, "fuzz.csv");
});
