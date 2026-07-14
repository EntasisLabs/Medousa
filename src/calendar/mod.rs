//! Personal calendar store backed by vault `.ics` files (RFC 5545).

mod service;

pub use service::{CalendarService, DEFAULT_CALENDAR_PATH};
