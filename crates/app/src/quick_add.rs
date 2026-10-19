//! Explicit shortcuts, not guessed dates: every result is reviewed in the editor.
use crate::{form::Form, theme};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use eframe::egui::{self, RichText};

fn clock(input: &str) -> Result<NaiveTime, String> {
    let input = input.to_ascii_lowercase();
    let (raw, period) = if let Some(raw) = input.strip_suffix("am") {
        (raw, Some(false))
    } else if let Some(raw) = input.strip_suffix("pm") {
        (raw, Some(true))
    } else {
        (input.as_str(), None)
    };
    let (hour, minute) = raw.split_once(':').unwrap_or((raw, "00"));
    if hour.is_empty() || hour.len() > 2 || minute.len() != 2 {
        return Err("Use a time such as 9am, 3:30pm, or 14:30".into());
    }
    let mut hour = hour.parse::<u32>().map_err(|_| "Invalid hour")?;
    let minute = minute.parse::<u32>().map_err(|_| "Invalid minutes")?;
    if let Some(pm) = period {
        if !(1..=12).contains(&hour) {
            return Err("12-hour times must use an hour from 1 to 12".into());
        }
        hour = hour % 12 + if pm { 12 } else { 0 };
    }
    NaiveTime::from_hms_opt(hour, minute, 0).ok_or_else(|| "That time does not exist".into())
}
fn weekday(input: &str) -> Option<u32> {
    Some(match input.to_ascii_lowercase().as_str() {
        "mon" | "monday" => 0,
        "tue" | "tuesday" => 1,
        "wed" | "wednesday" => 2,
        "thu" | "thursday" => 3,
        "fri" | "friday" => 4,
        "sat" | "saturday" => 5,
        "sun" | "sunday" => 6,
        _ => return None,
    })
}
pub fn parse(input: &str, now: NaiveDateTime, tz: Tz) -> Result<Form, String> {
    let mut parts: Vec<&str> = input.split_whitespace().collect();
    let mut minutes = 60;
    if parts.len() >= 2 && parts[parts.len() - 2].eq_ignore_ascii_case("for") {
        let value = parts.pop().unwrap().to_ascii_lowercase();
        parts.pop();
        let (amount, multiplier) = if let Some(raw) = value.strip_suffix('m') {
            (raw, 1)
        } else if let Some(raw) = value.strip_suffix('h') {
            (raw, 60)
        } else {
            return Err("Use a duration such as 30m or 2h".into());
        };
        minutes = amount
            .parse::<i64>()
            .ok()
            .and_then(|n| n.checked_mul(multiplier))
            .filter(|n| (1..=1440).contains(n))
            .ok_or("Duration must be between 1 minute and 24 hours")?;
    }
    let at = parts.iter().rposition(|p| p.eq_ignore_ascii_case("at")).ok_or("Use: title tomorrow at 14:30 for 45m")?;
    if at == 0 || at + 2 != parts.len() {
        return Err("Add a title, then ‘at’ and a time".into());
    }
    let time = clock(parts[at + 1])?;
    let mut end_title = at;
    let mut date = now.date();
    let date_word = parts[at - 1];
    if date_word.eq_ignore_ascii_case("today") {
        end_title -= 1;
    } else if date_word.eq_ignore_ascii_case("tomorrow") {
        date = date.succ_opt().ok_or("Date is out of range")?;
        end_title -= 1;
    } else if let Some(target) = weekday(date_word) {
        if at >= 2 && parts[at - 2].eq_ignore_ascii_case("next") {
            return Err("Use a weekday or an exact YYYY-MM-DD date without ‘next’".into());
        }
        let mut delta = (target + 7 - date.weekday().num_days_from_monday()) % 7;
        if delta == 0 && time < now.time() {
            delta = 7;
        }
        date = date.checked_add_signed(Duration::days(i64::from(delta))).ok_or("Date is out of range")?;
        end_title -= 1;
    } else if date_word.len() == 10 && date_word.as_bytes()[4] == b'-' {
        date = NaiveDate::parse_from_str(date_word, "%Y-%m-%d").map_err(|_| "That date does not exist")?;
        end_title -= 1;
    }
    let title = parts[..end_title].join(" ");
    if title.is_empty() {
        return Err("Add an event title before the date".into());
    }
    let start = date.and_time(time);
    let end = start.checked_add_signed(Duration::minutes(minutes)).ok_or("End time is out of range")?;
    let mut form = Form::new(start, end, false, tz);
    form.title = title;
    Ok(form)
}

