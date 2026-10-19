//! Guest capabilities and transactional booking management.
use crate::{
    db::{Booking, Db, Error, Result},
    delivery::{hash, time},
};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
#[derive(Serialize)]
pub struct BookingReceipt {
    pub booking: Booking,
    pub token: String,
}
impl Db {
    pub fn booking(&self, id: i64) -> Result<Option<Booking>> {
        Ok(self.conn().query_row("SELECT b.id,b.event_type_id,t.title,b.start,b.end,b.guest_name,b.guest_email,b.guest_note,b.guest_tz,b.status,b.created_at FROM bookings b JOIN event_types t ON t.id=b.event_type_id WHERE b.id=?1",[id],|r|Ok(Booking{id:r.get(0)?,event_type_id:r.get(1)?,type_title:r.get(2)?,start:r.get(3)?,end:r.get(4)?,guest_name:r.get(5)?,guest_email:r.get(6)?,guest_note:r.get(7)?,guest_tz:r.get(8)?,status:r.get(9)?,created_at:r.get(10)?})).optional()?)
    }
    pub fn managed_booking(&self, token: &str) -> Result<Booking> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::NotFound("Booking link not found".into()));
        }
        let id = self
            .conn()
            .query_row("SELECT booking_id FROM booking_access WHERE token_hash=?1", [hash(token)], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .ok_or_else(|| Error::NotFound("Booking link not found".into()))?;
        self.booking(id)?.ok_or_else(|| Error::NotFound("Booking link not found".into()))
    }
    pub fn reschedule_slots(
        &self,
        token: &str,
        guest_tz: Tz,
        organizer_tz: Tz,
        now: DateTime<Utc>,
        window: (DateTime<Utc>, DateTime<Utc>),
    ) -> Result<Vec<DateTime<Utc>>> {
        let booking = self.managed_booking(token)?;
        self.check_manageable(&booking, now)?;
        let slug = self.conn().query_row("SELECT slug FROM event_types WHERE id=?1", [booking.event_type_id], |r| {
            r.get::<_, String>(0)
        })?;
        let t = self.event_type_by_slug(&slug)?.ok_or_else(|| Error::NotFound("Booking type not found".into()))?;
        if !t.active {
            return Err(Error::Invalid("This booking type no longer accepts changes".into()));
        }
        self.slots_for_excluding(&t, organizer_tz, guest_tz, now, window, Some(booking.id))
    }
    fn check_manageable(&self, b: &Booking, now: DateTime<Utc>) -> Result<()> {
        if b.status != "confirmed"
            || b.start.parse::<DateTime<Utc>>().map_err(|_| Error::Invalid("Invalid booking time".into()))? <= now
        {
            return Err(Error::Invalid("Only upcoming confirmed bookings can be changed".into()));
        }
        Ok(())
    }
    pub fn reschedule(
        &self,
        token: &str,
        start: &str,
        guest_zone: &str,
        organizer_tz: Tz,
        now: DateTime<Utc>,
    ) -> Result<Booking> {
        let tx = rusqlite::Transaction::new_unchecked(self.conn(), rusqlite::TransactionBehavior::Immediate)?;
        let mut b = self.managed_booking(token)?;
        self.check_manageable(&b, now)?;
        let start = start.parse::<DateTime<Utc>>().map_err(|_| Error::Invalid("Invalid new booking time".into()))?;
        let guest_tz = guest_zone.parse::<Tz>().map_err(|_| Error::Invalid("Unknown guest timezone".into()))?;
        if time(start) == b.start {
            return Ok(b);
        }
        if !self
            .reschedule_slots(
                token,
                guest_tz,
                organizer_tz,
                now,
                (start - Duration::days(1), start + Duration::days(1)),
            )?
            .contains(&start)
        {
            return Err(Error::Taken);
        }
        let minutes =
            self.conn().query_row("SELECT duration_min FROM event_types WHERE id=?1", [b.event_type_id], |r| {
                r.get::<_, i64>(0)
            })?;
        let end = start + Duration::minutes(minutes);
        tx.execute(
            "UPDATE bookings SET start=?2,end=?3,guest_tz=?4 WHERE id=?1",
            params![b.id, time(start), time(end), guest_zone],
        )?;
        if tx.execute(
            "UPDATE events SET start=?2,end=?3,updated_at=?4 WHERE booking_id=?1",
            params![b.id, time(start), time(end), time(now)],
        )? != 1
        {
            return Err(Error::Invalid("Booking calendar entry is missing".into()));
        }
        b.start = time(start);
        b.end = time(end);
        b.guest_tz = guest_zone.into();
        self.supersede_booking_mail(b.id)?;
        self.queue_booking_mail(&b, token, "rescheduled", now)?;
        tx.commit()?;
        Ok(b)
    }
    pub fn cancel_managed_booking(&self, token: &str, now: DateTime<Utc>) -> Result<Booking> {
        let b = self.managed_booking(token)?;
        if b.status == "cancelled" {
            return Ok(b);
        }
        self.check_manageable(&b, now)?;
        self.cancel_booking_notified(b.id, now)?;
        self.managed_booking(token)
    }
    pub fn cancel_booking_notified(&self, id: i64, now: DateTime<Utc>) -> Result<()> {
        let tx = rusqlite::Transaction::new_unchecked(self.conn(), rusqlite::TransactionBehavior::Immediate)?;
        let mut b = self.booking(id)?.ok_or_else(|| Error::NotFound("Booking not found".into()))?;
        if b.status == "cancelled" {
            return Ok(());
        }
        let token = self.ensure_booking_token(id)?;
        self.supersede_booking_mail(id)?;
        tx.execute("UPDATE bookings SET status='cancelled' WHERE id=?1", [id])?;
        tx.execute("DELETE FROM events WHERE booking_id=?1", [id])?;
        b.status = "cancelled".into();
        self.queue_booking_mail(&b, &token, "cancelled", now)?;
        tx.commit()?;
        Ok(())
    }
}
