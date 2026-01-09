use compact_str::CompactString;
use std::fmt;

use chrono::NaiveDate;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StringId(pub u32);

impl fmt::Display for StringId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = RESOLVER_HOOK.with(|hook| {
            if let Some(ref func) = *hook.borrow() {
                func(*self)
            } else {
                format!("StringId({})", self.0)
            }
        });
        write!(f, "{}", s)
    }
}

type InternerHook = Box<dyn Fn(&str) -> StringId>;
type ResolverHook = Box<dyn Fn(StringId) -> String>;

thread_local! {
    static INTERNER_HOOK: RefCell<Option<InternerHook>> = RefCell::new(None);
    static RESOLVER_HOOK: RefCell<Option<ResolverHook>> = RefCell::new(None);
}

pub fn set_thread_local_interner<F>(f: F)
where
    F: Fn(&str) -> StringId + 'static,
{
    INTERNER_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(f));
    });
}

pub fn set_thread_local_resolver<F>(f: F)
where
    F: Fn(StringId) -> String + 'static,
{
    RESOLVER_HOOK.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(f));
    });
}

pub fn clear_thread_local_hooks() {
    INTERNER_HOOK.with(|hook| {
        *hook.borrow_mut() = None;
    });
    RESOLVER_HOOK.with(|hook| {
        *hook.borrow_mut() = None;
    });
}

