//! Read-only calendars fed by an iCal URL (`webcal://` or `https://`). The booking
//! service fetches the feed; this module stores it and swaps the events in one go.
use crate::{
    db::{Db, Error, Result},
    delivery::time,
    ical::ImportReport,
};
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub calendar_id: i64,
    pub name: String,
    /// Always `https://` or `http://`; `webcal://` is rewritten on the way in.
    pub url: String,
    pub last_fetched: Option<String>,
    pub etag: Option<String>,
}

/// `webcal://host/x.ics` → `https://host/x.ics`; anything but http(s) is refused.
pub fn normalize_url(raw: &str) -> Result<String> {
    let raw = raw.trim();
    let rewritten = match raw.split_once("://") {
        Some((scheme, rest)) if scheme.eq_ignore_ascii_case("webcal") || scheme.eq_ignore_ascii_case("webcals") => {
            format!("https://{rest}")
        }
        _ => raw.to_owned(),
    };
    let parsed = url::Url::parse(&rewritten).map_err(|_| Error::Invalid("Enter a calendar URL".into()))?;
    if !matches!(parsed.scheme(), "https" | "http") || parsed.host_str().is_none() {
        return Err(Error::Invalid("Subscriptions need a webcal:// or https:// address".into()));
    }
    Ok(parsed.to_string())
}

impl Db {
    pub fn create_subscription(&self, name: &str, color: &str, url: &str) -> Result<i64> {
        let url = normalize_url(url)?;
        let tx = self.savepoint()?;
        let id = self.create_calendar(name, color)?;
        tx.execute("UPDATE calendars SET subscription_url=?2 WHERE id=?1", params![id, url])?;
        tx.commit()?;
        Ok(id)
    }

    pub fn subscriptions(&self) -> Result<Vec<Subscription>> {
        let mut stmt = self.conn().prepare(
            "SELECT id,name,subscription_url,last_fetched,etag FROM calendars
             WHERE subscription_url IS NOT NULL ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Subscription {
                calendar_id: r.get(0)?,
                name: r.get(1)?,
                url: r.get(2)?,
                last_fetched: r.get(3)?,
                etag: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn is_subscribed(&self, calendar_id: i64) -> Result<bool> {
        Ok(self
            .conn()
            .query_row("SELECT subscription_url IS NOT NULL FROM calendars WHERE id=?1", [calendar_id], |r| r.get(0))
            .optional()?
            .unwrap_or(false))
    }

    /// Swaps the calendar's events for the feed's in one transaction. A feed that does
    /// not parse changes nothing, so the previous copy stays on screen. Times without a
    /// zone in the feed are read in `floating_zone` (the user's).
    pub fn replace_subscription(
        &self,
        calendar_id: i64,
        ics: &str,
        etag: Option<&str>,
        now: DateTime<Utc>,
        floating_zone: &str,
    ) -> Result<ImportReport> {
        if !self.is_subscribed(calendar_id)? {
            return Err(Error::Invalid("Not a subscribed calendar".into()));
        }
        let tx = self.savepoint()?;
        tx.execute("DELETE FROM events WHERE calendar_id=?1", [calendar_id])?;
        let report = self.import_ics(calendar_id, ics, floating_zone)?;
        self.mark_fetched(calendar_id, etag, now)?;
        tx.commit()?;
        Ok(report)
    }

    /// Records a fetch that changed nothing (HTTP 304) or a fresh copy's ETag.
    pub fn mark_fetched(&self, calendar_id: i64, etag: Option<&str>, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "UPDATE calendars SET last_fetched=?2, etag=?3 WHERE id=?1",
            params![calendar_id, time(now), etag],
        )?;
        Ok(())
    }
}
