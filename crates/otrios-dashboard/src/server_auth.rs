// SPDX-License-Identifier: Apache-2.0
//! Authenticated human/native identities. Space membership is checked separately.
use super::*;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, time::Instant};
const COOKIE: &str = "__Host-otrios_monitoring";
const MAX_SESSIONS: usize = 128;
pub struct Sessions {
    users: HashMap<String, Login>,
    native: HashMap<String, String>,
    ttl: Duration,
    sessions: Mutex<HashMap<String, (String, Instant)>>,
    attempts: Mutex<(Instant, u32)>,
    verifier: Arc<Semaphore>,
}
struct Login {
    identity: String,
    hash: String,
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
        // Unknown usernames still incur the configured Argon2 verification cost.
        let login = self.users.get(username);
        let identity = login.map(|user| user.identity.clone());
        let hash = login
            .or_else(|| self.users.values().next())
            .ok_or(StatusCode::UNAUTHORIZED)?
            .hash
            .clone();
        let password = password.to_owned();
        let verified = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let parsed = PasswordHash::new(&hash).expect("validated hash");
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .await
        .unwrap_or(false);
        let identity = identity
            .filter(|_| verified)
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let mut random = [0; 32];
        getrandom::fill(&mut random).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let key = digest(&token);
        let mut sessions = self.sessions.lock().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires)| *expires > now);
        if sessions.len() >= MAX_SESSIONS {
            return Err(StatusCode::SERVICE_UNAVAILABLE);
        }
        sessions.insert(key.clone(), (identity, now + self.ttl));
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
        use std::io::Read;
        let mut raw = vec![];
        fs::File::open(path)?.take(65537).read_to_end(&mut raw)?;
        if raw.len() > 65536 {
            return Err("Auth file too large".into());
        }
        let config: Value = serde_json::from_slice(&raw)?;
        let entries = if let Some(users) = config["users"].as_array() {
            if config["version"].as_u64() != Some(1) || users.len() > 64 {
                return Err("Unsupported identity configuration".into());
            }
            users.clone()
        } else {
            vec![config.clone()]
        };
        let mut users = HashMap::new();
        for entry in entries {
            let username = entry["username"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 320)
                .ok_or("Invalid username")?;
            // Legacy credential files identify a person, but grant no Space role.
            let identity = entry["identity"].as_str().unwrap_or(username);
            if !otrios_core::spaces::valid_identity(identity) {
                return Err("Invalid identity".into());
            }
            let hash = entry["password_hash"]
                .as_str()
                .ok_or("Missing password hash")?;
            let parsed = PasswordHash::new(hash).map_err(|_| "Invalid password hash")?;
            let params =
                argon2::Params::try_from(&parsed).map_err(|_| "Invalid Argon2 parameters")?;
            if parsed.algorithm.as_str() != "argon2id"
                || params.m_cost() > 262144
                || params.t_cost() > 10
                || params.p_cost() > 4
            {
                return Err("Unsupported password hash parameters".into());
            }
            if users
                .insert(
                    username.to_owned(),
                    Login {
                        identity: identity.into(),
                        hash: hash.into(),
                    },
                )
                .is_some()
            {
                return Err("Duplicate login".into());
            }
        }
        let mut native = HashMap::new();
        if let Some(entries) = config["native"].as_array() {
            if config["version"].as_u64() != Some(1) || entries.len() > 64 {
                return Err("Unsupported native identities".into());
            }
            for entry in entries {
                let identity = entry["identity"]
                    .as_str()
                    .ok_or("Missing native identity")?;
                let hash = entry["token_sha256"]
                    .as_str()
                    .ok_or("Missing native token digest")?;
                if !otrios_core::spaces::valid_identity(identity)
                    || !valid_id(hash)
                    || native.insert(hash.into(), identity.into()).is_some()
                {
                    return Err("Invalid/duplicate native identity".into());
                }
            }
        }
        Ok(Self {
            users,
            native,
            ttl,
            sessions: Mutex::new(HashMap::new()),
            attempts: Mutex::new((Instant::now(), 0)),
            verifier: Arc::new(Semaphore::new(1)),
        })
    }
    pub(super) async fn principal(&self, key: &str) -> Option<String> {
        if let Some(identity) = self.native.get(key) {
            return Some(identity.clone());
        }
        let mut sessions = self.sessions.lock().await;
        let now = Instant::now();
        sessions.retain(|_, (_, expires)| *expires > now);
        sessions.get(key).map(|(identity, _)| identity.clone())
    }
    pub(super) async fn valid(&self, key: &str) -> bool {
        self.principal(key).await.is_some()
    }
    pub(super) async fn key(&self, headers: &HeaderMap) -> Option<String> {
        let bearer = headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .filter(|s| valid_id(s));
        let browser = cookie(headers);
        // Ambiguous credential sources are rejected instead of choosing a grant.
        let token = match (bearer, browser.as_deref()) {
            (Some(token), None) | (None, Some(token)) => token,
            _ => return None,
        };
        let key = digest(token);
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

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn config(value: Value) -> (PathBuf, Sessions) {
        let path = std::env::temp_dir().join(format!(
            "otrios-identity-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .unwrap()
            .write_all(value.to_string().as_bytes())
            .unwrap();
        let sessions = Sessions::load(&path, Duration::from_secs(60)).unwrap();
        (path, sessions)
    }
    #[tokio::test]
    async fn native_tokens_identify_a_principal_without_inventing_membership() {
        let token = "a".repeat(64);
        let (path, sessions) = config(json!({"version":1,"users":[],"native":[
            {"identity":"frame-household","token_sha256":digest(&token)}
        ]}));
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        );
        let key = sessions.key(&headers).await.unwrap();
        assert_eq!(
            sessions.principal(&key).await.as_deref(),
            Some("frame-household")
        );
        let mut invalid = HeaderMap::new();
        invalid.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", "b".repeat(64)).parse().unwrap(),
        );
        assert!(sessions.key(&invalid).await.is_none());
        headers.insert(header::COOKIE, format!("{COOKIE}={token}").parse().unwrap());
        assert!(sessions.key(&headers).await.is_none());
        fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn two_logins_have_distinct_expiring_and_revocable_identities() {
        use argon2::{PasswordHasher, password_hash::SaltString};
        let hash = Argon2::default()
            .hash_password(
                b"test-only-password",
                &SaltString::encode_b64(b"identity-test-only").unwrap(),
            )
            .unwrap()
            .to_string();
        let (path, sessions) = config(json!({"version":1,"users":[
            {"username":"alice-login","identity":"alice","password_hash":hash},
            {"username":"bob-login","identity":"bob","password_hash":hash}
        ]}));
        let (_, alice) = sessions
            .authenticate("alice-login", "test-only-password")
            .await
            .unwrap();
        let (_, bob) = sessions
            .authenticate("bob-login", "test-only-password")
            .await
            .unwrap();
        assert_eq!(sessions.principal(&alice).await.as_deref(), Some("alice"));
        assert_eq!(sessions.principal(&bob).await.as_deref(), Some("bob"));
        assert!(
            sessions
                .authenticate("unknown", "test-only-password")
                .await
                .is_err()
        );
        sessions.revoke(&alice).await;
        assert!(sessions.principal(&alice).await.is_none());
        assert!(sessions.principal(&bob).await.is_some());
        sessions.sessions.lock().await.get_mut(&bob).unwrap().1 = Instant::now();
        assert!(sessions.principal(&bob).await.is_none());
        fs::remove_file(path).unwrap();
    }
}
