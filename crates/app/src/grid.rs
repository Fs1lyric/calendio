//! The calendar's geometry, kept free of drawing so it can be tested: which
//! days a view shows, the query window, where an occurrence falls in local
//! time, and how overlapping events share a day column.

use calendio_core::recur::Occurrence;
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Month,
    Week,
    Day,
    Agenda,
    Year,
}

/// Weeks start on Monday.
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// The days on screen: six full weeks for a month, seven or one otherwise.
pub fn days(view: View, anchor: NaiveDate) -> Vec<NaiveDate> {
    let (first, count) = match view {
        View::Month => (week_start(anchor.with_day(1).expect("day 1 exists")), 42),
        View::Week => (week_start(anchor), 7),
        View::Day => (anchor, 1),
        View::Agenda => (anchor, 30),
        View::Year => {
            let first = NaiveDate::from_ymd_opt(anchor.year(), 1, 1).expect("valid year");
            let next = NaiveDate::from_ymd_opt(anchor.year() + 1, 1, 1).expect("valid next year");
            (first, (next - first).num_days() as usize)
        }
    };
    first.iter_days().take(count).collect()
}

/// The anchor `by` months, weeks or days away. A month step keeps the day of
/// the month where it can (31 Jan → 28 Feb).
pub fn step(view: View, anchor: NaiveDate, by: i32) -> NaiveDate {
    match view {
        View::Month | View::Year => {
            let count = if view == View::Year { 12 } else { 1 };
            let months = chrono::Months::new(by.unsigned_abs() * count);
            if by >= 0 {
                anchor + months
            } else {
                anchor - months
            }
        }
        View::Week => anchor + Duration::weeks(by as i64),
        View::Day => anchor + Duration::days(by as i64),
        View::Agenda => anchor + Duration::days(30 * by as i64),
    }
}

/// The heading over the calendar as (bold, regular), Apple style: "November"
/// then "2026". A week across two months names both.
pub fn heading(view: View, anchor: NaiveDate) -> (String, String) {
    let year = |d: NaiveDate| d.format("%Y").to_string();
    match view {
        View::Month => (anchor.format("%B").to_string(), year(anchor)),
        View::Year => (year(anchor), String::new()),
        View::Day => (anchor.format("%-d %B").to_string(), year(anchor)),
        View::Agenda => ("Agenda".into(), anchor.format("from %-d %b %Y").to_string()),
        View::Week => {
            let (a, b) = (week_start(anchor), week_start(anchor) + Duration::days(6));
            let bold = if a.year() != b.year() {
                format!("{} – {}", a.format("%b %Y"), b.format("%b"))
            } else if a.month() != b.month() {
                format!("{} – {}", a.format("%b"), b.format("%b"))
            } else {
                a.format("%B").to_string()
            };
            (bold, year(b))
        }
    }
}

/// Local midnight as an instant. A zone that skips midnight starts the day an hour later.
pub fn midnight(day: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let at = |h| tz.from_local_datetime(&day.and_hms_opt(h, 0, 0).expect("valid hour")).earliest();
    at(0).or_else(|| at(1)).expect("a day has a first hour").with_timezone(&Utc)
}

/// From the first day's midnight to the midnight after the last day.
pub fn window(days: &[NaiveDate], tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let (first, last) = (days[0], days[days.len() - 1]);
    (midnight(first, tz), midnight(last + Duration::days(1), tz))
}

/// A stored timed value (UTC RFC3339) in local wall-clock time.
pub fn local(stamp: &str, tz: Tz) -> NaiveDateTime {
    stamp.parse::<DateTime<Utc>>().map(|t| t.with_timezone(&tz).naive_local()).unwrap_or_default()
}

