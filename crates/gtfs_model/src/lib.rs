use std::fmt;

use chrono::NaiveDate;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum GtfsParseError {
    #[error("invalid date format: {0}")]
    InvalidDateFormat(String),
    #[error("invalid date value: {0}")]
    InvalidDateValue(String),
    #[error("invalid time format: {0}")]
    InvalidTimeFormat(String),
    #[error("invalid time value: {0}")]
    InvalidTimeValue(String),
    #[error("invalid color format: {0}")]
    InvalidColorFormat(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GtfsDate {
    year: i32,
    month: u8,
    day: u8,
}

impl GtfsDate {
    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        let trimmed = value.trim();
        if trimmed.len() != 8 || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(GtfsParseError::InvalidDateFormat(value.to_string()));
        }

        let year: i32 = trimmed[0..4]
            .parse()
            .map_err(|_| GtfsParseError::InvalidDateFormat(value.to_string()))?;
        let month: u8 = trimmed[4..6]
            .parse()
            .map_err(|_| GtfsParseError::InvalidDateFormat(value.to_string()))?;
        let day: u8 = trimmed[6..8]
            .parse()
            .map_err(|_| GtfsParseError::InvalidDateFormat(value.to_string()))?;

        if NaiveDate::from_ymd_opt(year, month as u32, day as u32).is_none() {
            return Err(GtfsParseError::InvalidDateValue(value.to_string()));
        }

        Ok(Self { year, month, day })
    }

    pub fn year(&self) -> i32 {
        self.year
    }

    pub fn month(&self) -> u8 {
        self.month
    }

    pub fn day(&self) -> u8 {
        self.day
    }
}

impl fmt::Display for GtfsDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}{:02}{:02}", self.year, self.month, self.day)
    }
}

impl Serialize for GtfsDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsDateVisitor;

        impl<'de> Visitor<'de> for GtfsDateVisitor {
            type Value = GtfsDate;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a GTFS date in YYYYMMDD format")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsDate, E> {
                GtfsDate::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsDateVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GtfsTime {
    total_seconds: i32,
}

impl GtfsTime {
    pub fn from_seconds(total_seconds: i32) -> Self {
        Self { total_seconds }
    }

    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        let trimmed = value.trim();
        let parts: Vec<&str> = trimmed.split(':').collect();
        if parts.len() != 3 {
            return Err(GtfsParseError::InvalidTimeFormat(value.to_string()));
        }

        let hours: i32 = parts[0]
            .parse()
            .map_err(|_| GtfsParseError::InvalidTimeFormat(value.to_string()))?;
        let minutes: i32 = parts[1]
            .parse()
            .map_err(|_| GtfsParseError::InvalidTimeFormat(value.to_string()))?;
        let seconds: i32 = parts[2]
            .parse()
            .map_err(|_| GtfsParseError::InvalidTimeFormat(value.to_string()))?;

        if hours < 0 || !(0..=59).contains(&minutes) || !(0..=59).contains(&seconds) {
            return Err(GtfsParseError::InvalidTimeValue(value.to_string()));
        }

        Ok(Self {
            total_seconds: hours * 3600 + minutes * 60 + seconds,
        })
    }

    pub fn total_seconds(&self) -> i32 {
        self.total_seconds
    }

    pub fn hours(&self) -> i32 {
        self.total_seconds / 3600
    }

    pub fn minutes(&self) -> i32 {
        (self.total_seconds % 3600) / 60
    }

    pub fn seconds(&self) -> i32 {
        self.total_seconds % 60
    }
}

impl fmt::Display for GtfsTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}:{:02}:{:02}",
            self.hours(),
            self.minutes(),
            self.seconds()
        )
    }
}

impl Serialize for GtfsTime {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsTimeVisitor;

        impl<'de> Visitor<'de> for GtfsTimeVisitor {
            type Value = GtfsTime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a GTFS time in HH:MM:SS format")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsTime, E> {
                GtfsTime::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsTimeVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GtfsColor {
    rgb: u32,
}

impl GtfsColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self {
            rgb: (r as u32) << 16 | (g as u32) << 8 | (b as u32),
        }
    }

    pub fn parse(value: &str) -> Result<Self, GtfsParseError> {
        let trimmed = value.trim();
        if trimmed.len() != 6 || !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(GtfsParseError::InvalidColorFormat(value.to_string()));
        }

