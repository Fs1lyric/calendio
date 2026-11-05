//! Times of day in the system's clock format: "9 AM" or "09:00".

use chrono::{NaiveTime, Timelike};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    H12,
    H24,
}

impl Clock {
    /// `CALENDIO_CLOCK=12` or `24` wins; otherwise the locale's time format.
    /// Call it once from `main`, before any other thread starts.
    pub fn system() -> Self {
        match std::env::var("CALENDIO_CLOCK").as_deref() {
            Ok("12") => Self::H12,
            Ok("24") => Self::H24,
            _ => Self::from_time_format(&locale_time_format()),
        }
    }

    /// A strftime time format with an AM/PM or 12-hour field means a 12-hour clock.
    pub fn from_time_format(format: &str) -> Self {
        if ["%p", "%P", "%r", "%I", "%l"].iter().any(|field| format.contains(field)) {
            Self::H12
        } else {
            Self::H24
        }
    }

    /// A gutter label: "9 AM", "Noon", or "09:00".
    pub fn hour(self, hour: u32) -> String {
        match (self, hour) {
            (Self::H24, h) => format!("{h:02}:00"),
            (Self::H12, 12) => "Noon".into(),
            (Self::H12, h) => format!("{} {}", twelve(h), suffix(h)),
        }
    }

    /// "9 AM", "9:30 AM", or "09:30".
    pub fn short(self, t: NaiveTime) -> String {
        match self {
            Self::H24 => self.bare(t),
            Self::H12 => format!("{} {}", self.bare_12(t), suffix(t.hour())),
        }
    }

    /// The time without AM/PM: "2:41" or "14:41".
    pub fn bare(self, t: NaiveTime) -> String {
        match self {
            Self::H24 => format!("{:02}:{:02}", t.hour(), t.minute()),
            Self::H12 => format!("{}:{:02}", twelve(t.hour()), t.minute()),
        }
    }

    /// "9:30 – 10:30 AM", "11 AM – 1 PM", or "09:30 – 10:30".
    pub fn range(self, a: NaiveTime, b: NaiveTime) -> String {
        match self {
            Self::H24 => format!("{} – {}", self.bare(a), self.bare(b)),
            Self::H12 if suffix(a.hour()) == suffix(b.hour()) => {
                format!("{} – {}", self.bare_12(a), self.short(b))
            }
            Self::H12 => format!("{} – {}", self.short(a), self.short(b)),
        }
    }

    /// "9" on the hour, "9:30" otherwise.
    fn bare_12(self, t: NaiveTime) -> String {
        if t.minute() == 0 {
            twelve(t.hour()).to_string()
        } else {
            self.bare(t)
        }
    }
}

fn twelve(hour: u32) -> u32 {
    match hour % 12 {
        0 => 12,
        h => h,
    }
}

fn suffix(hour: u32) -> &'static str {
    if hour < 12 {
        "AM"
    } else {
        "PM"
    }
}

/// The locale's `T_FMT`, read through libc with LC_TIME switched on only for the read.
#[cfg(unix)]
fn locale_time_format() -> String {
    // SAFETY: `system` runs once from `main` before other threads exist, so nothing else
    // touches the locale meanwhile; the returned string is copied before it is reset.
    unsafe {
        if libc::setlocale(libc::LC_TIME, c"".as_ptr()).is_null() {
            return String::new();
        }
        let format = libc::nl_langinfo(libc::T_FMT);
        let format = if format.is_null() {
            String::new()
        } else {
            std::ffi::CStr::from_ptr(format).to_string_lossy().into_owned()
        };
        libc::setlocale(libc::LC_TIME, c"C".as_ptr());
        format
    }
}

#[cfg(not(unix))]
fn locale_time_format() -> String {
    String::new()
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
