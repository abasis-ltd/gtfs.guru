use std::cell::{Cell, RefCell};

use chrono::{NaiveDate, Utc};

thread_local! {
    static VALIDATION_DATE: Cell<Option<NaiveDate>> = const { Cell::new(None) };
    static VALIDATION_COUNTRY_CODE: RefCell<Option<String>> = const { RefCell::new(None) };
    static GOOGLE_RULES_ENABLED: Cell<bool> = const { Cell::new(false) };
    static THOROUGH_MODE: Cell<bool> = const { Cell::new(false) };
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
