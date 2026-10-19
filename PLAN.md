# Calendar feature implementation

1. `core/db.rs`, new `core/calendars.rs`, integration tests: migrate existing
   events into a Personal calendar; calendar CRUD/visibility, event assignment,
   Unicode event search and visible occurrence filtering. Hidden events must
   continue to block booking availability.
2. `app/grid.rs`, `main.rs`, new workspace UI module: agenda view, search,
   calendar controls and assignment in the editor.
3. New `core/ical.rs` and tests: ICS import/export with text escaping/folding,
   all-day/UTC/IANA timezone dates, recurrence and exceptions, stable UIDs,
   atomic validation and duplicate protection. Reject unsupported recurrence
   constructs rather than silently changing their meaning.
4. Wire import/export into the desktop UI; keep credentials-dependent Google
   sync, invitations and shared-account features explicitly incomplete.

Verification: write failing tests first, then implement. Run workspace tests,
Clippy, formatting and identity checks; launch against a disposable database
and screenshot the actual UI. Record precise supported interchange behavior.
