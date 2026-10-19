//! Optional event metadata, with legacy defaults and atomic event writes.
use crate::db::{Db, Error, EventInput, Result};
use chrono_tz::Tz;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDetails {
    pub location: String,
    pub meeting_url: String,
    pub busy: bool,
}
impl Default for EventDetails {
    fn default() -> Self {
        Self { location: String::new(), meeting_url: String::new(), busy: true }
    }
}
impl EventDetails {
    pub fn clean(&self) -> Result<Self> {
        if self.location.len() > 512 || self.location.chars().any(char::is_control) {
            return Err(Error::Invalid("Location must be a single line of at most 512 bytes".into()));
        }
        let mut meeting_url = self.meeting_url.trim().to_string();
        if !meeting_url.is_empty() {
            if meeting_url.len() > 4096 || self.meeting_url.chars().any(char::is_control) {
                return Err(Error::Invalid("Meeting link must be a single URL of at most 4096 bytes".into()));
            }
            let parsed = url::Url::parse(&meeting_url)
                .map_err(|_| Error::Invalid("Enter a complete https:// or http:// meeting link".into()))?;
            if !matches!(parsed.scheme(), "https" | "http")
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(Error::Invalid(
                    "Meeting links must use HTTP or HTTPS and cannot include passwords or usernames".into(),
                ));
            }
            meeting_url = parsed.to_string();
        }
        Ok(Self { location: self.location.trim().into(), meeting_url, busy: self.busy })
    }
}
impl Db {
    pub fn event_details(&self, id: i64) -> Result<EventDetails> {
        self.conn()
            .query_row(
                "SELECT COALESCE(d.location,''),COALESCE(d.meeting_url,''),COALESCE(d.busy,NOT e.all_day)
            FROM events e LEFT JOIN event_details d ON d.event_id=e.id WHERE e.id=?1",
                [id],
                |r| Ok(EventDetails { location: r.get(0)?, meeting_url: r.get(1)?, busy: r.get(2)? }),
            )?
            .clean()
    }
    pub fn set_event_details(&self, id: i64, input: &EventDetails) -> Result<()> {
        let details = input.clean()?;
        let event = self.event(id)?.ok_or_else(|| Error::NotFound("Event no longer exists".into()))?;
        if event.booking_id.is_some() && !details.busy {
            return Err(Error::Invalid(
                "A confirmed booking must remain busy; cancel the booking to free its time".into(),
            ));
        }
        self.conn().execute("INSERT INTO event_details(event_id,location,meeting_url,busy) VALUES (?1,?2,?3,?4)
            ON CONFLICT(event_id) DO UPDATE SET location=excluded.location,meeting_url=excluded.meeting_url,busy=excluded.busy",params![id,details.location,details.meeting_url,details.busy])?;
        Ok(())
    }
    pub fn create_event_with_details(
        &self,
        input: &EventInput,
        calendar_id: i64,
        details: &EventDetails,
    ) -> Result<i64> {
        let details = details.clean()?;
        let tx = self.savepoint()?;
        let id = self.create_event_in_calendar(input, calendar_id)?;
        self.set_event_details(id, &details)?;
        tx.commit()?;
        Ok(id)
    }
    pub(crate) fn availability_map(&self) -> Result<HashMap<i64, (bool, Tz)>> {
        let mut stmt=self.conn().prepare("SELECT e.id,COALESCE(d.busy,NOT e.all_day),e.tz FROM events e LEFT JOIN event_details d ON d.event_id=e.id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?, r.get::<_, String>(2)?)))?;
        let mut result = HashMap::new();
        for row in rows {
            let (id, busy, zone) = row?;
            let zone = zone.parse::<Tz>().map_err(|_| Error::Invalid("Unknown event timezone".into()))?;
            result.insert(id, (busy, zone));
        }
        Ok(result)
    }
}
