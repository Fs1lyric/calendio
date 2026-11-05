//! Times of day in the system's clock format: "9 AM" or "09:00".

use chrono::{NaiveTime, Timelike};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    H12,
    H24,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn the_locale_time_format_decides_the_clock() {
        for twelve in ["%r", "%I:%M:%S %p", "%l:%M %P", "%I:%M %p"] {
            assert_eq!(Clock::from_time_format(twelve), Clock::H12, "{twelve}");
        }
        for day in ["%T", "%H:%M:%S", "%H.%M.%S", ""] {
            assert_eq!(Clock::from_time_format(day), Clock::H24, "{day:?}");
        }
    }

    #[test]
    fn gutter_hours_read_like_apple_calendar() {
        assert_eq!(Clock::H12.hour(9), "9 AM");
        assert_eq!(Clock::H12.hour(12), "Noon");
        assert_eq!(Clock::H12.hour(13), "1 PM");
        assert_eq!(Clock::H12.hour(0), "12 AM");
        assert_eq!(Clock::H24.hour(9), "09:00");
        assert_eq!(Clock::H24.hour(21), "21:00");
    }

    #[test]
    fn times_drop_zero_minutes_in_twelve_hour() {
        assert_eq!(Clock::H12.short(t(9, 0)), "9 AM");
        assert_eq!(Clock::H12.short(t(9, 30)), "9:30 AM");
        assert_eq!(Clock::H12.short(t(0, 15)), "12:15 AM");
        assert_eq!(Clock::H24.short(t(9, 0)), "09:00");
        assert_eq!(Clock::H24.short(t(21, 5)), "21:05");
        assert_eq!(Clock::H12.bare(t(14, 41)), "2:41");
        assert_eq!(Clock::H24.bare(t(14, 41)), "14:41");
    }

    #[test]
    fn ranges_share_a_suffix_when_they_can() {
        assert_eq!(Clock::H12.range(t(9, 30), t(10, 30)), "9:30 – 10:30 AM");
        assert_eq!(Clock::H12.range(t(11, 0), t(13, 0)), "11 AM – 1 PM");
        assert_eq!(Clock::H24.range(t(9, 30), t(10, 30)), "09:30 – 10:30");
    }
}
