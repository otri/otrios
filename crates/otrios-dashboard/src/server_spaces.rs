// SPDX-License-Identifier: Apache-2.0
//! HTTP authentication/serialization only; management and admission live in core.
use super::*;
use otrios_core::spaces as core;
use otrios_interfaces::spaces::{self as contract, Change, Role};

#[derive(Clone, Default)]
pub(super) struct Scope {
    pub space: Option<String>,
    pub identity: Option<String>,
}
pub(super) fn selected(uri: &axum::http::Uri) -> Result<Option<String>, StatusCode> {
    let mut selected = None;
    for field in uri.query().unwrap_or("").split('&') {
        if let Some(value) = field.strip_prefix("space=") {
            if selected.is_some() || !core::valid_id(value) {
                return Err(StatusCode::BAD_REQUEST);
            }
            selected = Some(value.into());
        }
    }
    Ok(selected)
}
pub(super) async fn identity(app: &App, key: &Option<String>) -> Option<String> {
    match (&app.auth, key) {
        (Some(auth), Some(key)) => auth.principal(key).await,
        _ => None,
    }
}
fn response(value: Value) -> Response {
    let mut response = binary("application/json", value.to_string().into_bytes(), false);
    response.headers_mut().insert(
        header::VARY,
        header::HeaderValue::from_static("Cookie, Authorization"),
    );
    response
}
fn error(error: core::Error) -> Response {
    let status = match error {
        core::Error::Denied => StatusCode::FORBIDDEN,
        core::Error::Invalid => StatusCode::BAD_REQUEST,
        core::Error::Conflict | core::Error::LastAdmin | core::Error::Incompatible => {
            StatusCode::CONFLICT
        }
        _ => StatusCode::SERVICE_UNAVAILABLE,
    };
    // Detailed persistence errors belong in operator logs, never public responses.
    failure(status, "Space request rejected")
}
pub async fn list(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let key = match &app.auth {
        Some(auth) => auth.key(&headers).await,
        None => None,
    };
    let identity = identity(&app, &key).await;
    let guard = app.apps.lock().await;
    let Some(hub) = guard.as_ref() else {
        return error(core::Error::Unavailable);
    };
    let registry = match hub.registry() {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let records = registry.spaces.iter().filter(|s| core::visible(s, identity.as_deref())).map(|s| {
        let role = core::role(s, identity.as_deref());
        let mut value = json!({"id":s.id,"name":s.name,"audience":s.audience,"role":role,
            "apps":s.apps.iter().filter(|a| a.attached || role == Some(Role::Admin)).collect::<Vec<_>>()});
        if role == Some(Role::Admin) { value["members"] = json!(s.members); }
        value
    }).collect::<Vec<_>>();
    response(
        json!({"version":contract::VERSION,"generation":registry.generation,"identity":identity,"spaces":records}),
    )
}
pub async fn manage(State(app): State<Arc<App>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if !headers
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|origin| app.origins.iter().any(|o| o == origin))
        || headers
            .get("x-otrios-request")
            .is_none_or(|v| v != "spaces")
    {
        return failure(StatusCode::FORBIDDEN, "Origin rejected");
    }
    let key = match &app.auth {
        Some(auth) => auth.key(&headers).await,
        None => None,
    };
    let Some(actor) = identity(&app, &key).await else {
        return error(core::Error::Denied);
    };
    let request: contract::Request = match serde_json::from_slice(&bytes) {
        Ok(request) => request,
        Err(_) => return error(core::Error::Invalid),
    };
    // Later milestone operations stay unavailable until their serving/cleanup
    // integrations are qualified. The contract already reserves their encodings.
    if matches!(
        &request.change,
        Change::DeleteData { .. } | Change::Remove { .. }
    ) || matches!(&request.change, Change::Audience { audience, .. } if *audience != contract::Audience::Closed)
    {
        return error(core::Error::Unavailable);
    }
    let mut guard = app.apps.lock().await;
    let Some(hub) = guard.as_mut() else {
        return error(core::Error::Unavailable);
    };
    let policy = if let Change::Attach { app, .. } = &request.change {
        match hub.app_policy(app) {
            Ok(policy) => Some(policy),
            Err(_) => return error(core::Error::Unavailable),
        }
    } else {
        None
    };
    match hub.manage_space(&actor, &request, policy.as_ref()) {
        Ok(generation) => {
            announce(&app).await;
            response(json!({"version":contract::VERSION,"generation":generation}))
        }
        Err(e) => error(e),
    }
}
