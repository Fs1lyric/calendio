
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE event_types (
    id INTEGER PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    duration_min INTEGER NOT NULL CHECK (duration_min > 0),
    buffer_before_min INTEGER NOT NULL DEFAULT 0,
    buffer_after_min INTEGER NOT NULL DEFAULT 0,
    min_notice_min INTEGER NOT NULL DEFAULT 240,
    max_days_ahead INTEGER NOT NULL DEFAULT 60,
    color TEXT,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE bookings (
    id INTEGER PRIMARY KEY,
    event_type_id INTEGER NOT NULL REFERENCES event_types(id),
    start TEXT NOT NULL,
    end TEXT NOT NULL,
    guest_name TEXT NOT NULL,
    guest_email TEXT NOT NULL,
    guest_note TEXT NOT NULL DEFAULT '',
    guest_tz TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'confirmed' CHECK (status IN ('confirmed', 'cancelled')),
    created_at TEXT NOT NULL
);
CREATE INDEX bookings_start ON bookings(start);

CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    start TEXT NOT NULL,
    end TEXT NOT NULL,
    all_day INTEGER NOT NULL DEFAULT 0,
    tz TEXT NOT NULL,
    rrule TEXT,
    color TEXT,
    reminder_minutes INTEGER,
    booking_id INTEGER REFERENCES bookings(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX events_start ON events(start);

-- One occurrence of a series moved, edited or cancelled.
CREATE TABLE event_exceptions (
    event_id INTEGER NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    original_start TEXT NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0,
    title TEXT,
    notes TEXT,
    start TEXT,
    end TEXT,
    PRIMARY KEY (event_id, original_start)
);

CREATE TABLE availability (
    id INTEGER PRIMARY KEY,
    weekday INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    start_minute INTEGER NOT NULL,
    end_minute INTEGER NOT NULL
);

CREATE TABLE date_overrides (
    id INTEGER PRIMARY KEY,
    date TEXT NOT NULL,
    start_minute INTEGER NOT NULL,
    end_minute INTEGER NOT NULL
);
