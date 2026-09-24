use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use keyring::Entry;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

use crate::config::{google_client_id, google_client_secret, oauth_redirect};

const KEYRING_SERVICE: &str = "com.Storbyte.desktop";
const SCOPES: &str = "https://www.googleapis.com/auth/drive https://www.googleapis.com/auth/userinfo.email openid";
/// How long we wait for the browser callback before giving up.
const OAUTH_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
    id: Option<String>,
}

pub fn store_tokens(account_id: &str, tokens: &TokenSet) -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, account_id)?;
    entry.set_password(&serde_json::to_string(tokens)?)?;
    Ok(())
}

pub fn load_tokens(account_id: &str) -> Result<Option<TokenSet>> {
    let entry = Entry::new(KEYRING_SERVICE, account_id)?;
    match entry.get_password() {
        Ok(raw) => Ok(Some(serde_json::from_str(&raw)?)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete_tokens(account_id: &str) -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, account_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn pkce_pair() -> (String, String) {
    let mut verifier_bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut verifier_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn random_state() -> String {
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

/// Run Google OAuth PKCE with a loopback HTTP listener.
/// `cancel` can be set from the UI if the user aborts (or closes the browser and taps Cancel).
pub async fn connect_google_oauth(cancel: Arc<AtomicBool>) -> Result<(String, TokenSet)> {
    cancel.store(false, Ordering::SeqCst);

    let client_id = google_client_id();
    let client_secret = google_client_secret();
    if client_id.is_empty() || client_secret.is_empty() {
        return Err(anyhow!(
            "Google sign-in is not configured in this build. Reinstall Storbyte or contact support."
        ));
    }

    let preferred = oauth_redirect();
    let preferred_url = Url::parse(&preferred)?;
    let preferred_port = preferred_url.port().unwrap_or(17832);
    let callback_path = preferred_url.path().to_string();

    // Bind before opening the browser. If the preferred port is taken (stale
    // Storbyte / prior OAuth), fall back to an ephemeral port — Desktop clients
    // allow any 127.0.0.1 port as long as auth + token exchange use the same URI.
    let listener = match TcpListener::bind(("127.0.0.1", preferred_port)) {
        Ok(l) => l,
        Err(_) => TcpListener::bind(("127.0.0.1", 0)).with_context(|| {
            format!(
                "could not bind OAuth callback (preferred 127.0.0.1:{preferred_port} busy, and no free port available). Close other Storbyte windows and try again."
            )
        })?,
    };
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{port}{callback_path}");

    let (verifier, challenge) = pkce_pair();
    let state = random_state();

    let mut auth = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    {
        let mut q = auth.query_pairs_mut();
        q.append_pair("client_id", &client_id);
        q.append_pair("redirect_uri", &redirect);
        q.append_pair("response_type", "code");
        q.append_pair("scope", SCOPES);
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
        q.append_pair("code_challenge", &challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", &state);
    }

    let (tx, rx) = mpsc::channel::<Result<(String, String)>>();
    let expected_state = state.clone();
    let cancel_listener = cancel.clone();
    thread::spawn(move || {
        if let Err(e) = listen_for_code(listener, &expected_state, tx.clone(), cancel_listener) {
            let _ = tx.send(Err(e));
        }
    });

    // Small delay so the loopback listener is accepting before the browser opens.
    thread::sleep(Duration::from_millis(150));
    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow!("Sign-in cancelled"));
    }
    open::that(auth.as_str()).context("failed to open browser for Google OAuth")?;

    let cancel_wait = cancel.clone();
    let (code, _returned_state) = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + OAUTH_TIMEOUT;
        loop {
            if cancel_wait.load(Ordering::SeqCst) {
                return Err(anyhow!("Sign-in cancelled"));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(anyhow!(
                    "Sign-in timed out. Close the browser tab and try Import again."
                ));
            }
            match rx.recv_timeout(Duration::from_millis(200).min(remaining)) {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("Sign-in cancelled"));
                }
            }
        }
    })
    .await??;

    if cancel.load(Ordering::SeqCst) {
        return Err(anyhow!("Sign-in cancelled"));
    }

    let tokens = exchange_code(&client_id, &client_secret, &redirect, &code, &verifier).await?;
    let email = fetch_email(&tokens.access_token).await?;
    let account_id = email
        .id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let set = TokenSet {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_at: tokens.expires_in.map(|s| chrono::Utc::now().timestamp() + s),
        email: email.email,
    };
    store_tokens(&account_id, &set)?;
    Ok((account_id, set))
}

