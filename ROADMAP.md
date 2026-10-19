# Product scope

Goal: calendar management and appointment scheduling in one polished app.
The lists below are implementation targets, not claims of shipped support.
Full parity is an ongoing target, not a completed release criterion for this
initial repository. Provider-specific and enterprise behavior needs separate
requirements and acceptance tests.

## 1. Personal calendar

- [x] Month / week / day views, event editor and drag-to-move
- [x] Recurrence, exceptions, all-day events, local reminders
- [ ] Agenda and year views, search, keyboard navigation of calendar cells
- [ ] Multiple calendars, subscriptions, visibility and per-calendar colors
- [ ] ICS import/export and CalDAV
- [ ] Attendees, invitations, RSVP, shared calendars and permissions
- [ ] Focus time, working location, out-of-office, tasks and holidays
- [ ] Multiple timezone display, offline sync and conflict reconciliation
- [ ] Google / Microsoft OAuth and bidirectional calendar sync
- [ ] Meet / Zoom / Teams conferencing, rooms and resource booking

## 2. Booking

- [x] One-on-one event types and booking pages
- [x] Weekly hours, date overrides, buffers, notice and booking horizon
- [x] Conflict protection and cancellation
- [ ] Guest self-service rescheduling and verified confirmation email
- [ ] Custom intake questions, redirect and branded/embed booking pages
- [ ] Booking limits, private/one-use links and multi-calendar conflict checks
- [ ] Group capacity, collective hosts, round robin and team routing
- [ ] Meeting polls, packages and payment/refund integration
- [ ] Reminder/follow-up workflows, email/SMS delivery and retries
- [ ] Webhooks, API, CRM integrations and scheduling analytics

## 3. Product quality and delivery

- [ ] Deliberate desktop layout with primary calendar, navigation and agenda
- [ ] Light/dark themes, keyboard-first workflows, accessible event navigation
- [ ] Multi-user authentication, invitations, tenancy and role permissions
- [ ] Stable HTTPS booking host, rate limits and abuse protection
- [ ] Encrypted provider credential storage and provider token lifecycle
- [ ] Automated backups, restore verification and migrations
- [ ] Installer/release signing, updates and CI for supported desktop systems
- [ ] End-to-end tests covering booking, DST, recurrence, sync and cancellation

## Implementation order

1. Isolate the new app and verify the inherited calendar/booking engine.
2. Establish the calendar interface and navigation; validate screenshots at
   normal and narrow window sizes and check text contrast.
3. Add calendar organization, search and ICS interchange with tests first.
4. Add provider auth/sync, invitations and conferencing behind adapters.
5. Add booking communication, rescheduling, group/team scheduling and payments.
6. Harden hosting, account isolation and releases before shared production use.

External integrations require registered applications, credentials or service
accounts. They cannot be represented as functioning integrations before those
are configured and verified.

## Reference scope (reviewed 2026-10-19)

- [Google Calendar](https://workspace.google.com/products/calendar/)
- [Calendly event types](https://calendly.com/help/event-types-overview)
- [Calendly plans and capabilities](https://calendly.com/pricing)

These are starting references; they are not an exhaustive parity audit.
