# Calendio

A local-first calendar and booking app. Native Rust desktop UI, SQLite storage,
and a separate booking server that stays available when the window closes.

**Status:** calendar and booking app under active development, with independent app
identity and storage. This is not yet feature parity with Google Calendar or
Calendly. See [ROADMAP.md](ROADMAP.md) for the remaining product scope.

## Run

```sh
cargo run -p calendio
# Or open a specific view:
cargo run -p calendio -- --view agenda
cargo run -p calendio-booker -- --port 8788
```

The default database is `~/.local/share/calendio/cal.db` (respects XDG data
location). Set `CALENDIO_DB` for a separate database. Calendi's database is not
migrated or modified. Booking pages use `http://127.0.0.1:8788/<slug>`; public
access requires a separately configured HTTPS tunnel or hosting. No public
service is deployed by creating this repository.

## Available now

- Month, week, day, 30-day agenda, and year overview.
- Multiple calendars with names, colors, visibility, event assignment and safe removal.
- Literal, case-insensitive Unicode search across event titles and notes.
- ICS file import/export, including recurring events and exceptions; see
  [interchange support](docs/ical.md).
- Click to create/edit and drag to move events.
- All-day events, event colors and notes, timezone-aware recurring events,
  and occurrence exceptions.
- Booking event types, weekly availability and date overrides, duration,
  buffers, booking windows, minimum notice, and conflict checks.
- Public booking page and booking cancellation; local desktop reminders.

## Verify

```sh
python3 tests/identity.py
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

`./install.sh` builds and installs this app and its user booking service. It
starts the service; running the app from Cargo does not install anything.

## Origin

Forked from local Calendi commit `cea9036567e6549dd91d3ea0fb3ad1a0076741e4`.
Original development notes are retained in `CALENDI_HISTORY.md`. Cal.diy-derived
code attribution remains in `THIRD_PARTY_LICENSES`. Package metadata retains the
original MIT license declaration; inherited third-party terms still apply.

## Interface previews

- [Year overview](docs/year-preview.png)
- [Agenda](docs/agenda-preview.png)

Screenshots use a disposable database containing synthetic events.