fn listen_for_code(
    listener: TcpListener,
    expected_state: &str,
    tx: mpsc::Sender<Result<(String, String)>>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let deadline = Instant::now() + OAUTH_TIMEOUT;

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(anyhow!("Sign-in cancelled"));
        }
        if Instant::now() > deadline {
            return Err(anyhow!(
                "Sign-in timed out. Close the browser tab and try Import again."
            ));
        }
        listener.set_nonblocking(true)?;
        let accepted = listener.accept();
        listener.set_nonblocking(false)?;
        let (mut stream, _) = match accepted {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        };

        // Read until end of HTTP headers (or buffer full).
        let mut raw = Vec::new();
        let mut buf = [0u8; 1024];
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") || raw.len() > 16 * 1024 {
                        break;
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => break,
            }
        }
        let req = String::from_utf8_lossy(&raw);
        let first = req.lines().next().unwrap_or("");
        let path = first.split_whitespace().nth(1).unwrap_or("/");

        // Ignore favicon / noise — keep waiting for the real callback.
        if !path.starts_with("/oauth/callback") {
            let body = "ok";
            let resp = format!(
                "HTTP/1.1 204 No Content\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }

        let url = match Url::parse(&format!("http://127.0.0.1{path}")) {
            Ok(u) => u,
            Err(_) => continue,
        };
        let mut code = None;
        let mut state = None;
        let mut error = None;
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "code" => code = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                "error" => error = Some(v.into_owned()),
                _ => {}
            }
        }

        if let Some(err) = error {
            let body = "<html><body><h1>Authorization failed</h1><p>You can close this window.</p></body></html>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let msg = if err == "access_denied" {
                "Sign-in cancelled".to_string()
            } else {
                format!("Google OAuth error: {err}")
            };
            let _ = tx.send(Err(anyhow!(msg)));
            return Ok(());
        }

        let Some(code) = code else {
            // Callback without code — keep listening.
            continue;
        };
        let state = state.unwrap_or_default();
        if state != expected_state {
            // Old browser tab from a previous attempt — ignore and keep waiting.
            tracing::warn!("ignoring OAuth callback with mismatched state (stale tab?)");
            let body = "<html><body><h1>Stale login tab</h1><p>Close this tab and use the newest Google sign-in window.</p></body></html>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            continue;
        }

        let body = "<html><body><h1>Storbyte connected</h1><p>You can close this window and return to the app.</p></body></html>";
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = tx.send(Ok((code, state)));
        return Ok(());
    }
}

async fn exchange_code(
    client_id: &str,
    client_secret: &str,
    redirect: &str,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    let _ = resp.token_type;
    Ok(resp)
}

async fn fetch_email(access_token: &str) -> Result<UserInfo> {
    let client = reqwest::Client::new();
    let info = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await?
        .error_for_status()?
        .json::<UserInfo>()
        .await?;
    Ok(info)
}

pub async fn ensure_access_token(account_id: &str) -> Result<String> {
    let mut tokens = load_tokens(account_id)?.ok_or_else(|| anyhow!("no tokens for account"))?;
    let now = chrono::Utc::now().timestamp();
    if let Some(exp) = tokens.expires_at {
        if exp > now + 60 {
            return Ok(tokens.access_token);
        }
    }
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or_else(|| anyhow!("missing refresh token; reconnect Google account"))?;
    let client_id = google_client_id();
    let client_secret = google_client_secret();
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    tokens.access_token = resp.access_token;
    if let Some(r) = resp.refresh_token {
        tokens.refresh_token = Some(r);
    }
    tokens.expires_at = resp
        .expires_in
        .map(|s| chrono::Utc::now().timestamp() + s);
    store_tokens(account_id, &tokens)?;
    Ok(tokens.access_token)
}