/// A local wall-clock time as a stored UTC value.
pub fn stamp(t: NaiveDateTime, tz: Tz) -> String {
    // A wall-clock time skipped by DST lands on the first valid one after it.
    let mut t = t;
    loop {
        if let Some(at) = tz.from_local_datetime(&t).earliest() {
            return at.with_timezone(&Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        }
        t += Duration::minutes(15);
    }
}

/// A timed occurrence's minutes on `day`, clipped to the day, if it touches it.
pub fn span_on(o: &Occurrence, day: NaiveDate, tz: Tz) -> Option<(i64, i64)> {
    if o.all_day {
        return None;
    }
    let base = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    let minutes = |s: &str| (local(s, tz) - base).num_minutes();
    let (a, b) = (minutes(&o.start), minutes(&o.end));
    (a < 24 * 60 && b > 0).then(|| (a.max(0), b.min(24 * 60)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub column: usize,
    pub columns: usize,
}

/// Lays out spans (start, end minutes) side by side: each gets the first free
/// column, and every span in an overlapping cluster shares its column count.
pub fn pack(spans: &[(i64, i64)]) -> Vec<Placed> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| (spans[i].0, -spans[i].1));
    let mut out = vec![Placed { column: 0, columns: 1 }; spans.len()];
    let mut cluster: Vec<usize> = Vec::new();
    let mut column_ends: Vec<i64> = Vec::new();
    let mut cluster_end = i64::MIN;
    let close = |cluster: &mut Vec<usize>, columns: usize, out: &mut Vec<Placed>| {
        for i in cluster.drain(..) {
            out[i].columns = columns;
        }
    };
    for i in order {
        let (start, end) = spans[i];
        if start >= cluster_end {
            close(&mut cluster, column_ends.len(), &mut out);
            column_ends.clear();
        }
        let column = match column_ends.iter().position(|&e| e <= start) {
            Some(c) => c,
            None => {
                column_ends.push(i64::MIN);
                column_ends.len() - 1
            }
        };
        column_ends[column] = end;
        out[i].column = column;
        cluster.push(i);
        cluster_end = cluster_end.max(end);
    }
    close(&mut cluster, column_ends.len(), &mut out);
    out
}

/// The minute under `y` pixels from the top of a day column, snapped down to `snap`.
pub fn minute_at(y: f32, hour_px: f32, snap: i64) -> i64 {
    let minute = (y / hour_px * 60.0).floor() as i64;
    (minute.div_euclid(snap) * snap).clamp(0, 24 * 60 - snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn occ(start: &str, end: &str) -> Occurrence {
        Occurrence {
            event_id: 1,
            title: String::new(),
            notes: String::new(),
            start: start.into(),
            end: end.into(),
            all_day: !start.contains('T'),
            color: None,
            reminder_minutes: None,
            booking_id: None,
            original_start: None,
        }
    }

    const DHAKA: Tz = chrono_tz::Asia::Dhaka;

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(week_start(d("2026-10-15")), d("2026-10-12"));
        assert_eq!(week_start(d("2026-10-12")), d("2026-10-12"));
        assert_eq!(week_start(d("2026-10-18")), d("2026-10-12"));
    }

    #[test]
    fn views_show_the_right_days() {
        let month = days(View::Month, d("2026-10-15"));
        assert_eq!(month.len(), 42);
        assert_eq!(month[0], d("2026-09-28"));
        assert_eq!(month[41], d("2026-11-08"));
        assert_eq!(
            days(View::Week, d("2026-10-15")),
            (12..=18).map(|n| d(&format!("2026-10-{n}"))).collect::<Vec<_>>()
        );
        assert_eq!(days(View::Day, d("2026-10-15")), [d("2026-10-15")]);
    }

    #[test]
    fn stepping_moves_by_the_views_unit() {
        assert_eq!(step(View::Month, d("2026-01-31"), 1), d("2026-02-28"));
        assert_eq!(step(View::Month, d("2026-01-15"), -1), d("2025-12-15"));
        assert_eq!(step(View::Week, d("2026-10-15"), 1), d("2026-10-22"));
        assert_eq!(step(View::Day, d("2026-10-31"), 1), d("2026-11-01"));
    }

    /// Apple Calendar's heading: the month in bold, then the year in regular weight.
    #[test]
    fn headings_name_the_month_in_bold_and_the_year_after_it() {
        let h = |view, day| heading(view, d(day));
        let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(h(View::Month, "2026-10-15"), pair("October", "2026"));
        assert_eq!(h(View::Week, "2026-10-15"), pair("October", "2026"));
        assert_eq!(h(View::Week, "2026-09-30"), pair("Sep – Oct", "2026"));
        assert_eq!(h(View::Week, "2026-12-30"), pair("Dec 2026 – Jan", "2027"));
        assert_eq!(h(View::Day, "2026-10-15"), pair("15 October", "2026"));
        assert_eq!(h(View::Year, "2026-10-15"), pair("2026", ""));
        assert_eq!(h(View::Agenda, "2026-10-15"), pair("Agenda", "from 15 Oct 2026"));
    }

    #[test]
    fn the_window_runs_local_midnight_to_midnight() {
        let (from, to) = window(&days(View::Week, d("2026-10-15")), DHAKA);
        assert_eq!(from.to_rfc3339(), "2026-10-11T18:00:00+00:00");
        assert_eq!(to.to_rfc3339(), "2026-10-18T18:00:00+00:00");
    }

    #[test]
    fn stamps_convert_both_ways() {
        let t = local("2026-10-15T03:30:00Z", DHAKA);
        assert_eq!(t.to_string(), "2026-10-15 09:30:00");
        assert_eq!(stamp(t, DHAKA), "2026-10-15T03:30:00Z");
    }

    #[test]
    fn timed_spans_are_clipped_to_each_day() {
        // 22:00 on the 15th to 02:00 on the 16th, Dhaka time.
        let o = occ("2026-10-15T16:00:00Z", "2026-10-15T20:00:00Z");
        assert_eq!(span_on(&o, d("2026-10-15"), DHAKA), Some((22 * 60, 24 * 60)));
        assert_eq!(span_on(&o, d("2026-10-16"), DHAKA), Some((0, 2 * 60)));
        assert_eq!(span_on(&o, d("2026-10-17"), DHAKA), None);
        // Ending exactly at midnight does not touch the next day.
        let o = occ("2026-10-15T17:00:00Z", "2026-10-15T18:00:00Z");
        assert_eq!(span_on(&o, d("2026-10-16"), DHAKA), None);
    }

    #[test]
    fn overlapping_spans_share_the_width() {
        let p = |column, columns| Placed { column, columns };
        // A and B overlap, C fits under A after it ends, D is alone.
        let spans = [(540, 600), (570, 660), (600, 630), (720, 780)];
        assert_eq!(pack(&spans), [p(0, 2), p(1, 2), p(0, 2), p(0, 1)]);
        assert_eq!(pack(&[]), []);
        // Touching end to start is not an overlap.
        assert_eq!(pack(&[(540, 600), (600, 660)]), [p(0, 1), p(0, 1)]);
    }

    #[test]
    fn three_overlapping_and_nested_events_split_the_column() {
        let p = |column, columns| Placed { column, columns };
        // Three that all overlap get a third each.
        assert_eq!(pack(&[(540, 660), (560, 640), (580, 620)]), [p(0, 3), p(1, 3), p(2, 3)]);
        // Two short events inside a long one share the second half, one after the other.
        assert_eq!(pack(&[(540, 720), (570, 600), (630, 660)]), [p(0, 2), p(1, 2), p(1, 2)]);
        // Input order does not matter.
        assert_eq!(pack(&[(630, 660), (540, 720), (570, 600)]), [p(1, 2), p(0, 2), p(1, 2)]);
        // A chain where only neighbours overlap still needs only two columns.
        assert_eq!(pack(&[(540, 600), (570, 630), (610, 660)]), [p(0, 2), p(1, 2), p(0, 2)]);
    }

    #[test]
    fn pixels_map_to_snapped_minutes() {
        assert_eq!(minute_at(0.0, 48.0, 15), 0);
        assert_eq!(minute_at(48.0 * 9.5, 48.0, 15), 570);
        assert_eq!(minute_at(48.0 * 9.4, 48.0, 15), 555);
        assert_eq!(minute_at(-10.0, 48.0, 15), 0);
        assert_eq!(minute_at(48.0 * 30.0, 48.0, 15), 24 * 60 - 15);
    }
}

#[cfg(test)]
mod agenda_tests {
    use super::*;
    #[test]
    fn agenda_has_a_month_of_days_and_navigation_keeps_its_window() {
        let anchor = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let visible = days(View::Agenda, anchor);
        assert_eq!(visible.len(), 30);
        assert_eq!(visible[0], anchor);
        assert_eq!(step(View::Agenda, anchor, 1), anchor + Duration::days(30));
        assert_eq!(heading(View::Agenda, anchor).0, "Agenda");
    }
}

#[cfg(test)]
mod year_tests {
    use super::*;
    #[test]
    fn year_view_includes_leap_day_and_moves_by_calendar_years() {
        let leap = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let days = days(View::Year, leap);
        assert_eq!(days.len(), 366);
        assert_eq!(days[0], NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        assert_eq!(step(View::Year, leap, 1), NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
        assert_eq!(heading(View::Year, leap), ("2024".to_owned(), String::new()));
    }
}
