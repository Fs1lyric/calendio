//! Refreshes subscribed iCal feeds every 6 hours, or at once when the app asks.
//! The calendar mutex is never held while a feed downloads.
use crate::State;
use calendio_core::subscriptions::Subscription;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;

/// Feeds bigger than this are refused.
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
/// The app sets this setting to ask for a refresh now.
pub const REFRESH_SETTING: &str = "subscriptions_refresh";
const EVERY: i64 = 6;

pub struct Fetched {
    pub status: u16,
    pub etag: Option<String>,
    pub body: String,
}
pub trait Fetcher {
    async fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String>;
}

pub struct Http(reqwest::Client);
impl Http {
    pub fn new() -> Result<Self, String> {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            // Feed hosts often redirect (http → https, a CDN); a few hops are fine.
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent("Calendio")
            .build()
            .map(Self)
            .map_err(|_| "Cannot initialize HTTP client".into())
    }
}
impl Fetcher for Http {
    async fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
        let mut request = self.0.get(url).header("Accept", "text/calendar, */*;q=0.5");
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                "The feed took longer than 20 seconds".to_owned()
            } else {
                "Cannot reach the feed".to_owned()
            }
        })?;
        let status = response.status().as_u16();
        let etag = response.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "The feed download was interrupted")? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err("The feed is larger than 5 MB".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(bytes).map_err(|_| "The feed is not UTF-8 text")?;
        Ok(Fetched { status, etag, body })
    }
}

fn due(sub: &Subscription, now: DateTime<Utc>, force: bool) -> bool {
    force
        || sub
            .last_fetched
            .as_deref()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())
            .is_none_or(|t| now - t >= Duration::hours(EVERY))
}

pub fn status_key(calendar_id: i64) -> String {
    format!("subscription_status_{calendar_id}")
}

/// Fetches every due feed and stores the result. Returns how many feeds were tried.
pub async fn refresh<F: Fetcher>(state: &Arc<State>, fetcher: &F, now: DateTime<Utc>) -> usize {
    let (subs, force) = {
        let db = state.db.lock().unwrap();
        let force = db.setting(REFRESH_SETTING).ok().flatten().is_some_and(|v| !v.is_empty());
        if force {
            let _ = db.set_setting(REFRESH_SETTING, "");
        }
        (db.subscriptions().unwrap_or_default(), force)
    };
    let mut tried = 0;
    for sub in subs.iter().filter(|s| due(s, now, force)) {
        tried += 1;
        let fetched = fetcher.get(&sub.url, sub.etag.as_deref()).await;
        let db = state.db.lock().unwrap();
        let status = match fetched {
            Ok(f) if f.status == 304 => {
                db.mark_fetched(sub.calendar_id, sub.etag.as_deref(), now).map(|_| "Up to date".to_owned())
            }
            Ok(f) if (200..300).contains(&f.status) => db
                .replace_subscription(sub.calendar_id, &f.body, f.etag.as_deref(), now, state.tz.name())
                .map(|r| format!("{} events", r.imported)),
            Ok(f) => Err(calendio_core::db::Error::Invalid(format!("The feed answered HTTP {}", f.status))),
            Err(error) => Err(calendio_core::db::Error::Invalid(error)),
        };
        let line = match status {
            Ok(line) => format!("Updated {}: {line}", now.to_rfc3339()),
            Err(error) => {
                // Wait the full interval before retrying a failing feed, keeping its old events.
                let _ = db.mark_fetched(sub.calendar_id, sub.etag.as_deref(), now);
                format!("Failed {}: {error}", now.to_rfc3339())
            }
        };
        let _ = db.set_setting(&status_key(sub.calendar_id), &line);
    }
    tried
}

#[cfg(test)]
mod tests {
    use super::*;
    use calendio_core::db::Db;
    use std::{collections::VecDeque, sync::Mutex};

