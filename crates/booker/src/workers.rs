//! Runtime scheduling. No network request holds the calendar mutex.
use crate::{
    providers::{self, Credentials, EmailSender, GoogleApi},
    State,
};
use chrono::{Duration, Utc};
use std::sync::Arc;
pub async fn run(state: Arc<State>) {
    if std::env::var_os("CALENDIO_DISABLE_INTEGRATIONS").is_some() {
        return;
    }
    let http = match providers::client() {
        Ok(http) => http,
        Err(error) => {
            eprintln!("calendio-booker: {error}");
            return;
        }
    };
    let mail = match EmailSender::from_env(http.clone()) {
        Ok(mail) => mail,
        Err(error) => {
            let _ = state.db.lock().unwrap().set_setting("email_status", &error);
            None
        }
    };
    if mail.is_none() && std::env::var_os("CALENDIO_RESEND_API_KEY").is_none() {
        let _ = state.db.lock().unwrap().set_setting("email_status", "Not configured; messages remain queued");
    }
    match crate::subscriptions::Http::new() {
        Ok(fetcher) => {
            let state = state.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
                loop {
                    tick.tick().await;
                    crate::subscriptions::refresh(&state, &fetcher, (state.now)()).await;
                }
            });
        }
        Err(error) => eprintln!("calendio-booker: subscriptions: {error}"),
    }
    let mut last_google = Utc::now() - Duration::minutes(2);
    let mut last_backup = None;
    let mut next_backup_try = chrono::DateTime::<Utc>::MIN_UTC;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        tick.tick().await;
        let now = (state.now)();
        {
            let db = state.db.lock().unwrap();
            let _ = db.set_setting("booker_heartbeat", &now.to_rfc3339());
            if last_backup != Some(now.date_naive()) && next_backup_try <= now {
                let directory = crate::backups::directory();
                let status = crate::backups::daily(&db, &directory, now);
                if status.is_ok() {
                    last_backup = Some(now.date_naive());
                } else {
                    // An unplugged mirror stick retries every 10 minutes, not every tick.
                    next_backup_try = now + Duration::minutes(10);
                }
                let _ = db.set_setting("backup_status", &status.unwrap_or_else(|e| e));
            }
        }
        if let Some(sender) = &mail {
            for _ in 0..10 {
                let job = state.db.lock().unwrap().claim_email(now);
                let job = match job {
                    Ok(Some(job)) => job,
                    Ok(None) => break,
                    Err(error) => {
                        eprintln!("calendio-booker: email queue: {error}");
                        break;
                    }
                };
                let current = state.db.lock().unwrap().email_is_current(job.id, &job.lease).unwrap_or(false);
                if !current {
                    continue;
                }
                if job.kind == "reminder"
                    && job.payload.booking.start.parse::<chrono::DateTime<Utc>>().is_ok_and(|t| t <= now)
                {
                    let db = state.db.lock().unwrap();
                    let _ = db.conn().execute(
                        "UPDATE email_jobs SET status='superseded',lease=NULL,lease_until=NULL WHERE id=?1",
                        [job.id],
                    );
                    continue;
                }
                let delivered = sender.send(&job).await;
                let db = state.db.lock().unwrap();
                match delivered {
                    Ok(()) => {
                        let _ = db.email_sent(job.id, &job.lease, now);
                        let _ = db.set_setting("email_status", "Email delivery operational");
                    }
                    Err(error) => {
                        let _ = db.email_failed(job.id, &job.lease, now);
                        let _ = db.set_setting("email_status", &error);
                    }
                }
            }
        }
        if now - last_google >= Duration::minutes(1) {
            last_google = now;
            let result = match Credentials::load() {
                Ok(Some(credentials)) => match credentials.access_token(&http).await {
                    Ok(token) => {
                        let api = GoogleApi {
                            http: http.clone(),
                            token,
                            calendar: std::env::var("CALENDIO_GOOGLE_CALENDAR_ID").unwrap_or_else(|_| "primary".into()),
                        };
                        let synced = crate::google::sync(&state, &api, now).await;
                        if synced.is_ok() {
                            let _ = state.db.lock().unwrap().set_setting("google_last_sync", &now.to_rfc3339());
                        }
                        synced.map(|s| {
                            format!("Synced: {} pulled, {} pushed, {} conflict copies", s.pulled, s.pushed, s.conflicts)
                        })
                    }
                    Err(error) => Err(error),
                },
                Ok(None) => Ok("Not connected".into()),
                Err(error) => Err(error),
            };
            let db = state.db.lock().unwrap();
            let _ = db.set_setting("google_status", &result.unwrap_or_else(|e| e));
        }
    }
}
