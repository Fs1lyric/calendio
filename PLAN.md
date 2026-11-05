# Apple Calendar look and feel

Goal: Calendio should look and feel like Apple Calendar (macOS) and feel solid.
The previous plan (booking, delivery, Google sync, recovery) shipped in d1cd6be.

Never launch Calendio on the desktop to check it. CI's `screenshots` job runs every
view in Xvfb on a synthetic database (`tests/screenshots.sh`, seeded by
`crates/core/examples/demo_seed.rs`) and uploads PNGs. Read them with
`gh run download <id> -n screenshots -D <scratch dir>`.

Baseline: CI run 42012254019 (2026-11-05). The toolbar is three rows of grey
buttons (view tabs, Booking, + New, Quick add, Search, Calendars, Import/export,
Personal, Undo, Theme, Recovery & connections). The sidebar repeats the
Year/Agenda/Day/Week/Month switch. The week view opens at 08:00 with that label
cut in half.

## Steps (one or more commits each, test first where there is logic)

1. **Apple theme** (`theme.rs`): `apple-light` / `apple-dark` themes. White or
   #1e1e1e canvas, hairline grid (#e5e5e5 / #3a3a3a), system red (#ff3b30) for
   today and the now line, and Inter as the UI font (OFL, bundled under
   `crates/app/assets`, license added to THIRD_PARTY_LICENSES). Make it the default
   theme.
2. **Toolbar** (`main.rs`): one row. On the left, a sidebar toggle and `+`. In the
   centre, a Day / Week / Month / Year segmented control. On the right, a search
   field. Under it, a large bold "November 2026" title on the left and
   `‹ Today ›` on the right. Quick add stays on its shortcut. Import/export moves
   to a `…` menu. The Personal picker goes away (the sidebar replaces it).
3. **Sidebar** (`workspace.rs`, `navigation.rs`): calendars grouped under
   "My Calendars", each with a round coloured checkbox for visibility. Mini month
   at the bottom, with today in red and the selected week tinted.
4. **Week and day grid** (`views.rs`, `grid.rs`):
   - Header shows "Mon 19", with today's number in a filled red circle.
   - 12-hour labels ("9 AM") unless the system clock is 24-hour.
   - Hairline hour lines and fainter half-hours.
   - A red now-line across today's column, with the time in a red pill in the
     gutter.
   - Overlapping events split the column side by side. This is pure layout logic,
     so unit-test it: two, three, nested and touching events.
5. **Event blocks**: fill in the calendar colour at about 18% alpha (30% in
   dark), a 3 px solid bar on the left, a semibold title in a darkened version
   of the calendar colour, the time underneath, and a 5 px corner radius. The
   selected event gets a solid fill with white text.
6. **Month view**: date number top-right (today in a red circle), and timed
   events as "● 9 AM Title" lines. All-day events are solid pills. Overflow
   shows "+2 more".
7. **Solid feel**:
   - Keyboard: `T` for today, `←`/`→` to page, and `⌘1`–`⌘4` for views (Ctrl
     on Linux).
   - No layout jump when switching views.
   - Audit `unwrap()`/`expect()` on user data paths in `crates/app`.

Done when CI is green and the screenshots match the description above in both
light and dark mode at 1180x780 and 390 px wide.