    struct Fake {
        replies: Mutex<VecDeque<Result<Fetched, String>>>,
        calls: Mutex<Vec<(String, Option<String>)>>,
    }
    impl Fetcher for Fake {
        async fn get(&self, url: &str, etag: Option<&str>) -> Result<Fetched, String> {
            self.calls.lock().unwrap().push((url.into(), etag.map(str::to_owned)));
            self.replies.lock().unwrap().pop_front().unwrap_or(Err("Unexpected fetch".into()))
        }
    }
    fn fake(replies: Vec<Result<Fetched, String>>) -> Fake {
        Fake { replies: Mutex::new(replies.into()), calls: Mutex::new(vec![]) }
    }
    fn ok(body: &str, etag: &str) -> Result<Fetched, String> {
        Ok(Fetched { status: 200, etag: Some(etag.into()), body: body.into() })
    }
    fn feed(titles: &[&str]) -> String {
        let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//T//EN\r\n");
        for (i, t) in titles.iter().enumerate() {
            out.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:{i}\r\nDTSTAMP:20261001T000000Z\r\nDTSTART;VALUE=DATE:2026122{i}\r\nSUMMARY:{t}\r\nEND:VEVENT\r\n"
            ));
        }
        out + "END:VCALENDAR\r\n"
    }
    fn now() -> DateTime<Utc> {
        "2026-11-08T09:00:00Z".parse().unwrap()
    }
    fn state() -> Arc<State> {
        Arc::new(State { db: Mutex::new(Db::open_in_memory().unwrap()), tz: chrono_tz::Asia::Dhaka, now })
    }
    fn titles(state: &State) -> Vec<String> {
        state.db.lock().unwrap().all_events().unwrap().into_iter().map(|e| e.input.title).collect()
    }

    #[tokio::test]
    async fn fetches_new_feeds_then_waits_six_hours_and_sends_the_etag() {
        let state = state();
        let id = state.db.lock().unwrap().create_subscription("Holidays", "#ff9500", "webcal://x.test/h.ics").unwrap();
        let fetcher = fake(vec![
            ok(&feed(&["Christmas"]), "\"v1\""),
            Ok(Fetched { status: 304, etag: None, body: String::new() }),
            ok(&feed(&["Christmas", "Boxing Day"]), "\"v2\""),
        ]);
        assert_eq!(refresh(&state, &fetcher, now()).await, 1);
        assert_eq!(titles(&state), ["Christmas"]);
        assert_eq!(refresh(&state, &fetcher, now() + Duration::hours(5)).await, 0, "not due yet");
        assert_eq!(refresh(&state, &fetcher, now() + Duration::hours(6)).await, 1);
        assert_eq!(titles(&state), ["Christmas"], "304 keeps the events");
        state.db.lock().unwrap().set_setting(REFRESH_SETTING, "1").unwrap();
        assert_eq!(refresh(&state, &fetcher, now() + Duration::hours(7)).await, 1, "on demand");
        assert_eq!(titles(&state).len(), 2);
        let calls = fetcher.calls.lock().unwrap();
        assert_eq!(calls[0], ("https://x.test/h.ics".into(), None));
        assert_eq!(calls[1].1.as_deref(), Some("\"v1\""));
        assert_eq!(calls[2].1.as_deref(), Some("\"v1\""), "a 304 keeps the old etag");
        let db = state.db.lock().unwrap();
        assert_eq!(db.subscriptions().unwrap()[0].etag.as_deref(), Some("\"v2\""));
        assert!(db.setting(&status_key(id)).unwrap().unwrap().starts_with("Updated"));
        assert_eq!(db.setting(REFRESH_SETTING).unwrap().as_deref(), Some(""));
    }

    #[tokio::test]
    async fn a_failing_feed_keeps_its_events_and_reports_the_error() {
        let state = state();
        let id = state.db.lock().unwrap().create_subscription("Holidays", "#ff9500", "https://x.test/h.ics").unwrap();
        let fetcher = fake(vec![
            ok(&feed(&["Christmas"]), "\"v1\""),
            Ok(Fetched { status: 404, etag: None, body: String::new() }),
            Err("The feed is larger than 5 MB".into()),
            ok("<html>login page</html>", "\"x\""),
        ]);
        refresh(&state, &fetcher, now()).await;
        for hours in [6, 12, 18] {
            assert_eq!(refresh(&state, &fetcher, now() + Duration::hours(hours)).await, 1);
            assert_eq!(titles(&state), ["Christmas"]);
            let status = state.db.lock().unwrap().setting(&status_key(id)).unwrap().unwrap();
            assert!(status.starts_with("Failed"), "{status}");
        }
        let status = state.db.lock().unwrap().setting(&status_key(id)).unwrap().unwrap();
        assert!(!status.contains("404"), "the last error replaced the first: {status}");
    }

    #[test]
    fn due_rules() {
        let sub = |last: Option<&str>| Subscription {
            calendar_id: 2,
            name: "H".into(),
            url: "https://x.test".into(),
            last_fetched: last.map(str::to_owned),
            etag: None,
        };
        assert!(due(&sub(None), now(), false));
        assert!(!due(&sub(Some("2026-11-08T08:00:00Z")), now(), false));
        assert!(due(&sub(Some("2026-11-08T08:00:00Z")), now(), true));
        assert!(due(&sub(Some("2026-11-08T03:00:00Z")), now(), false));
        assert!(due(&sub(Some("garbage")), now(), false));
    }
}
