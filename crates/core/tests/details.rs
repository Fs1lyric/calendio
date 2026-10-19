use calendio_core::{
    db::{Db, EventInput},
    details::EventDetails,
};
fn event(all_day: bool) -> EventInput {
    EventInput {
        title: "Planning".into(),
        notes: String::new(),
        start: if all_day { "2026-11-01" } else { "2026-11-01T14:00:00Z" }.into(),
        end: if all_day { "2026-11-02" } else { "2026-11-01T15:00:00Z" }.into(),
        all_day,
        tz: "America/New_York".into(),
        rrule: None,
        color: None,
        reminder_minutes: None,
    }
}
#[test]
fn details_round_trip_and_free_events_do_not_block_bookings() {
    let db = Db::open_in_memory().unwrap();
    let input = event(false);
    let details =
        EventDetails { location: "Studio 4".into(), meeting_url: "https://meet.example.com/team".into(), busy: false };
    let id = db.create_event_with_details(&input, 1, &details).unwrap();
    assert_eq!(db.event_details(id).unwrap(), details);
    assert!(db
        .busy_between("2026-11-01T00:00:00Z".parse().unwrap(), "2026-11-03T00:00:00Z".parse().unwrap())
        .unwrap()
        .is_empty());
    db.set_event_details(id, &EventDetails { busy: true, ..details }).unwrap();
    assert_eq!(
        db.busy_between("2026-11-01T00:00:00Z".parse().unwrap(), "2026-11-03T00:00:00Z".parse().unwrap())
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn busy_all_day_events_cover_the_real_local_day_through_dst() {
    let db = Db::open_in_memory().unwrap();
    let input = event(true);
    let id = db.create_event(&input).unwrap();
    assert!(!db.event_details(id).unwrap().busy);
    db.set_event_details(id, &EventDetails { busy: true, ..Default::default() }).unwrap();
    let ranges =
        db.busy_between("2026-11-01T00:00:00Z".parse().unwrap(), "2026-11-03T00:00:00Z".parse().unwrap()).unwrap();
    assert_eq!(ranges[0].start.to_rfc3339(), "2026-11-01T04:00:00+00:00");
    assert_eq!((ranges[0].end - ranges[0].start).num_hours(), 25);
}
#[test]
fn bad_links_are_rejected_without_partially_saving_changes() {
    let db = Db::open_in_memory().unwrap();
    let input = event(false);
    let original = EventDetails { location: "Studio".into(), ..Default::default() };
    let id = db.create_event_with_details(&input, 1, &original).unwrap();
    let mut changed = input.clone();
    changed.title = "Changed".into();
    for link in
        ["javascript:alert(1)", "https://user:password@example.com", "https://", "https://example.com\nmalicious"]
    {
        let bad = EventDetails { meeting_url: link.into(), ..Default::default() };
        assert!(db.update_event_with_details(id, &changed, Some(1), Some(&bad)).is_err());
        assert_eq!(db.event(id).unwrap().unwrap().input, input);
        assert_eq!(db.event_details(id).unwrap(), original);
        assert!(db.create_event_with_details(&input, 1, &bad).is_err());
    }
    assert_eq!(db.all_events().unwrap().len(), 1);
}
#[test]
fn location_is_searchable_and_deleting_an_event_cleans_its_details() {
    let db = Db::open_in_memory().unwrap();
    let id = db
        .create_event_with_details(
            &event(false),
            1,
            &EventDetails { location: "North Studio".into(), ..Default::default() },
        )
        .unwrap();
    assert_eq!(db.search_events("north studio").unwrap()[0].id, id);
    db.delete_event(id).unwrap();
    assert!(db.event_details(id).is_err());
    assert_eq!(db.conn().query_row("SELECT COUNT(*) FROM event_details", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}
#[test]
fn legacy_defaults_and_regular_edits_preserve_details() {
    let db = Db::open_in_memory().unwrap();
    let id = db.create_event(&event(false)).unwrap();
    assert!(db.event_details(id).unwrap().busy);
    let details = EventDetails { location: "Desk 12".into(), ..Default::default() };
    db.set_event_details(id, &details).unwrap();
    let mut input = event(false);
    input.notes = "Updated".into();
    db.update_event(id, &input).unwrap();
    assert_eq!(db.event_details(id).unwrap(), details);
}