#[derive(Default)]
pub struct QuickAdd {
    open: bool,
    text: String,
    focus: bool,
}
impl QuickAdd {
    pub fn open(&mut self) {
        self.open = true;
        self.text.clear();
        self.focus = true;
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn show(&mut self, ctx: &egui::Context, now: NaiveDateTime, tz: Tz) -> Option<Form> {
        if !self.open {
            return None;
        }
        let mut open = self.open;
        let mut result = None;
        egui::Window::new("Quick add").open(&mut open).default_width(470.0).min_width(240.0).collapsible(false).show(
            ctx,
            |ui| {
                ui.label("Write a title, date, and time. Review the event before saving.");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.text)
                        .hint_text("Design review tomorrow at 14:30 for 45m")
                        .desired_width(f32::INFINITY),
                );
                if self.focus {
                    response.request_focus();
                    self.focus = false;
                }
                let parsed = parse(&self.text, now, tz);
                match &parsed {
                    Ok(form) => {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&form.title).strong());
                        ui.label(format!(
                            "{} · {} – {}",
                            form.start.format("%a %-d %b %Y"),
                            form.start.format("%H:%M"),
                            form.end.format(if form.end.date() == form.start.date() {
                                "%H:%M"
                            } else {
                                "%a %-d %b %H:%M"
                            })
                        ));
                        ui.label(RichText::new(tz.to_string()).small().color(theme::ink_3()));
                    }
                    Err(message) if !self.text.trim().is_empty() => {
                        ui.label(RichText::new(message).color(theme::danger()));
                    }
                    _ => {}
                }
                ui.add_space(10.0);
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(parsed.is_ok(), egui::Button::new("Review event")).clicked() || enter {
                    if let Ok(form) = parsed {
                        result = Some(form);
                    }
                }
                ui.separator();
                ui.label(
                    RichText::new(
                        "Dates: today, tomorrow, Friday, or YYYY-MM-DD. Times: 9am or 14:30. Duration: 30m or 2h.",
                    )
                    .small()
                    .color(theme::ink_3()),
                );
            },
        );
        self.open = open && result.is_none();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    fn now() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 19).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }
    #[test]
    fn tomorrow_and_duration_preserve_the_title() {
        let f = parse("Design review tomorrow at 14:30 for 45m", now(), chrono_tz::Asia::Dhaka).unwrap();
        assert_eq!(f.title, "Design review");
        assert_eq!(f.start.to_string(), "2026-10-20 14:30:00");
        assert_eq!((f.end - f.start).num_minutes(), 45);
        assert_eq!(f.tz, chrono_tz::Asia::Dhaka);
    }
    #[test]
    fn weekdays_12_hour_times_iso_dates_and_midnight_are_explicit() {
        let f = parse("Team call Friday at 3pm", now(), chrono_tz::UTC).unwrap();
        assert_eq!(f.start.to_string(), "2026-10-23 15:00:00");
        let f = parse("Standup monday at 9am", now(), chrono_tz::UTC).unwrap();
        assert_eq!(f.start.to_string(), "2026-10-26 09:00:00");
        let f = parse("Deploy 2026-10-21 at 23:30 for 2h", now(), chrono_tz::UTC).unwrap();
        assert_eq!(f.end.to_string(), "2026-10-22 01:30:00");
        let f = parse("Midnight tomorrow at 12am", now(), chrono_tz::UTC).unwrap();
        assert_eq!(f.start.to_string(), "2026-10-20 00:00:00");
    }
    #[test]
    fn invalid_or_ambiguous_inputs_cannot_create_events() {
        for input in [
            "",
            "Meeting tomorrow",
            "Meeting at 25:00",
            "Meeting at 13pm",
            "Meeting at 9 for 0m",
            "Meeting at 9 for 25h",
            "tomorrow at 9",
            "Meeting 2026-02-30 at 9",
            "Meeting next Tuesday at 9",
        ] {
            assert!(parse(input, now(), chrono_tz::UTC).is_err(), "Accepted {input}");
        }
    }
    #[test]
    fn unicode_titles_and_at_inside_titles_are_not_corrupted() {
        let f = parse("Équipe at home today at 11:15", now(), chrono_tz::UTC).unwrap();
        assert_eq!(f.title, "Équipe at home");
        assert_eq!(f.start.to_string(), "2026-10-19 11:15:00");
    }
}
