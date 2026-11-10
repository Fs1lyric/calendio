//! Fills a disposable database with synthetic events around the current week, for
//! screenshots. Refuses to touch a database that already has events.
//!
//!     CALENDIO_DB=/tmp/demo.db cargo run -p calendio-core --example demo_seed

use calendio_core::db::{Db, EventInput};
use chrono::{Datelike, Duration, NaiveDate, Utc};

fn at(day: NaiveDate, hour: u32, min: u32) -> String {
    format!("{day}T{hour:02}:{min:02}:00Z")
}

fn event(title: &str, notes: &str, start: String, end: String) -> EventInput {
    EventInput {
        title: title.into(),
        notes: notes.into(),
        start,
        end,
        all_day: false,
        tz: "UTC".into(),
        rrule: None,
        color: None,
        reminder_minutes: None,
    }
}

fn main() {
    let path = Db::default_path();
    let db = Db::open(&path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let existing: i64 = db.conn().query_row("SELECT count(*) FROM events", [], |r| r.get(0)).unwrap();
    assert_eq!(existing, 0, "{} already has events; demo_seed only fills an empty database", path.display());

    let work = db.create_calendar("Work", "#ff3b30").unwrap();
    let health = db.create_calendar("Health", "#34c759").unwrap();
    let home = db.create_calendar("Home", "#af52de").unwrap();

    let today = Utc::now().date_naive();
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
    let d = |n: i64| monday + Duration::days(n);

    let events: Vec<(i64, EventInput)> = vec![
        (work, event("Weekly planning", "", at(d(0), 9, 0), at(d(0), 10, 0))),
        (work, event("Design review", "Room 2", at(d(0), 11, 0), at(d(0), 12, 30))),
        (work, event("1:1 with Sam", "", at(d(0), 11, 30), at(d(0), 12, 0))),
        (health, event("Gym", "Legs", at(d(1), 7, 0), at(d(1), 8, 0))),
        (1, event("Focus time", "", at(d(1), 10, 0), at(d(1), 12, 0))),
        (home, event("Lunch with Zara", "", at(d(2), 13, 0), at(d(2), 14, 0))),
        (work, event("Project review", "", at(d(3), 9, 30), at(d(3), 10, 30))),
        (work, event("Standup", "", at(d(3), 9, 45), at(d(3), 10, 0))),
        (health, event("Run", "5 km", at(d(4), 6, 30), at(d(4), 7, 15))),
        (home, event("Dinner", "", at(d(5), 19, 0), at(d(5), 21, 0))),
        (1, event("Read", "", at(d(6), 16, 0), at(d(6), 17, 0))),
        (work, event("Intro call", "", at(today, 15, 0), at(today, 15, 30))),
        (1, event("Dentist", "", at(d(9), 10, 0), at(d(9), 11, 0))),
        (work, event("Quarterly review", "", at(d(16), 14, 0), at(d(16), 16, 0))),
    ];
    // A few blocks wear their own colour, the rest their calendar's.
    let own = [
        ("Focus time", "#ff9500"),
        ("Read", "#5856d6"),
        ("Lunch with Zara", "#ff2d55"),
        ("Standup", "#00c7be"),
        ("Dentist", "#ffcc00"),
    ];
    for (calendar, input) in &events {
        let mut input = input.clone();
        input.color = own.iter().find(|(t, _)| *t == input.title).map(|(_, c)| c.to_string());
        db.create_event_in_calendar(&input, *calendar).unwrap();
    }

    let mut trip = event("Trip to Sylhet", "", d(4).to_string(), d(7).to_string());
    trip.all_day = true;
    db.create_event_in_calendar(&trip, home).unwrap();
    let mut gym = event("Gym", "", at(d(0), 7, 0), at(d(0), 8, 0));
    gym.rrule = Some("FREQ=WEEKLY;BYDAY=MO,WE".into());
    db.create_event_in_calendar(&gym, health).unwrap();

    // A subscribed holidays feed, as the booking service would have fetched it.
    let holidays = db.create_subscription("Holidays", "#34c759", "webcal://example.com/holidays.ics").unwrap();
    let mut feed = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Calendio demo//EN\r\n");
    for (uid, day, title) in [("founders", d(2), "Founders' Day"), ("harvest", d(12), "Harvest Festival")] {
        feed.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:{uid}@demo\r\nDTSTAMP:20260101T000000Z\r\nDTSTART;VALUE=DATE:{}\r\n\
             SUMMARY:{title}\r\nTRANSP:TRANSPARENT\r\nEND:VEVENT\r\n",
            day.format("%Y%m%d")
        ));
    }
    feed.push_str("END:VCALENDAR\r\n");
    db.replace_subscription(holidays, &feed, None, Utc::now(), "UTC").unwrap();

    println!("seeded {} with {} events", path.display(), events.len() + 4);
}
