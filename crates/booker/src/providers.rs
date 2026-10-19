//! Fixed-origin network adapters. Credentials are never logged or forwarded on redirects.
use crate::google::{Api, Reply, Request};
use calendio_core::delivery::EmailJob;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot initialize HTTP client".into())
}
pub async fn json_response(mut response: reqwest::Response) -> Result<(u16, Value), String> {
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "Provider response was interrupted")? {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err("Provider response exceeded 8 MiB".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).map_err(|_| "Provider returned invalid JSON")?
    };
    Ok((status, body))
}
pub fn google_url(calendar: &str, event: Option<&str>, query: &[(String, String)]) -> Result<url::Url, String> {
    let mut url = url::Url::parse("https://www.googleapis.com/calendar/v3/").map_err(|_| "Invalid Google base URL")?;
    {
        let mut parts = url.path_segments_mut().map_err(|_| "Invalid Google URL")?;
        parts.pop_if_empty().push("calendars").push(calendar).push("events");
        if let Some(event) = event {
            parts.push(event);
        }
    }
    url.query_pairs_mut().extend_pairs(query.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    Ok(url)
}
pub fn public_origin(value: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(value).map_err(|_| "CALENDIO_PUBLIC_URL must be an absolute origin")?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if (url.scheme() != "https" && !(url.scheme() == "http" && local))
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Public URL must be an HTTPS origin (HTTP is allowed only for localhost)".into());
    }
    Ok(url)
}
#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
}
impl Credentials {
    pub fn load() -> Result<Option<Self>, String> {
        let id = std::env::var("CALENDIO_GOOGLE_CLIENT_ID").ok();
        let secret = std::env::var("CALENDIO_GOOGLE_CLIENT_SECRET").ok();
        let refresh = std::env::var("CALENDIO_GOOGLE_REFRESH_TOKEN").ok();
        if let (Some(client_id), Some(refresh_token)) = (id.clone(), refresh) {
            return Ok(Some(Self { client_id, client_secret: secret.unwrap_or_default(), refresh_token }));
        }
        let output =
            Command::new("secret-tool").args(["lookup", "application", "calendio", "account", "google"]).output();
        if let Ok(output) = output {
            if output.status.success() && !output.stdout.is_empty() {
                return serde_json::from_slice(&output.stdout)
                    .map(Some)
                    .map_err(|_| "Google keyring entry is invalid".into());
            }
        }
        if id.is_some() {
            return Err("Complete Google sign-in with calendio-booker --google-connect".into());
        }
        Ok(None)
    }
    pub fn store(&self) -> Result<(), String> {
        let mut process = Command::new("secret-tool")
            .args(["store", "--label=Calendio Google Calendar", "application", "calendio", "account", "google"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "Install secret-tool to store Google credentials in the desktop keyring")?;
        let encoded = serde_json::to_vec(self).map_err(|_| "Cannot encode Google credentials")?;
        process
            .stdin
            .take()
            .ok_or("Keyring input unavailable")?
            .write_all(&encoded)
            .map_err(|_| "Cannot write to desktop keyring")?;
        if !process.wait().map_err(|_| "Keyring did not finish")?.success() {
            return Err("Desktop keyring is locked or unavailable".into());
        }
        Ok(())
    }
    pub async fn access_token(&self, http: &reqwest::Client) -> Result<String, String> {
        let response = http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
            ])
            .send()
            .await
            .map_err(|_| "Google token refresh could not connect")?;
        let (status, body) = json_response(response).await?;
        if status != 200 {
            return Err(format!("Google authorization needs attention ({status})"));
        }
        body["access_token"].as_str().map(str::to_owned).ok_or("Google did not return an access token".into())
    }
}
pub struct GoogleApi {
    pub http: reqwest::Client,
    pub token: String,
    pub calendar: String,
}
impl Api for GoogleApi {
    async fn call(&self, request: Request) -> Result<Reply, String> {
        let url = google_url(&self.calendar, request.event.as_deref(), &request.query)?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|_| "Invalid provider method")?;
        let mut outgoing = self.http.request(method, url).bearer_auth(&self.token);
        if let Some(body) = request.body {
            outgoing = outgoing.json(&body);
        }
        if let Some(etag) = request.etag {
            outgoing = outgoing.header(reqwest::header::IF_MATCH, etag);
        }
        let response = outgoing.send().await.map_err(|_| "Google Calendar connection failed")?;
        let (status, body) = json_response(response).await?;
        Ok(Reply { status, body })
    }
}
pub struct EmailSender {
    http: reqwest::Client,
    key: String,
    from: String,
    organizer: Option<String>,
    origin: url::Url,
}
impl EmailSender {
    pub fn from_env(http: reqwest::Client) -> Result<Option<Self>, String> {
        let Ok(key) = std::env::var("CALENDIO_RESEND_API_KEY") else { return Ok(None) };
        let from =
            std::env::var("CALENDIO_EMAIL_FROM").map_err(|_| "Set CALENDIO_EMAIL_FROM to your verified sender")?;
        let origin = public_origin(
            &std::env::var("CALENDIO_PUBLIC_URL").map_err(|_| "Set CALENDIO_PUBLIC_URL before sending email")?,
        )?;
        let organizer = std::env::var("CALENDIO_ORGANIZER_EMAIL").ok().filter(|v| !v.trim().is_empty());
        Ok(Some(Self { http, key, from, organizer, origin }))
    }
    fn payload(&self, job: &EmailJob) -> Result<Value, String> {
        let booking = &job.payload.booking;
        let zone = booking.guest_tz.parse::<chrono_tz::Tz>().map_err(|_| "Invalid guest timezone")?;
        let at = booking.start.parse::<DateTime<Utc>>().map_err(|_| "Invalid booking date")?.with_timezone(&zone);
        let action = match job.kind.as_str() {
            "confirmation" => "Booking confirmed",
            "rescheduled" => "Booking rescheduled",
            "cancelled" => "Booking cancelled",
            "reminder" => "Upcoming booking",
            _ => return Err("Unknown email job".into()),
        };
        let link =
            self.origin.join(&format!("manage/{}", job.payload.token)).map_err(|_| "Cannot build management link")?;
        let text=format!("Hi {},\n\n{}: {}\nWhen: {} ({})\n\nChange or cancel your booking: {}\n\nKeep this private link for your records.",booking.guest_name,action,booking.type_title,at.format("%A %-d %B %Y at %H:%M"),zone,link).replace("\n","
");
        let subject = format!("{action}: {}", booking.type_title).replace(['\r', '\n'], " ");
        let mut payload = json!({"from":self.from,"to":[booking.guest_email],"subject":subject,"text":text});
        if let Some(organizer) = &self.organizer {
            payload["bcc"] = json!([organizer]);
        }
        Ok(payload)
    }
    pub async fn send(&self, job: &EmailJob) -> Result<(), String> {
        let response = self
            .http
            .post("https://api.resend.com/emails")
            .bearer_auth(&self.key)
            .header("Idempotency-Key", job.idempotency_key())
            .json(&self.payload(job)?)
            .send()
            .await
            .map_err(|_| "Email provider connection failed")?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("Email provider rejected delivery ({status})"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_never_follow_a_caller_supplied_origin() {
        let url = google_url("a/b@example.com", Some("https://evil.example/token"), &[]).unwrap();
        assert_eq!(url.host_str(), Some("www.googleapis.com"));
        assert!(url.path().contains("a%2Fb@example.com"));
        assert!(!url.as_str().contains("https://evil.example"));
    }
    #[test]
    fn management_links_are_built_from_the_configured_origin() {
        assert!(public_origin("javascript:alert(1)").is_err());
        assert!(public_origin("https://u:p@example.com").is_err());
        assert!(public_origin("https://example.com/path").is_err());
        assert_eq!(public_origin("https://calendar.example.com/").unwrap().as_str(), "https://calendar.example.com/");
    }
}