        let rgb = u32::from_str_radix(trimmed, 16)
            .map_err(|_| GtfsParseError::InvalidColorFormat(value.to_string()))?;
        Ok(Self { rgb })
    }

    pub fn rgb(&self) -> u32 {
        self.rgb
    }

    pub fn rec601_luma(&self) -> i32 {
        let r = ((self.rgb >> 16) & 0xFF) as f64;
        let g = ((self.rgb >> 8) & 0xFF) as f64;
        let b = (self.rgb & 0xFF) as f64;
        (0.30 * r + 0.59 * g + 0.11 * b) as i32
    }
}

impl fmt::Display for GtfsColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:06X}", self.rgb)
    }
}

impl Serialize for GtfsColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GtfsColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct GtfsColorVisitor;

        impl<'de> Visitor<'de> for GtfsColorVisitor {
            type Value = GtfsColor;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a 6-digit GTFS color hex string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<GtfsColor, E> {
                GtfsColor::parse(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(GtfsColorVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum LocationType {
    #[serde(rename = "0")]
    StopOrPlatform,
    #[serde(rename = "1")]
    Station,
    #[serde(rename = "2")]
    EntranceOrExit,
    #[serde(rename = "3")]
    GenericNode,
    #[serde(rename = "4")]
    BoardingArea,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum WheelchairBoarding {
    #[serde(rename = "0")]
    NoInfo,
    #[serde(rename = "1")]
    Some,
    #[serde(rename = "2")]
    NotPossible,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteType {
    Tram,
    Subway,
    Rail,
    Bus,
    Ferry,
    CableCar,
    Gondola,
    Funicular,
    Trolleybus,
    Monorail,
    Extended(u16),
    Unknown,
}

impl RouteType {
    fn from_i32(value: i32) -> Self {
        match value {
            0 => RouteType::Tram,
            1 => RouteType::Subway,
            2 => RouteType::Rail,
            3 => RouteType::Bus,
            4 => RouteType::Ferry,
            5 => RouteType::CableCar,
            6 => RouteType::Gondola,
            7 => RouteType::Funicular,
            11 => RouteType::Trolleybus,
            12 => RouteType::Monorail,
            100..=1702 => RouteType::Extended(value as u16),
            _ => RouteType::Unknown,
        }
    }
}

impl<'de> Deserialize<'de> for RouteType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RouteTypeVisitor;

        impl<'de> Visitor<'de> for RouteTypeVisitor {
            type Value = RouteType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a GTFS route_type numeric value")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<RouteType, E> {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Err(E::custom("empty route_type"));
                }
                let parsed: i32 = trimmed.parse().map_err(E::custom)?;
                Ok(RouteType::from_i32(parsed))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<RouteType, E> {
                Ok(RouteType::from_i32(value as i32))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<RouteType, E> {
                Ok(RouteType::from_i32(value as i32))
            }
        }

        deserializer.deserialize_any(RouteTypeVisitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum ContinuousPickupDropOff {
    #[serde(rename = "0")]
    Continuous,
    #[serde(rename = "1")]
    NoContinuous,
    #[serde(rename = "2")]
    MustPhone,
    #[serde(rename = "3")]
    MustCoordinateWithDriver,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum PickupDropOffType {
    #[serde(rename = "0")]
    Regular,
    #[serde(rename = "1")]
    NoPickup,
    #[serde(rename = "2")]
    MustPhone,
    #[serde(rename = "3")]
    MustCoordinateWithDriver,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum BookingType {
    #[serde(rename = "0")]
    Realtime,
    #[serde(rename = "1")]
    SameDay,
    #[serde(rename = "2")]
    PriorDay,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum DirectionId {
    #[serde(rename = "0")]
    Direction0,
    #[serde(rename = "1")]
    Direction1,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum WheelchairAccessible {
    #[serde(rename = "0")]
    NoInfo,
    #[serde(rename = "1")]
    Accessible,
    #[serde(rename = "2")]
    NotAccessible,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum BikesAllowed {
    #[serde(rename = "0")]
    NoInfo,
    #[serde(rename = "1")]
    Allowed,
    #[serde(rename = "2")]
    NotAllowed,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum ServiceAvailability {
    #[default]
    #[serde(rename = "0")]
    Unavailable,
    #[serde(rename = "1")]
    Available,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum ExceptionType {
    #[serde(rename = "1")]
    Added,
    #[serde(rename = "2")]
    Removed,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum PaymentMethod {
    #[serde(rename = "0")]
    OnBoard,
    #[serde(rename = "1")]
    BeforeBoarding,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Transfers {
    #[serde(rename = "0")]
    NoTransfers,
    #[serde(rename = "1")]
    OneTransfer,
    #[serde(rename = "2")]
    TwoTransfers,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum ExactTimes {
    #[serde(rename = "0")]
    FrequencyBased,
    #[serde(rename = "1")]
    ExactTimes,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum TransferType {
    #[serde(rename = "0")]
    Recommended,
    #[serde(rename = "1")]
    Timed,
    #[serde(rename = "2")]
    MinTime,
    #[serde(rename = "3")]
    NoTransfer,
    #[serde(rename = "4")]
    InSeat,
    #[serde(rename = "5")]
    InSeatNotAllowed,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum PathwayMode {
    #[default]
    #[serde(rename = "1")]
    Walkway,
    #[serde(rename = "2")]
    Stairs,
    #[serde(rename = "3")]
    MovingSidewalk,
    #[serde(rename = "4")]
    Escalator,
    #[serde(rename = "5")]
    Elevator,
    #[serde(rename = "6")]
    FareGate,
    #[serde(rename = "7")]
    ExitGate,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Default)]
pub enum Bidirectional {
    #[default]
    #[serde(rename = "0")]
    Unidirectional,
    #[serde(rename = "1")]
    Bidirectional,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum YesNo {
    #[serde(rename = "0")]
    No,
    #[serde(rename = "1")]
    Yes,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Timepoint {
    #[serde(rename = "0")]
    Approximate,
    #[serde(rename = "1")]
    Exact,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum FareMediaType {
    #[serde(rename = "0")]
    NoneType,
    #[serde(rename = "1")]
    PaperTicket,
    #[serde(rename = "2")]
    TransitCard,
    #[serde(rename = "3")]
    ContactlessEmv,
    #[serde(rename = "4")]
    MobileApp,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum DurationLimitType {
    #[serde(rename = "0")]
    DepartureToArrival,
    #[serde(rename = "1")]
    DepartureToDeparture,
    #[serde(rename = "2")]
    ArrivalToDeparture,
    #[serde(rename = "3")]
    ArrivalToArrival,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum FareTransferType {
    #[serde(rename = "0")]
    APlusAb,
    #[serde(rename = "1")]
    APlusAbPlusB,
    #[serde(rename = "2")]
    Ab,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum RiderFareCategory {
    #[serde(rename = "0")]
    NotDefault,
    #[serde(rename = "1")]
    IsDefault,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct Agency {
    pub agency_id: Option<String>,
    pub agency_name: String,
    pub agency_url: String,
    pub agency_timezone: String,
    pub agency_lang: Option<String>,
    pub agency_phone: Option<String>,
    pub agency_fare_url: Option<String>,
    pub agency_email: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct Stop {
    pub stop_id: String,
    pub stop_code: Option<String>,
    pub stop_name: Option<String>,
    pub tts_stop_name: Option<String>,
    pub stop_desc: Option<String>,
    pub stop_lat: Option<f64>,
    pub stop_lon: Option<f64>,
    pub zone_id: Option<String>,
    pub stop_url: Option<String>,
    pub location_type: Option<LocationType>,
    pub parent_station: Option<String>,
    pub stop_timezone: Option<String>,
    pub wheelchair_boarding: Option<WheelchairBoarding>,
    pub level_id: Option<String>,
    pub platform_code: Option<String>,
    pub stop_address: Option<String>,
    pub stop_city: Option<String>,
    pub stop_region: Option<String>,
    pub stop_postcode: Option<String>,
    pub stop_country: Option<String>,
    pub stop_phone: Option<String>,
    pub signposted_as: Option<String>,
    pub vehicle_type: Option<RouteType>,
}

impl Stop {
    pub fn has_coordinates(&self) -> bool {
        self.stop_lat.is_some() && self.stop_lon.is_some()
    }
}


#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub route_id: String,
    pub agency_id: Option<String>,
    pub route_short_name: Option<String>,
    pub route_long_name: Option<String>,
    pub route_desc: Option<String>,
    pub route_type: RouteType,
    pub route_url: Option<String>,
    pub route_color: Option<GtfsColor>,
    pub route_text_color: Option<GtfsColor>,
    pub route_sort_order: Option<u32>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
    pub network_id: Option<String>,
    pub route_branding_url: Option<String>,
    pub checkin_duration: Option<u32>,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            route_id: String::new(),
            agency_id: None,
            route_short_name: None,
            route_long_name: None,
            route_desc: None,
            route_type: RouteType::Bus,
            route_url: None,
            route_color: None,
            route_text_color: None,
            route_sort_order: None,
            continuous_pickup: None,
            continuous_drop_off: None,
            network_id: None,
            route_branding_url: None,
            checkin_duration: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct Trip {
    pub route_id: String,
    pub service_id: String,
    pub trip_id: String,
    pub trip_headsign: Option<String>,
    pub trip_short_name: Option<String>,
    pub direction_id: Option<DirectionId>,
    pub block_id: Option<String>,
    pub shape_id: Option<String>,
    pub wheelchair_accessible: Option<WheelchairAccessible>,
    pub bikes_allowed: Option<BikesAllowed>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct StopTime {
    pub trip_id: String,
    pub arrival_time: Option<GtfsTime>,
    pub departure_time: Option<GtfsTime>,
    pub stop_id: String,
    pub location_group_id: Option<String>,
    pub location_id: Option<String>,
    pub stop_sequence: u32,
    pub stop_headsign: Option<String>,
    pub pickup_type: Option<PickupDropOffType>,
    pub drop_off_type: Option<PickupDropOffType>,
    pub pickup_booking_rule_id: Option<String>,
    pub drop_off_booking_rule_id: Option<String>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
    pub shape_dist_traveled: Option<f64>,
    pub timepoint: Option<Timepoint>,
    pub start_pickup_drop_off_window: Option<GtfsTime>,
    pub end_pickup_drop_off_window: Option<GtfsTime>,
    pub stop_direction_name: Option<String>,
}



#[derive(Debug, Clone, Deserialize)]
pub struct BookingRules {
    pub booking_rule_id: String,
    pub booking_type: BookingType,
    pub prior_notice_duration_min: Option<i32>,
    pub prior_notice_duration_max: Option<i32>,
    pub prior_notice_start_day: Option<i32>,
    pub prior_notice_start_time: Option<GtfsTime>,
    pub prior_notice_last_day: Option<i32>,
    pub prior_notice_last_time: Option<GtfsTime>,
    pub prior_notice_service_id: Option<String>,
    pub message: Option<String>,
    pub pickup_message: Option<String>,
    pub drop_off_message: Option<String>,
    pub phone_number: Option<String>,
    pub info_url: Option<String>,
    pub booking_url: Option<String>,
}

impl Default for BookingRules {
    fn default() -> Self {
        Self {
            booking_rule_id: String::new(),
            booking_type: BookingType::Other,
            prior_notice_duration_min: None,
            prior_notice_duration_max: None,
            prior_notice_start_day: None,
            prior_notice_start_time: None,
            prior_notice_last_day: None,
            prior_notice_last_time: None,
            prior_notice_service_id: None,
            message: None,
            pickup_message: None,
            drop_off_message: None,
            phone_number: None,
            info_url: None,
            booking_url: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Calendar {
    pub service_id: String,
    pub monday: ServiceAvailability,
    pub tuesday: ServiceAvailability,
    pub wednesday: ServiceAvailability,
    pub thursday: ServiceAvailability,
    pub friday: ServiceAvailability,
    pub saturday: ServiceAvailability,
    pub sunday: ServiceAvailability,
    pub start_date: GtfsDate,
    pub end_date: GtfsDate,
}

impl Default for Calendar {
    fn default() -> Self {
        Self {
            service_id: String::new(),
            monday: ServiceAvailability::Unavailable,
            tuesday: ServiceAvailability::Unavailable,
            wednesday: ServiceAvailability::Unavailable,
            thursday: ServiceAvailability::Unavailable,
            friday: ServiceAvailability::Unavailable,
            saturday: ServiceAvailability::Unavailable,
            sunday: ServiceAvailability::Unavailable,
            start_date: GtfsDate {
                year: 0,
                month: 1,
                day: 1,
            },
            end_date: GtfsDate {
                year: 0,
                month: 1,
                day: 1,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CalendarDate {
    pub service_id: String,
    pub date: GtfsDate,
    pub exception_type: ExceptionType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareAttribute {
    pub fare_id: String,
    pub price: f64,
    pub currency_type: String,
    pub payment_method: PaymentMethod,
    pub transfers: Option<Transfers>,
    pub agency_id: Option<String>,
    pub transfer_duration: Option<u32>,
    pub ic_price: Option<f64>,
}

impl Default for FareAttribute {
    fn default() -> Self {
        Self {
            fare_id: String::new(),
            price: 0.0,
            currency_type: String::new(),
            payment_method: PaymentMethod::OnBoard,
            transfers: None,
            agency_id: None,
            transfer_duration: None,
            ic_price: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct FareRule {
    pub fare_id: String,
    pub route_id: Option<String>,
    pub origin_id: Option<String>,
    pub destination_id: Option<String>,
    pub contains_id: Option<String>,
    pub contains_route_id: Option<String>,
}


#[derive(Debug, Clone, Deserialize, Default)]
pub struct Shape {
    pub shape_id: String,
    pub shape_pt_lat: f64,
    pub shape_pt_lon: f64,
    pub shape_pt_sequence: u32,
    pub shape_dist_traveled: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Frequency {
    pub trip_id: String,
    pub start_time: GtfsTime,
    pub end_time: GtfsTime,
    pub headway_secs: u32,
    pub exact_times: Option<ExactTimes>,
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct Transfer {
    pub from_stop_id: Option<String>,
    pub to_stop_id: Option<String>,
    pub transfer_type: Option<TransferType>,
    pub min_transfer_time: Option<u32>,
    pub from_route_id: Option<String>,
    pub to_route_id: Option<String>,
    pub from_trip_id: Option<String>,
    pub to_trip_id: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
pub struct Area {
    pub area_id: String,
    pub area_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StopArea {
    pub area_id: String,
    pub stop_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Timeframe {
    pub timeframe_group_id: Option<String>,
    pub start_time: Option<GtfsTime>,
    pub end_time: Option<GtfsTime>,
    pub service_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareMedia {
    pub fare_media_id: String,
    pub fare_media_name: Option<String>,
    pub fare_media_type: FareMediaType,
}

impl Default for FareMedia {
    fn default() -> Self {
        Self {
            fare_media_id: String::new(),
            fare_media_name: None,
            fare_media_type: FareMediaType::NoneType,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareProduct {
    pub fare_product_id: String,
    pub fare_product_name: Option<String>,
    pub amount: f64,
    pub currency: String,
    pub fare_media_id: Option<String>,
    pub rider_category_id: Option<String>,
}

impl Default for FareProduct {
    fn default() -> Self {
        Self {
            fare_product_id: String::new(),
            fare_product_name: None,
            amount: 0.0,
            currency: String::new(),
            fare_media_id: None,
            rider_category_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct FareLegRule {
    pub leg_group_id: Option<String>,
    pub network_id: Option<String>,
    pub from_area_id: Option<String>,
    pub to_area_id: Option<String>,
    pub from_timeframe_group_id: Option<String>,
    pub to_timeframe_group_id: Option<String>,
    pub fare_product_id: String,
    pub rule_priority: Option<u32>,
}


#[derive(Debug, Clone, Deserialize)]
pub struct FareTransferRule {
    pub from_leg_group_id: Option<String>,
    pub to_leg_group_id: Option<String>,
    pub duration_limit: Option<i32>,
    pub duration_limit_type: Option<DurationLimitType>,
    pub fare_transfer_type: FareTransferType,
    pub transfer_count: Option<i32>,
    pub fare_product_id: Option<String>,
}

impl Default for FareTransferRule {
    fn default() -> Self {
        Self {
            from_leg_group_id: None,
            to_leg_group_id: None,
            duration_limit: None,
            duration_limit_type: None,
            fare_transfer_type: FareTransferType::APlusAb,
            transfer_count: None,
            fare_product_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct FareLegJoinRule {
    pub from_network_id: String,
    pub to_network_id: String,
    pub from_stop_id: Option<String>,
    pub to_stop_id: Option<String>,
    pub from_area_id: Option<String>,
    pub to_area_id: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct RiderCategory {
    pub rider_category_id: String,
    pub rider_category_name: String,
    #[serde(rename = "is_default_category")]
    pub is_default_fare_category: Option<RiderFareCategory>,
    pub eligibility_url: Option<String>,
}


#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocationGroup {
    pub location_group_id: String,
    pub location_group_name: Option<String>,
    pub location_group_desc: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocationGroupStop {
    pub location_group_id: String,
    pub stop_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[derive(Default)]
pub struct Network {
    pub network_id: String,
    pub network_name: Option<String>,
}


#[derive(Debug, Clone, Deserialize)]
pub struct RouteNetwork {
    pub route_id: String,
    pub network_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedInfo {
    pub feed_publisher_name: String,
    pub feed_publisher_url: String,
    pub feed_lang: String,
    pub feed_start_date: Option<GtfsDate>,
    pub feed_end_date: Option<GtfsDate>,
    pub feed_version: Option<String>,
    pub feed_contact_email: Option<String>,
    pub feed_contact_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attribution {
    pub attribution_id: Option<String>,
    pub agency_id: Option<String>,
    pub route_id: Option<String>,
    pub trip_id: Option<String>,
    pub organization_name: String,
    pub is_producer: Option<YesNo>,
    pub is_operator: Option<YesNo>,
    pub is_authority: Option<YesNo>,
    pub attribution_url: Option<String>,
    pub attribution_email: Option<String>,
    pub attribution_phone: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Level {
    pub level_id: String,
    pub level_index: f64,
    pub level_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pathway {
    pub pathway_id: String,
    pub from_stop_id: String,
    pub to_stop_id: String,
    pub pathway_mode: PathwayMode,
    pub is_bidirectional: Bidirectional,
    pub length: Option<f64>,
    pub traversal_time: Option<u32>,
    pub stair_count: Option<u32>,
    pub max_slope: Option<f64>,
    pub min_width: Option<f64>,
    pub signposted_as: Option<String>,
    pub reversed_signposted_as: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Translation {
    pub table_name: String,
    pub field_name: String,
    pub language: String,
    pub translation: String,
    pub record_id: Option<String>,
    pub record_sub_id: Option<String>,
    pub field_value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gtfs_date() {
        let date = GtfsDate::parse("20240131").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 31);
        assert_eq!(date.to_string(), "20240131");
    }

    #[test]
    fn parses_gtfs_date_with_whitespace() {
        let date = GtfsDate::parse(" 20240131 ").unwrap();
        assert_eq!(date.to_string(), "20240131");
    }

    #[test]
    fn rejects_invalid_date() {
        assert!(GtfsDate::parse("20240230").is_err());
        assert!(GtfsDate::parse("2024-01-01").is_err());
    }

    #[test]
    fn parses_gtfs_time() {
        let time = GtfsTime::parse("25:10:05").unwrap();
        assert_eq!(time.total_seconds(), 25 * 3600 + 10 * 60 + 5);
        assert_eq!(time.to_string(), "25:10:05");
    }

    #[test]
    fn parses_gtfs_time_with_whitespace() {
        let time = GtfsTime::parse(" 25:10:05 ").unwrap();
        assert_eq!(time.to_string(), "25:10:05");
    }

    #[test]
    fn rejects_invalid_time() {
        assert!(GtfsTime::parse("25:99:00").is_err());
        assert!(GtfsTime::parse("bad").is_err());
    }

    #[test]
    fn parses_gtfs_color() {
        let color = GtfsColor::parse("FF00AA").unwrap();
        assert_eq!(color.rgb(), 0xFF00AA);
        assert_eq!(color.to_string(), "FF00AA");
    }

    #[test]
    fn parses_gtfs_color_with_whitespace() {
        let color = GtfsColor::parse(" ff00aa ").unwrap();
        assert_eq!(color.rgb(), 0xFF00AA);
    }

    #[test]
    fn rejects_invalid_color() {
        assert!(GtfsColor::parse("GG00AA").is_err());
        assert!(GtfsColor::parse("12345").is_err());
    }
}
