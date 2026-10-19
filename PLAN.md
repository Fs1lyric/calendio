# Complete booking, delivery, sync and recovery

1. Core booking flow + public management page: secret guest links, atomic
   rescheduling/cancellation, conflict checks and durable notification jobs.
2. Email delivery: Resend adapter, persistent leases, retry/backoff, reminders,
   stale-job suppression, and organizer copies; test with fake transport.
3. Google Calendar: refresh-token credentials from environment, paginated
   incremental pulls, conditional writes/deletes, local change tracking and
   conflict preservation. Test provider behavior with a fake transport.
4. Recovery: event undo with conflict guards; consistent SQLite backups,
   automatic daily snapshots, guarded restore with a safety backup, desktop UI.

Write behavior tests first. Finish with workspace tests, Clippy, formatting,
build, management-page checks and desktop screenshots. Never send real mail or
modify a real Google account during verification. Document credential setup
and the limits of live integration verification.
