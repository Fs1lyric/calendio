# Connections and recovery

The desktop **Recovery & connections** panel shows connection status, email
failures, backup controls and undo. Run `calendio-booker` for background email,
Google sync and daily backups. Guest bookings include a private management link
for rescheduling or cancellation. Treat that link as a password.

## Email

Configure these environment variables in the booking service:

- `CALENDIO_RESEND_API_KEY`: your Resend API key.
- `CALENDIO_EMAIL_FROM`: an address on your verified sender domain.
- `CALENDIO_PUBLIC_URL`: the public HTTPS origin, without a path.
- `CALENDIO_ORGANIZER_EMAIL`: optional organizer copy recipient.

Confirmation, rescheduling and cancellation messages enter a durable SQLite
queue. Reminders are scheduled ten minutes before the booking, when possible.
Delivery retries with backoff, up to eight attempts; failed messages can be
retried in the desktop panel. Without credentials, messages remain queued.
Rescheduling supersedes obsolete messages. Delivery uses provider idempotency
keys, but already delivered mail cannot be recalled.

## Google Calendar

Enable the Google Calendar API and create a Desktop OAuth client in your Google
Cloud project. Set `CALENDIO_GOOGLE_CLIENT_ID` and
`CALENDIO_GOOGLE_CLIENT_SECRET`, then run `calendio-booker --google-connect`
or select **Connect Google Calendar**. The flow uses a local callback, PKCE and
state validation. `xdg-open` and `secret-tool` with an unlocked Secret Service
keyring are required. Refresh credentials are stored in the desktop keyring.
For headless operation, the three environment variables ending in CLIENT_ID,
CLIENT_SECRET and REFRESH_TOKEN can supply credentials instead.

`CALENDIO_GOOGLE_CALENDAR_ID` defaults to `primary`. Sync runs every minute
against one configured Google calendar, covering 90 days back and 365 days
forward. Recurring occurrences are synchronized as individual instances.
Google events appear in a separate local calendar. Conflicting local edits are
preserved in **Sync conflicts**; conditional writes protect newer remote edits.
Bookings managed by Calendio remain authoritative so guest reservations stay
consistent. This does not provide every Google Calendar collaboration feature.

Keep service credentials in a private environment file (permissions 0600),
never in this repository. A systemd user drop-in can set `EnvironmentFile=`.
Restart the booking service after changing its environment.

## Undo and backups

Desktop event creation, edits, moves and deletion support undo, including
occurrence changes. Undo refuses to overwrite a newer change and does not alter
guest bookings. The latest 100 edits are retained.

Daily SQLite snapshots are saved beside the database in `backups/`, or in
`CALENDIO_BACKUP_DIR`. Backups are private files and include guest data and
management tokens. Existing files are never overwritten; no backups are deleted
automatically. Monitor available disk space.

To restore, stop the booking service, wait at least 30 seconds, and use the
panel's restore controls. A safety backup is required and created first.
Restored queued messages are paused to prevent accidental replay; the Google
cursor is reset for reconciliation. Credentials in the keyring are unaffected.

## Verification boundary

Automated tests use fake Google responses and local delivery-queue operations.
Real Google authorization and email delivery require your configured accounts;
no real account changes or email sends are performed during development checks.
`CALENDIO_DISABLE_INTEGRATIONS=1` disables the background worker for isolated UI
and booking-page checks, including its daily backups.
