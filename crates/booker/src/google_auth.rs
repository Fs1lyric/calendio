//! Installed-app OAuth with PKCE, loopback-only callback and desktop keyring storage.
use crate::providers::{self, Credentials};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn authorization_url(client: &str, redirect: &str, state: &str, verifier: &str) -> Result<url::Url, String> {
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_err(|_| "Invalid authorization endpoint")?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    url.query_pairs_mut().extend_pairs([
        ("client_id", client),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", "https://www.googleapis.com/auth/calendar.events"),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("state", state),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]);
    Ok(url)
}
fn callback(path: &str, state: &str) -> Result<String, String> {
    let url = url::Url::parse(&format!("http://127.0.0.1{path}")).map_err(|_| "Invalid OAuth callback")?;
    if url.path() != "/oauth/callback" {
        return Err("Invalid callback path".into());
    }
    let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if query.get("state").map(String::as_str) != Some(state) {
        return Err("OAuth state did not match".into());
    }
    if query.contains_key("error") {
        return Err("Google authorization was declined".into());
    }
    query.get("code").cloned().filter(|s| !s.is_empty()).ok_or("Google did not return an authorization code".into())
}
pub async fn connect() -> Result<(), String> {
    let client_id = std::env::var("CALENDIO_GOOGLE_CLIENT_ID")
        .map_err(|_| "Set CALENDIO_GOOGLE_CLIENT_ID to a Desktop OAuth client ID first")?;
    let client_secret = std::env::var("CALENDIO_GOOGLE_CLIENT_SECRET").unwrap_or_default();
    if !std::process::Command::new("secret-tool")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
    {
        return Err("Install secret-tool for secure credential storage".into());
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| "Cannot open local Google sign-in callback")?;
    let port = listener.local_addr().map_err(|_| "Cannot read callback port")?.port();
    let redirect = format!("http://127.0.0.1:{port}/oauth/callback");
    let state = calendio_core::delivery::random_secret().map_err(|_| "Secure random source unavailable")?;
    let verifier = calendio_core::delivery::random_secret().map_err(|_| "Secure random source unavailable")?;
    let url = authorization_url(&client_id, &redirect, &state, &verifier)?;
    std::process::Command::new("xdg-open")
        .arg(url.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "Cannot open browser for Google sign-in")?;
    let code=tokio::time::timeout(std::time::Duration::from_secs(180),async{for _ in 0..20{let (mut stream,_)=listener.accept().await.map_err(|_|"Callback connection failed")?;let mut buffer=vec![0;8192];let count=tokio::time::timeout(std::time::Duration::from_secs(5),stream.read(&mut buffer)).await.map_err(|_|"OAuth callback timed out")?.map_err(|_|"Cannot read OAuth callback")?;let request=std::str::from_utf8(&buffer[..count]).map_err(|_|"Invalid callback request")?;let path=request.lines().next().and_then(|line|line.strip_prefix("GET ")).and_then(|line|line.split_whitespace().next());let result=path.ok_or("Invalid callback method".to_string()).and_then(|path|callback(path,&state));let (status,body)=if result.is_ok(){("200 OK","Google sign-in received. Return to Calendio.")}else{("400 Bad Request","Invalid sign-in callback.")};let response=format!("HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",body.len());let _=stream.write_all(response.as_bytes()).await;if let Ok(code)=result{return Ok::<_,String>(code);}}Err("Too many invalid OAuth callbacks".into())}).await.map_err(|_|"Google sign-in timed out")??;
    let http = providers::client()?;
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| "Cannot exchange Google authorization code")?;
    let (status, body) = providers::json_response(response).await?;
    if status != 200 {
        return Err(format!("Google authorization exchange failed ({status})"));
    }
    let refresh_token = body["refresh_token"]
        .as_str()
        .ok_or("Google did not grant offline access; revoke the prior grant and reconnect")?
        .to_string();
    Credentials { client_id, client_secret, refresh_token }.store()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pkce_and_callback_state_are_enforced() {
        let url = authorization_url("client", "http://127.0.0.1:9000/oauth/callback", "state", "verifier").unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["code_challenge_method"], "S256");
        assert_ne!(query["code_challenge"], "verifier");
        assert_eq!(callback("/oauth/callback?state=state&code=code", "state").unwrap(), "code");
        assert!(callback("/oauth/callback?state=wrong&code=code", "state").is_err());
        assert!(callback("/oauth/callback?state=state&error=access_denied", "state").is_err());
    }
}
