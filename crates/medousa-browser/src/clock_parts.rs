//! Split a millisecond timestamp into the `(secs, nanos)` layout of
//! `std::time::Duration` on `wasm32-unknown-unknown`.
#![allow(dead_code)]

pub fn duration_parts_from_ms(ms: f64) -> (u64, u32) {
    if !ms.is_finite() || ms <= 0.0 {
        return (0, 0);
    }
    let secs = (ms / 1000.0) as u64;
    let frac = ms - (secs as f64) * 1000.0;
    let mut nanos = (frac * 1_000_000.0).round();
    if !nanos.is_finite() || nanos < 0.0 {
        nanos = 0.0;
    }
    if nanos >= 1_000_000_000.0 {
        return (secs.saturating_add(1), 0);
    }
    (secs, nanos as u32)
}

pub fn saturating_between(later: (u64, u32), earlier: (u64, u32)) -> (u64, u32) {
    let (later_secs, later_nanos) = later;
    let (earlier_secs, earlier_nanos) = earlier;
    if later_secs < earlier_secs || (later_secs == earlier_secs && later_nanos < earlier_nanos) {
        return (0, 0);
    }
    if later_nanos >= earlier_nanos {
        (later_secs - earlier_secs, later_nanos - earlier_nanos)
    } else {
        (
            later_secs - earlier_secs - 1,
            later_nanos + 1_000_000_000 - earlier_nanos,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{duration_parts_from_ms, saturating_between};

    #[test]
    fn splits_fractional_milliseconds() {
        assert_eq!(duration_parts_from_ms(1500.5), (1, 500_500_000));
    }

    #[test]
    fn carries_a_rounded_nanosecond_overflow() {
        assert_eq!(duration_parts_from_ms(1999.9999996), (2, 0));
    }

    #[test]
    fn rejects_non_finite_input() {
        assert_eq!(duration_parts_from_ms(f64::NAN), (0, 0));
        assert_eq!(duration_parts_from_ms(-5.0), (0, 0));
    }

    #[test]
    fn subtracts_with_a_nanosecond_borrow() {
        assert_eq!(saturating_between((2, 1), (1, 2)), (0, 999_999_999));
    }

    #[test]
    fn saturates_when_the_earlier_instant_is_later() {
        assert_eq!(saturating_between((1, 0), (2, 0)), (0, 0));
    }
}
