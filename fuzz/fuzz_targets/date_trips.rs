#![no_main]
use libfuzzer_sys::fuzz_target;
use gtfs_validator_core::{
    rules::DateTripsValidator, CsvTable, GtfsFeed, NoticeContainer, Validator,
};
use gtfs_model::{
    Agency, Calendar, CalendarDate, ExceptionType, Frequency, GtfsDate, GtfsTime, Route,
    RouteType, ServiceAvailability, Stop, StopTime, Trip,
};
use arbitrary::Arbitrary;
use chrono::NaiveDate;

#[derive(Debug, Arbitrary)]
struct FuzzData {
    trips: Vec<TripData>,
    calendars: Vec<CalendarData>,
    calendar_dates: Vec<CalendarDateData>,
    frequencies: Vec<FrequencyData>,
}

#[derive(Debug, Arbitrary)]
struct TripData {
    trip_id: String,
    service_id: String,
    route_id: String,
}

#[derive(Debug, Arbitrary)]
struct CalendarData {
    service_id: String,
    monday: bool,
    tuesday: bool,
    wednesday: bool,
    thursday: bool,
    friday: bool,
    saturday: bool,
    sunday: bool,
    start_date: u32, // yyyymmdd
    end_date: u32,
}

#[derive(Debug, Arbitrary)]
struct CalendarDateData {
    service_id: String,
    date: u32,
    exception_type: u8,
}

#[derive(Debug, Arbitrary)]
struct FrequencyData {
    trip_id: String,
    start_time: String,
    end_time: String,
    headway_secs: u32,
}

fuzz_target!(|data: FuzzData| {
    let mut feed = GtfsFeed::default();

    // Populate Trips
    let trips: Vec<Trip> = data
        .trips
        .into_iter()
        .map(|t| Trip {
            route_id: t.route_id,
            service_id: t.service_id,
            trip_id: t.trip_id,
            trip_headsign: None,
            trip_short_name: None,
            direction_id: None,
            block_id: None,
            shape_id: None,
            wheelchair_accessible: None,
            bikes_allowed: None,
            continuous_pickup: None,
            continuous_drop_off: None,
        })
        .collect();
    feed.trips = CsvTable {
        headers: vec![],
        rows: trips,
        row_numbers: vec![],
    };

    // Populate Calendar
    let calendars: Vec<Calendar> = data
        .calendars
        .into_iter()
        .filter_map(|c| {
            Some(Calendar {
                service_id: c.service_id,
                monday: if c.monday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                tuesday: if c.tuesday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                wednesday: if c.wednesday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                thursday: if c.thursday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                friday: if c.friday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                saturday: if c.saturday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                sunday: if c.sunday { ServiceAvailability::Available } else { ServiceAvailability::Unavailable },
                start_date: GtfsDate::parse(&c.start_date.to_string()).unwrap_or(GtfsDate::parse("20230101").unwrap()),
                end_date: GtfsDate::parse(&c.end_date.to_string()).unwrap_or(GtfsDate::parse("20231231").unwrap()),
            })
        })
        .collect();
    if !calendars.is_empty() {
         feed.calendar = Some(CsvTable {
            headers: vec![],
            rows: calendars,
            row_numbers: vec![],
        });
    }

    // Populate CalendarDates
    let calendar_dates: Vec<CalendarDate> = data
        .calendar_dates
        .into_iter()
        .filter_map(|cd| {
             Some(CalendarDate {
                service_id: cd.service_id,
                date: GtfsDate::parse(&cd.date.to_string()).ok()?,
                exception_type: match cd.exception_type % 2 {
                    0 => ExceptionType::Added,
                    _ => ExceptionType::Removed,
                },
            })
        })
        .collect();
    if !calendar_dates.is_empty() {
        feed.calendar_dates = Some(CsvTable {
            headers: vec![],
            rows: calendar_dates,
            row_numbers: vec![],
        });
    }

    // Populate Frequencies
     let frequencies: Vec<Frequency> = data
        .frequencies
        .into_iter()
        .filter_map(|f| {
            Some(Frequency {
                trip_id: f.trip_id,
                start_time: GtfsTime::parse(&f.start_time).ok()?,
                end_time: GtfsTime::parse(&f.end_time).ok()?,
                headway_secs: f.headway_secs,
                exact_times: None,
            })
        })
        .collect();
    if !frequencies.is_empty() {
        feed.frequencies = Some(CsvTable {
            headers: vec![],
            rows: frequencies,
            row_numbers: vec![],
        });
    }

    let mut notices = NoticeContainer::new();
    DateTripsValidator.validate(&feed, &mut notices);
});
