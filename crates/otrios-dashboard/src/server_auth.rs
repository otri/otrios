// SPDX-License-Identifier: Apache-2.0
//! Monitoring-only browser sessions. Never serialize grants into shared state.
use super::*;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Instant};
const COOKIE: &str = "__Host-otrios_monitoring";
const MAX_SESSIONS: usize = 128;
pub struct Sessions {
    username: String,
    hash: String,
    ttl: Duration,
    sessions: Mutex<HashMap<String, Instant>>,
    attempts: Mutex<(Instant, u32)>,
    verifier: Arc<Semaphore>,
}
impl Sessions {
    /// Shared verifier for browser sign-in and explicitly authenticated native
    /// room connections. Trusted-listener admission alone never calls this.
    pub(super) async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<(String, String), StatusCode> {
        if username.len() > 320 || password.len() > 1024 || password.is_empty() {
            return Err(StatusCode::UNAUTHORIZED);
        }
        {
            let mut window = self.attempts.lock().await;
            if window.0.elapsed() >= Duration::from_secs(60) {
                *window = (Instant::now(), 0);
            }
            if window.1 >= 20 {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            window.1 += 1;
        }
        let permit = self
            .verifier
            .clone()
            .try_acquire_owned()
            .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
        let (hash, expected, user, password) = (
            self.hash.clone(),
            self.username.clone(),
            username.to_owned(),
            password.to_owned(),
        );
        let verified = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let parsed = PasswordHash::new(&hash).expect("validated hash");
            let matches = Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok();
            matches && user == expected
        })
        .await
        .unwrap_or(false);
        if !verified {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let key = digest(&token);
        let mut sessions = self.sessions.lock().await;
        let now = Instant::now();
        sessions.retain(|_, expires| *expires > now);
        if sessions.len() >= MAX_SESSIONS {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        sessions.insert(key.clone(), now + self.ttl);
        Ok((token, key))
    }
    pub(super) async fn revoke(&self, key: &str) {
        self.sessions.lock().await.remove(key);
    }
    pub fn load(
        path: &Path,
        ttl: Duration,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
                return Err("Auth file must be private (mode 600)".into());
            }
        }
        let raw = fs::read(path)?;
        if raw.len() > 4096 {
            return Err("Auth file too large".into());
        }
        let config: Value = serde_json::from_slice(&raw)?;
        let username = config["username"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 320)
            .ok_or("Invalid username")?
            .to_string();
        let hash = config["password_hash"]
            .as_str()
            .ok_or("Missing password hash")?
            .to_string();
        let parsed = PasswordHash::new(&hash).map_err(|_| "Invalid password hash")?;
        let params = argon2::Params::try_from(&parsed).map_err(|_| "Invalid Argon2 parameters")?;
        if parsed.algorithm.as_str() != "argon2id"
            || params.m_cost() > 262144
            || params.t_cost() > 10
            || params.p_cost() > 4
        {
            return Err("Unsupported password hash parameters".into());
        }
        Ok(Self {
            username,
            hash,
            ttl,
            sessions: Mutex::new(HashMap::new()),
            attempts: Mutex::new((Instant::now(), 0)),
            verifier: Arc::new(Semaphore::new(1)),
        })
    }
    pub(super) async fn valid(&self, key: &str) -> bool {
        let mut sessions = self.sessions.lock().await;
        let now = Instant::now();
        sessions.retain(|_, expires| *expires > now);
        sessions.contains_key(key)
    }
    pub(super) async fn key(&self, headers: &HeaderMap) -> Option<String> {
        let token = cookie(headers)?;
        let key = digest(&token);
        self.valid(&key).await.then_some(key)
    }
}
fn digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn cookie(headers: &HeaderMap) -> Option<String> {
    let mut token = None;
    for header in headers.get_all(header::COOKIE) {
        for part in header.to_str().ok()?.split(';') {
            if let Some((name, value)) = part.trim().split_once('=')
                && name == COOKIE
            {
                if token.is_some()
                    || value.len() != 64
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                {
                    return None;
                }
                token = Some(value.to_string());
            }
        }
    }
    token
}
fn response(status: StatusCode, value: Value) -> Response {
    let mut response = (
        status,
        binary("application/json", value.to_string().into_bytes(), false),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::VARY, header::HeaderValue::from_static("Cookie"));
    response
}
fn denied() -> Response {
    response(StatusCode::UNAUTHORIZED, json!({"monitoring":false}))
}
fn origin(app: &App, headers: &HeaderMap) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|v| app.origins.iter().any(|a| a == v))
}
fn mutation(app: &App, headers: &HeaderMap) -> bool {
    origin(app, headers)
        && headers
            .get("x-otrios-request")
            .is_some_and(|v| v == "session")
}
pub async fn session(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(auth) = &app.auth else {
        return denied();
    };
    if auth.key(&headers).await.is_some() {
        response(StatusCode::OK, json!({"monitoring":true}))
    } else {
        denied()
    }
}
pub async fn login(State(app): State<Arc<App>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if !mutation(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        != Some("application/json")
    {
        return failure(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected JSON credentials",
        );
    }
    let Some(auth) = &app.auth else {
        return denied();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return denied();
    };
    let (Some(username), Some(password)) = (value["username"].as_str(), value["password"].as_str())
    else {
        return denied();
    };
    let (token, _) = match auth.authenticate(username, password).await {
        Ok(value) => value,
        Err(StatusCode::TOO_MANY_REQUESTS) => {
            return response(
                StatusCode::TOO_MANY_REQUESTS,
                json!({"error":"Please wait before trying again"}),
            );
        }
        Err(StatusCode::UNAUTHORIZED) => return denied(),
        Err(_) => return failure(StatusCode::SERVICE_UNAVAILABLE, "Session unavailable"),
    };
    if let Some(old) = cookie(&headers) {
        auth.revoke(&digest(&old)).await;
    }
    let mut result = response(StatusCode::OK, json!({"monitoring":true}));
    result.headers_mut().insert(
        header::SET_COOKIE,
        format!(
            "{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={}",
            auth.ttl.as_secs()
        )
        .parse()
        .unwrap(),
    );
    result
}
pub async fn logout(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !mutation(&app, &headers) {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    if let Some(auth) = &app.auth
        && let Some(token) = cookie(&headers)
    {
        auth.sessions.lock().await.remove(&digest(&token));
    }
    let mut result = response(StatusCode::OK, json!({"monitoring":false}));
    result.headers_mut().insert(
        header::SET_COOKIE,
        format!("{COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")
            .parse()
            .unwrap(),
    );
    result
}
pub async fn presentation(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let Some(auth) = &app.auth else {
        return denied();
    };
    if auth.key(&headers).await.is_none() {
        return denied();
    }
    let mut result = get_presentation(State(app)).await;
    result
        .headers_mut()
        .insert(header::VARY, header::HeaderValue::from_static("Cookie"));
    result
}
