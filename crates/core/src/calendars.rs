//! Calendar organization is separate from booking availability: hidden is a UI preference.
use crate::db::{Db, Error, Event, Result};
use crate::recur::Occurrence;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub visible: bool,
}

fn metadata(name: &str, color: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 100 {
        return Err(Error::Invalid("Calendar name must be 1–100 bytes".into()));
    }
    if color.len() != 7 || !color.starts_with('#') || !color[1..].bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Invalid("Choose a color in #rrggbb format".into()));
    }
    Ok(())
}

impl Db {
    pub fn calendars(&self) -> Result<Vec<Calendar>> {
        let mut stmt = self.conn().prepare("SELECT id,name,color,visible FROM calendars ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| Ok(Calendar { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, visible: r.get(3)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn create_calendar(&self, name: &str, color: &str) -> Result<i64> {
        metadata(name, color)?;
        self.conn().execute("INSERT INTO calendars (name,color) VALUES (?1,?2)", params![name.trim(), color])?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn update_calendar(&self, id: i64, name: &str, color: &str, visible: bool) -> Result<()> {
        metadata(name, color)?;
        let changed = self.conn().execute(
            "UPDATE calendars SET name=?2,color=?3,visible=?4 WHERE id=?1",
            params![id, name.trim(), color, visible],
        )?;
        if changed == 0 {
            return Err(Error::NotFound("Calendar no longer exists".into()));
        }
        Ok(())
    }

    /// Preserve events and UIDs by moving them to Personal. In the rare case
    /// of a duplicate imported UID, assign a fresh UID to the moved copy.
    pub fn delete_calendar(&self, id: i64) -> Result<()> {
        if id == 1 {
            return Err(Error::Invalid("Personal is the default calendar and cannot be deleted".into()));
        }
        let tx = self.conn().unchecked_transaction()?;
        tx.execute(
            "UPDATE event_uids SET uid=lower(hex(randomblob(16))) || '@calendio.local'
            WHERE calendar_id=?1 AND uid IN (SELECT uid FROM event_uids WHERE calendar_id=1)",
            [id],
        )?;
        tx.execute("UPDATE event_uids SET calendar_id=1 WHERE calendar_id=?1", [id])?;
        tx.execute("UPDATE events SET calendar_id=1 WHERE calendar_id=?1", [id])?;
        tx.execute("DELETE FROM calendars WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn event_calendar(&self, id: i64) -> Result<i64> {
        Ok(self.conn().query_row("SELECT calendar_id FROM events WHERE id=?1", [id], |r| r.get(0))?)
    }

    pub fn assign_calendar(&self, event_id: i64, calendar_id: i64) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        let changed = tx.execute("UPDATE events SET calendar_id=?2 WHERE id=?1", params![event_id, calendar_id])?;
        if changed == 0 {
            return Err(Error::NotFound("Event no longer exists".into()));
        }
        tx.execute("UPDATE event_uids SET calendar_id=?2 WHERE event_id=?1", params![event_id, calendar_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Literal Unicode-insensitive search, never LIKE patterns or SQL fragments.
    pub fn search_events(&self, query: &str) -> Result<Vec<Event>> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .all_events()?
            .into_iter()
            .filter(|e| e.title.to_lowercase().contains(&needle) || e.notes.to_lowercase().contains(&needle))
            .collect())
    }

    pub fn visible_occurrences(&self, from: &str, to: &str) -> Result<Vec<Occurrence>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT e.id,c.color FROM events e JOIN calendars c ON c.id=e.calendar_id WHERE c.visible=1")?;
        let colors = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(self
            .occurrences_between(from, to)?
            .into_iter()
            .filter_map(|mut o| {
                let color = colors.get(&o.event_id)?;
                if o.color.is_none() {
                    o.color = Some(color.clone());
                }
                Some(o)
            })
            .collect())
    }
}