impl Serialize for StringId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = RESOLVER_HOOK.with(|hook| {
            if let Some(ref f) = *hook.borrow() {
                f(*self)
            } else {
                format!("StringId({})", self.0)
            }
        });
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for StringId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringIdVisitor;

        impl<'de> Visitor<'de> for StringIdVisitor {
            type Value = StringId;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string to be interned")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<StringId, E> {
                INTERNER_HOOK.with(|hook| {
                    if let Some(ref f) = *hook.borrow() {
                        Ok(f(value))
                    } else {
                        // Fallback or error? For now, let's treat it as a bug if hook is missing
                        // but maybe we can just return a dummy ID if we don't care about interning here.
                        // However, the goal is always to intern during load.
                        Ok(StringId(0))
                    }
                })
            }
        }

        deserializer.deserialize_str(StringIdVisitor)
    }
}

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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Agency {
    pub agency_id: Option<StringId>,
    pub agency_name: CompactString,
    pub agency_url: StringId,
    pub agency_timezone: StringId,
    pub agency_lang: Option<StringId>,
    pub agency_phone: Option<CompactString>,
    pub agency_fare_url: Option<StringId>,
    pub agency_email: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Stop {
    pub stop_id: StringId,
    pub stop_code: Option<CompactString>,
    pub stop_name: Option<CompactString>,
    pub tts_stop_name: Option<CompactString>,
    pub stop_desc: Option<CompactString>,
    pub stop_lat: Option<f64>,
    pub stop_lon: Option<f64>,
    pub zone_id: Option<StringId>,
    pub stop_url: Option<StringId>,
    pub location_type: Option<LocationType>,
    pub parent_station: Option<StringId>,
    pub stop_timezone: Option<StringId>,
    pub wheelchair_boarding: Option<WheelchairBoarding>,
    pub level_id: Option<StringId>,
    pub platform_code: Option<CompactString>,
    pub stop_address: Option<CompactString>,
    pub stop_city: Option<CompactString>,
    pub stop_region: Option<CompactString>,
    pub stop_postcode: Option<CompactString>,
    pub stop_country: Option<CompactString>,
    pub stop_phone: Option<CompactString>,
    pub signposted_as: Option<CompactString>,
    pub vehicle_type: Option<RouteType>,
}

impl Stop {
    pub fn has_coordinates(&self) -> bool {
        self.stop_lat.is_some() && self.stop_lon.is_some()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub route_id: StringId,
    pub agency_id: Option<StringId>,
    pub route_short_name: Option<CompactString>,
    pub route_long_name: Option<CompactString>,
    pub route_desc: Option<CompactString>,
    pub route_type: RouteType,
    pub route_url: Option<StringId>,
    pub route_color: Option<GtfsColor>,
    pub route_text_color: Option<GtfsColor>,
    pub route_sort_order: Option<u32>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
    pub network_id: Option<StringId>,
    pub route_branding_url: Option<StringId>,
    pub checkin_duration: Option<u32>,
}

impl Default for Route {
    fn default() -> Self {
        Self {
            route_id: StringId::default(),
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Trip {
    pub route_id: StringId,
    pub service_id: StringId,
    pub trip_id: StringId,
    pub trip_headsign: Option<CompactString>,
    pub trip_short_name: Option<CompactString>,
    pub direction_id: Option<DirectionId>,
    pub block_id: Option<StringId>,
    pub shape_id: Option<StringId>,
    pub wheelchair_accessible: Option<WheelchairAccessible>,
    pub bikes_allowed: Option<BikesAllowed>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StopTime {
    pub trip_id: StringId,
    pub arrival_time: Option<GtfsTime>,
    pub departure_time: Option<GtfsTime>,
    pub stop_id: StringId,
    pub location_group_id: Option<StringId>,
    pub location_id: Option<StringId>,
    pub stop_sequence: u32,
    pub stop_headsign: Option<CompactString>,
    pub pickup_type: Option<PickupDropOffType>,
    pub drop_off_type: Option<PickupDropOffType>,
    pub pickup_booking_rule_id: Option<StringId>,
    pub drop_off_booking_rule_id: Option<StringId>,
    pub continuous_pickup: Option<ContinuousPickupDropOff>,
    pub continuous_drop_off: Option<ContinuousPickupDropOff>,
    pub shape_dist_traveled: Option<f64>,
    pub timepoint: Option<Timepoint>,
    pub start_pickup_drop_off_window: Option<GtfsTime>,
    pub end_pickup_drop_off_window: Option<GtfsTime>,
    pub stop_direction_name: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BookingRules {
    pub booking_rule_id: StringId,
    pub booking_type: BookingType,
    pub prior_notice_duration_min: Option<i32>,
    pub prior_notice_duration_max: Option<i32>,
    pub prior_notice_start_day: Option<i32>,
    pub prior_notice_start_time: Option<GtfsTime>,
    pub prior_notice_last_day: Option<i32>,
    pub prior_notice_last_time: Option<GtfsTime>,
    pub prior_notice_service_id: Option<StringId>,
    pub message: Option<CompactString>,
    pub pickup_message: Option<CompactString>,
    pub drop_off_message: Option<CompactString>,
    pub phone_number: Option<CompactString>,
    pub info_url: Option<StringId>,
    pub booking_url: Option<StringId>,
}

impl Default for BookingRules {
    fn default() -> Self {
        Self {
            booking_rule_id: StringId::default(),
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
    pub service_id: StringId,
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
            service_id: StringId::default(),
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
    pub service_id: StringId,
    pub date: GtfsDate,
    pub exception_type: ExceptionType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareAttribute {
    pub fare_id: StringId,
    pub price: f64,
    pub currency_type: StringId,
    pub payment_method: PaymentMethod,
    pub transfers: Option<Transfers>,
    pub agency_id: Option<StringId>,
    pub transfer_duration: Option<u32>,
    pub ic_price: Option<f64>,
}

impl Default for FareAttribute {
    fn default() -> Self {
        Self {
            fare_id: StringId::default(),
            price: 0.0,
            currency_type: StringId::default(),
            payment_method: PaymentMethod::OnBoard,
            transfers: None,
            agency_id: None,
            transfer_duration: None,
            ic_price: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FareRule {
    pub fare_id: StringId,
    pub route_id: Option<StringId>,
    pub origin_id: Option<StringId>,
    pub destination_id: Option<StringId>,
    pub contains_id: Option<StringId>,
    pub contains_route_id: Option<StringId>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Shape {
    pub shape_id: StringId,
    pub shape_pt_lat: f64,
    pub shape_pt_lon: f64,
    pub shape_pt_sequence: u32,
    pub shape_dist_traveled: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Frequency {
    pub trip_id: StringId,
    pub start_time: GtfsTime,
    pub end_time: GtfsTime,
    pub headway_secs: u32,
    pub exact_times: Option<ExactTimes>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Transfer {
    pub from_stop_id: Option<StringId>,
    pub to_stop_id: Option<StringId>,
    pub transfer_type: Option<TransferType>,
    pub min_transfer_time: Option<u32>,
    pub from_route_id: Option<StringId>,
    pub to_route_id: Option<StringId>,
    pub from_trip_id: Option<StringId>,
    pub to_trip_id: Option<StringId>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Area {
    pub area_id: StringId,
    pub area_name: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StopArea {
    pub area_id: StringId,
    pub stop_id: StringId,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Timeframe {
    pub timeframe_group_id: Option<StringId>,
    pub start_time: Option<GtfsTime>,
    pub end_time: Option<GtfsTime>,
    pub service_id: StringId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareMedia {
    pub fare_media_id: StringId,
    pub fare_media_name: Option<CompactString>,
    pub fare_media_type: FareMediaType,
}

impl Default for FareMedia {
    fn default() -> Self {
        Self {
            fare_media_id: StringId::default(),
            fare_media_name: None,
            fare_media_type: FareMediaType::NoneType,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareProduct {
    pub fare_product_id: StringId,
    pub fare_product_name: Option<CompactString>,
    pub amount: f64,
    pub currency: StringId,
    pub fare_media_id: Option<StringId>,
    pub rider_category_id: Option<StringId>,
}

impl Default for FareProduct {
    fn default() -> Self {
        Self {
            fare_product_id: StringId::default(),
            fare_product_name: None,
            amount: 0.0,
            currency: StringId::default(),
            fare_media_id: None,
            rider_category_id: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FareLegRule {
    pub leg_group_id: Option<StringId>,
    pub network_id: Option<StringId>,
    pub from_area_id: Option<StringId>,
    pub to_area_id: Option<StringId>,
    pub from_timeframe_group_id: Option<StringId>,
    pub to_timeframe_group_id: Option<StringId>,
    pub fare_product_id: StringId,
    pub rule_priority: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FareTransferRule {
    pub from_leg_group_id: Option<StringId>,
    pub to_leg_group_id: Option<StringId>,
    pub duration_limit: Option<i32>,
    pub duration_limit_type: Option<DurationLimitType>,
    pub fare_transfer_type: FareTransferType,
    pub transfer_count: Option<i32>,
    pub fare_product_id: Option<StringId>,
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

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FareLegJoinRule {
    pub from_network_id: StringId,
    pub to_network_id: StringId,
    pub from_stop_id: Option<StringId>,
    pub to_stop_id: Option<StringId>,
    pub from_area_id: Option<StringId>,
    pub to_area_id: Option<StringId>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RiderCategory {
    pub rider_category_id: StringId,
    pub rider_category_name: CompactString,
    #[serde(rename = "is_default_category")]
    pub is_default_fare_category: Option<RiderFareCategory>,
    pub eligibility_url: Option<StringId>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocationGroup {
    pub location_group_id: StringId,
    pub location_group_name: Option<CompactString>,
    pub location_group_desc: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LocationGroupStop {
    pub location_group_id: StringId,
    pub stop_id: StringId,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Network {
    pub network_id: StringId,
    pub network_name: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RouteNetwork {
    pub route_id: StringId,
    pub network_id: StringId,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedInfo {
    pub feed_publisher_name: CompactString,
    pub feed_publisher_url: StringId,
    pub feed_lang: StringId,
    pub feed_start_date: Option<GtfsDate>,
    pub feed_end_date: Option<GtfsDate>,
    pub feed_version: Option<CompactString>,
    pub feed_contact_email: Option<CompactString>,
    pub feed_contact_url: Option<StringId>,
    pub default_lang: Option<StringId>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Attribution {
    pub attribution_id: Option<StringId>,
    pub agency_id: Option<StringId>,
    pub route_id: Option<StringId>,
    pub trip_id: Option<StringId>,
    pub organization_name: StringId,
    pub is_producer: Option<YesNo>,
    pub is_operator: Option<YesNo>,
    pub is_authority: Option<YesNo>,
    pub attribution_url: Option<StringId>,
    pub attribution_email: Option<CompactString>,
    pub attribution_phone: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Level {
    pub level_id: StringId,
    pub level_index: f64,
    pub level_name: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Pathway {
    pub pathway_id: StringId,
    pub from_stop_id: StringId,
    pub to_stop_id: StringId,
    pub pathway_mode: PathwayMode,
    pub is_bidirectional: Bidirectional,
    pub length: Option<f64>,
    pub traversal_time: Option<u32>,
    pub stair_count: Option<u32>,
    pub max_slope: Option<f64>,
    pub min_width: Option<f64>,
    pub signposted_as: Option<CompactString>,
    pub reversed_signposted_as: Option<CompactString>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Translation {
    pub table_name: Option<StringId>,
    pub field_name: Option<StringId>,
    #[serde(alias = "lang")]
    pub language: StringId,
    pub translation: CompactString,
    pub record_id: Option<StringId>,
    pub record_sub_id: Option<StringId>,
    #[serde(alias = "trans_id")]
    pub field_value: Option<CompactString>,
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
