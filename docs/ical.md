# Calendar file interchange

Use **Import / export** in Calendio. Choose the calendar and a local `.ics`
path, then import or export. `~/` is expanded to your home directory. Export
creates a new file and refuses to overwrite an existing file.

Supported event data follows [RFC 5545](https://www.rfc-editor.org/rfc/rfc5545):
UTF-8 text escaping, folded lines, all-day dates with exclusive end dates, UTC
and IANA timezone date-times, floating date-times in the organizer's timezone,
RRULE patterns accepted by the calendar engine, EXDATE cancellation, and
RECURRENCE-ID overrides/cancellation. Export normalizes timed date-only UNTIL
values to UTC. One before-start display reminder can be imported; export also
preserves Calendio's exact reminder setting with an extension property.

Imports are atomic: invalid or unsupported recurrence data rolls back the
entire import. Files are limited to 8 MiB and 10,000 event components. UIDs are
scoped to a destination calendar. Reimporting an existing UID skips it instead
of overwriting local changes. Imports are not bidirectional synchronization.

Locations, HTTP/HTTPS meeting links, and Free/Busy transparency are preserved.
Per-occurrence changes to these fields are rejected; edit the series instead.
The import report warns when attendee/organizer data, unsupported or additional
alarms, or custom VTIMEZONE rules cannot be imported. Named timezones use installed IANA rules. RDATE,
EXRULE, THISANDFUTURE changes, invitation METHODs, and unsupported components
are rejected. Timed events require positive duration. This is bounded event
interchange, not complete iCalendar or invitation support.

Deleting a local calendar moves its events and their import identity to
Personal. If both calendars contain the same imported UID, the moved copy
receives a new UID to keep the two events distinct.
