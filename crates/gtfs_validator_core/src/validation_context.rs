use std::cell::{Cell, RefCell};

use chrono::NaiveDate;
#[cfg(not(target_arch = "wasm32"))]
use chrono::Utc;

thread_local! {
    static VALIDATION_DATE: Cell<Option<NaiveDate>> = const { Cell::new(None) };
    static VALIDATION_COUNTRY_CODE: RefCell<Option<String>> = const { RefCell::new(None) };
    static GOOGLE_RULES_ENABLED: Cell<bool> = const { Cell::new(false) };
    static THOROUGH_MODE: Cell<bool> = const { Cell::new(false) };
    static NOTICE_GROUP_LIMIT: Cell<Option<usize>> = const { Cell::new(None) };
}

pub struct ValidationDateGuard {
    previous: Option<NaiveDate>,
}

impl Drop for ValidationDateGuard {
    fn drop(&mut self) {
        VALIDATION_DATE.with(|cell| cell.set(self.previous));
    }
}

pub fn set_validation_date(date: Option<NaiveDate>) -> ValidationDateGuard {
    let previous = VALIDATION_DATE.with(|cell| {
        let previous = cell.get();
        cell.set(date);
        previous
    });
    ValidationDateGuard { previous }
}

pub fn validation_date() -> NaiveDate {
    VALIDATION_DATE.with(|cell| {
        cell.get().unwrap_or_else(|| {
            #[cfg(target_arch = "wasm32")]
            {
                // Fallback for WASM where Utc::now() might panic without wasm-bindgen feature
                chrono::NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                Utc::now().date_naive()
            }
        })
    })
}

pub struct ValidationCountryCodeGuard {
    previous: Option<String>,
}

impl Drop for ValidationCountryCodeGuard {
    fn drop(&mut self) {
        VALIDATION_COUNTRY_CODE.with(|cell| {
            *cell.borrow_mut() = self.previous.take();
        });
    }
}

pub fn set_validation_country_code(code: Option<String>) -> ValidationCountryCodeGuard {
    let previous = VALIDATION_COUNTRY_CODE.with(|cell| {
        let previous = cell.borrow().clone();
        *cell.borrow_mut() = code;
        previous
    });
    ValidationCountryCodeGuard { previous }
}

pub fn validation_country_code() -> Option<String> {
    VALIDATION_COUNTRY_CODE.with(|cell| cell.borrow().clone())
}

pub struct ValidationGoogleRulesGuard {
    previous: bool,
}

impl Drop for ValidationGoogleRulesGuard {
    fn drop(&mut self) {
        GOOGLE_RULES_ENABLED.with(|cell| cell.set(self.previous));
    }
}

pub fn set_google_rules_enabled(enabled: bool) -> ValidationGoogleRulesGuard {
    let previous = GOOGLE_RULES_ENABLED.with(|cell| {
        let previous = cell.get();
        cell.set(enabled);
        previous
    });
    ValidationGoogleRulesGuard { previous }
}

pub fn google_rules_enabled() -> bool {
    GOOGLE_RULES_ENABLED.with(|cell| cell.get())
}

pub struct ThoroughModeGuard {
    previous: bool,
}

impl Drop for ThoroughModeGuard {
    fn drop(&mut self) {
        THOROUGH_MODE.with(|cell| cell.set(self.previous));
    }
}

pub fn set_thorough_mode_enabled(enabled: bool) -> ThoroughModeGuard {
    let previous = THOROUGH_MODE.with(|cell| {
        let previous = cell.get();
        cell.set(enabled);
        previous
    });
    ThoroughModeGuard { previous }
}

pub fn thorough_mode_enabled() -> bool {
    THOROUGH_MODE.with(|cell| cell.get())
}

pub struct NoticeGroupLimitGuard {
    previous: Option<usize>,
}

impl Drop for NoticeGroupLimitGuard {
    fn drop(&mut self) {
        NOTICE_GROUP_LIMIT.with(|cell| cell.set(self.previous));
    }
}

/// Caps how many notices a `NoticeContainer` stores per (code, severity)
/// group. Exact totals are still tracked for every group; only the stored
/// samples are bounded. `None` (the default) stores everything.
pub fn set_notice_group_limit(limit: Option<usize>) -> NoticeGroupLimitGuard {
    let previous = NOTICE_GROUP_LIMIT.with(|cell| {
        let previous = cell.get();
        cell.set(limit);
        previous
    });
    NoticeGroupLimitGuard { previous }
}

pub fn notice_group_limit() -> Option<usize> {
    NOTICE_GROUP_LIMIT.with(|cell| cell.get())
}

#[derive(Clone, Debug)]
pub struct ValidationContextState {
    pub date: NaiveDate,
    pub country_code: Option<String>,
    pub google_rules: bool,
    pub thorough_mode: bool,
    pub notice_group_limit: Option<usize>,
}

// Ensure it is Send + Sync (NaiveDate is Copy/Send/Sync, String is Send/Sync)
unsafe impl Send for ValidationContextState {}
unsafe impl Sync for ValidationContextState {}

impl ValidationContextState {
    pub fn capture() -> Self {
        Self {
            date: validation_date(),
            country_code: validation_country_code(),
            google_rules: google_rules_enabled(),
            thorough_mode: thorough_mode_enabled(),
            notice_group_limit: notice_group_limit(),
        }
    }

    pub fn apply(
        &self,
    ) -> (
        ValidationDateGuard,
        ValidationCountryCodeGuard,
        ValidationGoogleRulesGuard,
        ThoroughModeGuard,
        NoticeGroupLimitGuard,
    ) {
        (
            set_validation_date(Some(self.date)),
            set_validation_country_code(self.country_code.clone()),
            set_google_rules_enabled(self.google_rules),
            set_thorough_mode_enabled(self.thorough_mode),
            set_notice_group_limit(self.notice_group_limit),
        )
    }
}
