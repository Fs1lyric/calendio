//! Durable notification outbox. A lease never spans a database lock or HTTP call.
use crate::db::{Booking, Db, Error, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub fn random_secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Invalid("Secure random source is unavailable".into()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
pub(crate) fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn time(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Secs, true)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct MailPayload {
    pub booking: Booking,
    pub token: String,
}
pub struct EmailJob {
    pub id: i64,
    pub kind: String,
    pub payload: MailPayload,
    pub lease: String,
}
impl EmailJob {
    pub fn idempotency_key(&self) -> String {
        format!(
            "calendio-{}-{}",
            self.id,
            hash(&format!("{}:{}", self.kind, serde_json::to_string(&self.payload).unwrap_or_default()))
        )
    }
}
impl Db {
    pub(crate) fn ensure_booking_token(&self, id: i64) -> Result<String> {
        let payload = self
            .conn()
            .query_row("SELECT payload FROM email_jobs WHERE booking_id=?1 ORDER BY id LIMIT 1", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        if let Some(payload) = payload {
            return serde_json::from_str::<MailPayload>(&payload)
                .map(|p| p.token)
                .map_err(|_| Error::Invalid("Invalid notification data".into()));
        }
        let token = random_secret()?;
        self.conn().execute(
            "INSERT OR IGNORE INTO booking_access(booking_id,token_hash) VALUES (?1,?2)",
            params![id, hash(&token)],
        )?;
        Ok(token)
    }
    pub(crate) fn queue_booking_mail(
        &self,
        booking: &Booking,
        token: &str,
        kind: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let version =
            self.conn().query_row("SELECT version FROM booking_access WHERE booking_id=?1", [booking.id], |r| {
                r.get::<_, i64>(0)
            })?;
        let payload = serde_json::to_string(&MailPayload { booking: booking.clone(), token: token.into() })
            .map_err(|_| Error::Invalid("Cannot encode notification".into()))?;
        self.conn().execute(
            "INSERT OR IGNORE INTO email_jobs(booking_id,kind,version,payload,next_attempt) VALUES (?1,?2,?3,?4,?5)",
            params![booking.id, kind, version, payload, time(now)],
        )?;
        if booking.status == "confirmed" {
            let start =
                booking.start.parse::<DateTime<Utc>>().map_err(|_| Error::Invalid("Invalid booking time".into()))?;
            let due = start - Duration::minutes(10);
            if due > now {
                self.conn().execute("INSERT OR IGNORE INTO email_jobs(booking_id,kind,version,payload,next_attempt) VALUES (?1,'reminder',?2,?3,?4)",params![booking.id,version,payload,time(due)])?;
            }
        }
        Ok(())
    }
    pub(crate) fn supersede_booking_mail(&self, id: i64) -> Result<()> {
        self.conn().execute("UPDATE email_jobs SET status='superseded',lease=NULL,lease_until=NULL WHERE booking_id=?1 AND status IN ('pending','sending','failed')",[id])?;
        self.conn().execute("UPDATE booking_access SET version=version+1 WHERE booking_id=?1", [id])?;
        Ok(())
    }
    pub fn claim_email(&self, now: DateTime<Utc>) -> Result<Option<EmailJob>> {
        let tx = rusqlite::Transaction::new_unchecked(self.conn(), rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("UPDATE email_jobs SET status='failed',lease=NULL,lease_until=NULL WHERE status='sending' AND attempts>=8 AND lease_until<=?1", [time(now)])?;
        let row=tx.query_row("SELECT id,kind,payload FROM email_jobs WHERE attempts<8 AND next_attempt<=?1 AND (status='pending' OR (status='sending' AND lease_until<=?1)) ORDER BY next_attempt,id LIMIT 1",[time(now)],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).optional()?;
        let Some((id, kind, payload)) = row else {
            tx.commit()?;
            return Ok(None);
        };
        let lease = random_secret()?;
        tx.execute(
            "UPDATE email_jobs SET status='sending',lease=?2,lease_until=?3,attempts=attempts+1 WHERE id=?1",
            params![id, lease, time(now + Duration::minutes(2))],
        )?;
        let payload =
            serde_json::from_str(&payload).map_err(|_| Error::Invalid("Invalid notification payload".into()))?;
        tx.commit()?;
        Ok(Some(EmailJob { id, kind, payload, lease }))
    }
    pub fn email_is_current(&self, id: i64, lease: &str) -> Result<bool> {
        Ok(self.conn().query_row("SELECT EXISTS(SELECT 1 FROM email_jobs j JOIN booking_access a ON a.booking_id=j.booking_id JOIN bookings b ON b.id=j.booking_id WHERE j.id=?1 AND j.lease=?2 AND j.status='sending' AND a.version=j.version AND (j.kind='cancelled' OR b.status='confirmed'))",params![id,lease],|r|r.get(0))?)
    }
    pub fn email_sent(&self, id: i64, lease: &str, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute("UPDATE email_jobs SET status='sent',delivered_at=?3,lease=NULL,lease_until=NULL WHERE id=?1 AND lease=?2 AND status='sending'",params![id,lease,time(now)])?;
        Ok(())
    }
    pub fn email_failed(&self, id: i64, lease: &str, now: DateTime<Utc>) -> Result<()> {
        let attempts =
            self.conn().query_row("SELECT attempts FROM email_jobs WHERE id=?1", [id], |r| r.get::<_, u32>(0))?;
        let delay = 30i64 * 2i64.pow(attempts.min(10));
        self.conn().execute("UPDATE email_jobs SET status=CASE WHEN attempts>=8 THEN 'failed' ELSE 'pending' END,next_attempt=?3,lease=NULL,lease_until=NULL WHERE id=?1 AND lease=?2 AND status='sending'",params![id,lease,time(now+Duration::seconds(delay.min(21600)))])?;
        Ok(())
    }
    pub fn email_counts(&self) -> Result<(i64, i64)> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(status IN ('pending','sending')),0),COALESCE(SUM(status='failed'),0) FROM email_jobs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }
    pub fn retry_failed_email(&self, now: DateTime<Utc>) -> Result<()> {
        self.conn().execute(
            "UPDATE email_jobs SET status='pending',attempts=0,next_attempt=?1 WHERE status='failed'",
            [time(now)],
        )?;
        Ok(())
    }
}
