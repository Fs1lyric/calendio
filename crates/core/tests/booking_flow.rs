use calendio_core::db::{BookRequest, Db, EventTypeInput, WeeklyHours};
use chrono::{DateTime, Utc};
fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}
fn setup() -> Db {
    let db = Db::open_in_memory().unwrap();
    db.set_weekly_hours(&[WeeklyHours { weekday: 1, start_minute: 540, end_minute: 720 }]).unwrap();
    db.create_event_type(&EventTypeInput {
        slug: "chat".into(),
        title: "Chat".into(),
        description: String::new(),
        duration_min: 30,
        buffer_before_min: 0,
        buffer_after_min: 0,
        min_notice_min: 0,
        max_days_ahead: 30,
        color: None,
        active: true,
    })
    .unwrap();
    db
}
fn request(start: &str) -> BookRequest {
    BookRequest {
        slug: "chat".into(),
        start: start.into(),
        guest_name: "Ana Ruiz".into(),
        guest_email: "ana@example.com".into(),
        guest_note: String::new(),
        guest_tz: "UTC".into(),
    }
}
fn now() -> DateTime<Utc> {
    at("2026-10-15T00:00:00Z")
}
#[test]
fn secret_links_reschedule_the_same_event_and_release_the_old_slot() {
    let db = setup();
    let receipt = db.book_with_management(&request("2026-10-19T09:00:00Z"), chrono_tz::UTC, now()).unwrap();
    assert_eq!(receipt.token.len(), 64);
    assert!(db.managed_booking(&"a".repeat(64)).is_err());
    let id = db.all_events().unwrap()[0].id;
    let moved = db.reschedule(&receipt.token, "2026-10-19T10:00:00Z", "UTC", chrono_tz::UTC, now()).unwrap();
    assert_eq!(moved.id, receipt.booking.id);
    assert_eq!(db.all_events().unwrap()[0].id, id);
    assert_eq!(db.event(id).unwrap().unwrap().start, "2026-10-19T10:00:00Z");
    db.book(&request("2026-10-19T09:00:00Z"), chrono_tz::UTC, now()).unwrap();
}
#[test]
fn conflicting_moves_roll_back_and_expired_or_cancelled_links_cannot_move() {
    let db = setup();
    let a = db.book_with_management(&request("2026-10-19T09:00:00Z"), chrono_tz::UTC, now()).unwrap();
    db.book(&request("2026-10-19T10:00:00Z"), chrono_tz::UTC, now()).unwrap();
    assert!(db.reschedule(&a.token, "2026-10-19T10:00:00Z", "UTC", chrono_tz::UTC, now()).is_err());
    assert_eq!(db.managed_booking(&a.token).unwrap().start, "2026-10-19T09:00:00Z");
    assert!(db
        .reschedule(&a.token, "2026-10-26T09:00:00Z", "UTC", chrono_tz::UTC, at("2026-10-19T09:01:00Z"))
        .is_err());
    db.cancel_managed_booking(&a.token, now()).unwrap();
    db.cancel_managed_booking(&a.token, now()).unwrap();
    assert!(db.reschedule(&a.token, "2026-10-26T09:00:00Z", "UTC", chrono_tz::UTC, now()).is_err());
}
#[test]
fn notification_jobs_are_atomic_leased_retried_and_superseded() {
    let db = setup();
    let a = db.book_with_management(&request("2026-10-19T09:00:00Z"), chrono_tz::UTC, now()).unwrap();
    let job = db.claim_email(now()).unwrap().unwrap();
    assert_eq!(job.kind, "confirmation");
    assert!(db.claim_email(now()).unwrap().is_none());
    db.email_failed(job.id, &job.lease, now()).unwrap();
    assert!(db.claim_email(now()).unwrap().is_none());
    let retried = db.claim_email(now() + chrono::Duration::minutes(5)).unwrap().unwrap();
    assert_eq!(retried.id, job.id);
    db.reschedule(&a.token, "2026-10-19T10:00:00Z", "UTC", chrono_tz::UTC, now()).unwrap();
    assert!(!db.email_is_current(retried.id, &retried.lease).unwrap());
    let replacement = db.claim_email(now()).unwrap().unwrap();
    assert_eq!(replacement.kind, "rescheduled");
    db.email_sent(replacement.id, &replacement.lease, now()).unwrap();
    assert!(db.claim_email(now()).unwrap().is_none());
}

#[test]
fn abandoned_final_delivery_lease_becomes_a_visible_failure() {
    let db = setup();
    db.book_with_management(&request("2026-10-19T09:00:00Z"), chrono_tz::UTC, now()).unwrap();
    for attempt in 0..8 {
        assert!(db.claim_email(now() + chrono::Duration::hours(attempt)).unwrap().is_some());
    }
    assert!(db.claim_email(now() + chrono::Duration::hours(9)).unwrap().is_none());
    assert_eq!(db.email_counts().unwrap().1, 1);
}
