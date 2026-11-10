# iPadOS 26 Calendar reference

Source: Apple's iPad User Guide (support.apple.com/guide/ipad, Calendar chapter), fetched 2026-11-10.
- `week-big.png` (ipad99d9847f): Week view, full toolbar.
- `week-new.png`: Week view with the new-event sheet (Event / Reminder, glass sheet on the left).
- `day.png` (ipadafeefacf): Day view with the event inspector pane on the right.
- `calendars.png` (ipad1b02bf5b): calendars list sidebar (iCloud / Other groups, round checks, (i) buttons, toggles, Add Calendar).
All light mode; no dark reference yet.

## Measured (light)
- Calendar colours = iOS 26 system colours: red #FF383C, blue #0088FF, green #34C759, yellow #FFCC00,
  purple #CB30E0 (sampled #FE363A, #0087FE, #32C758, #FEC400, #C928DF). Orange #FF8D28 and brown
  #AC7F5E are the iOS 26 values, not seen in these shots.
- Event block fill: colour at ~15% on white (blue -> #DAEEFF, red -> #FEE2E3). Title: colour mixed 50%
  toward black (blue #074580, red #801C1E, green #105D24, yellow #7B5A00). Time line lighter.
- Selected block: solid colour (#FF484C for red) with white text.
- Inspector pane background #F2F2F6, cards white.
- Done in code (more-apple): colours, fill, title, time line.

## Still to match (interface)
- Toolbar: left glass pill with three icons (calendars, inbox, list) + separate round "+"; centred
  Day/Week/Month/Year capsule; Search capsule with mic on the right.
- Title row: "April" bold + "2026" regular, red "Today" text button on the right.
- Hour labels: big hour number with small-caps "AM"/"PM", "Noon"; lighter hour lines.
- Blocks: smaller type (title ~11pt semibold, details ~9.5pt), clock/pin icons before time/location,
  recurrence icon top-right, rounded bar.
- All-day: capsule pills with a coloured circle/star icon.
- Now line: red time capsule in the gutter, red line on today's column.
- Day view: inspector pane (title with colour bar, date, time, repeats; Options cards: Calendar,
  Show As, Alert, Travel Time; Edit top-right, Delete Event at the bottom).
- Calendars list: grouped iCloud / Other with chevrons, round colour checks, red (i) buttons.
- New-event sheet: glass sheet anchored left, Event/Reminder segmented, X and red check buttons.
