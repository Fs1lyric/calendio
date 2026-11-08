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

## Progress

**Done 2026-11-05.** All seven steps shipped; CI run 42018989778 is green (fmt, clippy
`-D warnings`, 219 workspace tests of which 86 are the app's, screenshots). The `screenshots` artifact now has 18 PNGs:
the five views and the editor in `apple-light` and `apple-dark` at 1180x780, plus
week, month and day at 390x780 (`--size 390x780`, with `LC_TIME=en_US.UTF-8` so the
gutter shows 12-hour times).

What changed besides the steps above:
- Agenda moved to the `…` menu (and `A`); Undo is in the menu and on Ctrl+Z; the
  theme picker, Booking links & hours, Manage calendars and Recovery are in the menu.
- The clock follows the locale's `T_FMT` (`CALENDIO_CLOCK=12|24` overrides it).
- Phone-width weeks stack the weekday over the date; phone-width month cells show
  "● Title" without the time.
- A stale occurrence index reloads instead of panicking; an incomplete series form
  returns an error.

## Next session: start here (the four leftovers)

Same rules: test first, push, read CI; never launch or build locally. Test helpers
live in `main.rs` `mod frames` (`Harness`, `text_rects`, `circles`, `rects`,
`text_color`, `text_font`) and `views.rs` tests (`week_at`, `month_at`, `event`).

1. **Year view** (`views.rs` `year`): its month cards use egui Buttons, which paint
   grey tiles. Pull the painted mini month out of `navigation.rs` `Navigation::show`
   into a shared `paint_month(ui, month, today, tint)` and use it for both. Keep the
   busy-day mark from `year_busy_days` as a small dot under the number. Test: no
   `surface_2` rect behind year-view day numbers; today white in a red circle.
2. **Agenda** (`views.rs` `agenda`): rows are full-width Buttons with centred text.
   Make an Apple list: a semibold day header ("Thursday, 5 November"), then per
   event a colour dot, the title left in ink, the time right in `ink_2`, hairline
   separators, hover tint, click opens. Test: title starts near the left, time ends
   near the right, dot filled with the event colour.
3. **Joined all-day pills in the month** (`views.rs` `month`): a multi-day all-day
   event is one pill per day. Per week row, lay out all-day events as spans of
   columns with `grid::pack`, draw one pill per span with the title once, and start
   timed lines below the lanes. Test: a Fri–Sun trip is one rect about three
   columns wide with one "Trip" label.
4. **12/24-hour everywhere** (`navigation.rs` `today_agenda`, `quick_add.rs`
   preview): pass `Clock` and use `range`/`short`. Test: the today panel with
   `Clock::H12` shows "9 – 10 AM". Screenshot the today panel: widen Xvfb in
   `tests/screenshots.sh` to 1600x780 and add a `week-light-1500` shot.

Done when CI is green and the year, agenda, month and 1500-wide shots look like
Apple Calendar in light and dark.

---

# Finish line (decided 2026-11-08)

Calendio is the user's only daily app. "Finished" = these three, nothing else from
ROADMAP.md. Same rules: test first, push, read CI; never build or launch locally
(screenshots come from the CI `screenshots` job). Commit as `Fs1lyric <roohlyric57@gmail.com>`
(set per repo), no AI attribution lines.

## A. Backups you can trust (small)
Already there: booker `workers.rs` calls `Db::daily_backup` (core `recovery.rs`) into
`~/.local/share/calendio/backups/calendio-YYYY-MM-DD.sqlite3`; restore lives in the `…`
menu's "Undo & backups" (`recovery_ui.rs`, one very long line per row; reformat it).
1. Keep the newest 30 daily files: `Db::prune_backups(dir, keep)`; call it after
   `daily_backup`. Test: 35 dated files in a temp dir → 30 left, the oldest gone, other
   files (e.g. `pre-claude-*.db`) untouched.
2. Off-disk copy: optional setting `backup_mirror` (a folder, e.g. a USB stick or a
   synced cloud folder); after the daily backup, copy the file there too. Test with
   two temp dirs.
3. Restore picker: list the backups newest first ("Today", "Yesterday", "Mon 3 Nov")
   with a Restore button each, instead of a typed path. Keep the safety backup and
   the confirm checkbox.

## B. Subscribed calendars (medium, new)
Add any `webcal://`/`https://` iCal URL as a read-only calendar that refreshes itself.
1. Core: `calendars.subscription_url`, `last_fetched`, `etag` columns (migration test);
   `Db::replace_subscription(calendar_id, ics)` that swaps the calendar's events in
   one transaction using the existing `import_ics` parser. Test: re-importing a
   changed feed updates/removes events, never duplicates.
2. Booker worker: every 6 h (and on demand) fetch each URL with `If-None-Match`,
   size cap 5 MB, 20 s timeout, then `replace_subscription`. Test the worker with a
   fake fetcher (see `google.rs` `Api` trait for the pattern).
3. App: "New Subscription…" in the sidebar menu (URL + name + colour via
   `swatches.rs`); subscribed calendars show a small ↻ glyph and their events open
   read-only in the editor (no Save, no drag). Tests: editor read-only, drag ignored.
4. Seed the demo DB with a holidays feed fixture so CI screenshots show one.

## C. Google two-way sync (code exists; needs setup)
Already there: booker `google.rs` (windowed incremental pull, ETag-guarded writes),
`google_auth.rs` (`calendio-booker --google-connect`, PKCE, reads
`CALENDIO_GOOGLE_CLIENT_ID` / `_SECRET`), "Connect Google Calendar" button.
1. User step (cannot be done by Claude): Google Cloud Console → new project → enable
   Google Calendar API → OAuth consent screen (External, add own address as test
   user) → Credentials → OAuth client ID, type **Desktop app**. Paste ID + secret.
2. Store them in `~/.config/calendio/google.env` (0600) and add
   `EnvironmentFile=-%h/.config/calendio/google.env` to the `calendio-booker` unit
   in `packaging/` + install.sh; the app's Connect button passes the same env.
3. Settings shows the real `google_status`, last sync time and the error if any.
4. Verify for real: connect, make an event on the phone → appears in Calendio within
   a minute; edit in Calendio → appears on the phone.

Order: A, then B, then C (C waits on the user's credentials).

## Finish line progress (2026-11-08, branch `finish-line`)
- A done: `Db::prune_backups` (30), `Db::mirror_backups` + `backup_mirror` setting
  (booker `backups.rs`, retries every 10 min), restore picker with "Today / Yesterday /
  Tue 3 Nov" rows (`recovery::list_backups`, `backup_label`). Screenshot `recovery-light`.
- B done: migration 7 (`subscription_url`, `last_fetched`, `etag`), core
  `subscriptions.rs` (`create_subscription`, `replace_subscription`, read-only guard in
  `record_edit`, unsubscribe drops events), booker `subscriptions.rs` (6 h / on demand via
  `subscriptions_refresh`, ETag, 5 MB, 20 s, own task; feeds skipped by the Google push),
  app: "New subscription…" in `…`, ↻ in sidebar, read-only editor, drags ignored. Demo
  seed has a Holidays feed.
- C code done: `~/.config/calendio/google.env` (core `config.rs`, install.sh template,
  unit `EnvironmentFile`, booker loads it at start), `google_last_sync`, Recovery window
  shows setup state / last sync / errors.
- Fixed a flaky test (week-open test failed around 7 AM UTC: the now line hides "7 AM").
- NOT VERIFIED YET: GitHub Actions stopped starting jobs (billing/spending limit) after
  run 42458374149. That run: clippy green, app tests 107/108 (the flaky one), screenshots
  fine; core + booker tests never ran. Next: get one green run, merge to main, install.
- Left for the user: C1 (Google Cloud Desktop OAuth client → google.env), C4 (real
  phone round trip).
