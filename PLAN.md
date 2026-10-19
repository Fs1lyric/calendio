# Usability and event-detail improvements

- `core/details.rs`, schema migration, `db.rs`, ICS and tests: persist event
  location, meeting link, and free/busy status. Save details atomically with
  events; keep bookings protected; support busy all-day events through DST.
- `app/quick_add.rs`, tests and toolbar: parse explicit date/time shortcuts,
  preview the result, then open the regular editor before saving.
- `app/navigation.rs`, editor and app layout: mini-calendar navigation,
  integrated event details, and a today agenda on wide windows.
- Update documentation and screenshots for implemented behavior only.

Verification: failing behavior tests before code, then workspace tests,
Clippy, formatting, build, and actual app screenshots against disposable data.
No accounts, provider credentials, or hosting decisions are required.
