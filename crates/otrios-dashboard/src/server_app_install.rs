// SPDX-License-Identifier: Apache-2.0
//! Local operator bridge transport. Only registered on the trusted loopback
//! listener; the Mac reaches it through the existing pinned SSH configuration.
use super::*;
fn bridge(headers: &HeaderMap) -> bool {
    !headers.contains_key(header::ORIGIN)
        && headers
            .get("x-otrios-request")
            .is_some_and(|h| h == "signer")
}
pub(super) async fn pending(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if !bridge(&headers) {
        return failure(StatusCode::FORBIDDEN, "Operator bridge required");
    }
    let guard = app.apps.lock().await;
    match guard
        .as_ref()
        .ok_or(otrios_core::spaces::Error::Unavailable)
        .and_then(|h| h.pending_install())
    {
        Ok(job) => binary(
            "application/json",
            json!(job).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Install authority unavailable",
        ),
    }
}
pub(super) async fn status(
    State(app): State<Arc<App>>,
    RoutePath(operation): RoutePath<String>,
    headers: HeaderMap,
) -> Response {
    if !bridge(&headers) {
        return failure(StatusCode::FORBIDDEN, "Operator bridge required");
    }
    let guard = app.apps.lock().await;
    match guard
        .as_ref()
        .ok_or(otrios_core::spaces::Error::Unavailable)
        .and_then(|h| h.operator_import_state(&operation))
    {
        Ok(state) => binary(
            "application/json",
            json!(state).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Install authority unavailable",
        ),
    }
}
pub(super) async fn reject(
    State(app): State<Arc<App>>,
    RoutePath(operation): RoutePath<String>,
    headers: HeaderMap,
) -> Response {
    if !bridge(&headers) {
        return failure(StatusCode::FORBIDDEN, "Operator bridge required");
    }
    let mut guard = app.apps.lock().await;
    let result = guard
        .as_mut()
        .ok_or(otrios_core::spaces::Error::Unavailable)
        .and_then(|h| h.reject_install(&operation));
    drop(guard);
    announce(&app).await;
    match result {
        Ok(state) => binary(
            "application/json",
            json!(state).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::CONFLICT,
            "Only a queued install can be rejected",
        ),
    }
}
pub(super) async fn payload(
    State(app): State<Arc<App>>,
    RoutePath(operation): RoutePath<String>,
    headers: HeaderMap,
) -> Response {
    if !bridge(&headers) {
        return failure(StatusCode::FORBIDDEN, "Operator bridge required");
    }
    let Ok(credit) = app.transfer_jobs.clone().try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Transfer busy");
    };
    let guard = app.apps.lock().await;
    let bytes = match guard
        .as_ref()
        .ok_or(otrios_core::spaces::Error::Unavailable)
        .and_then(|h| h.install_archive(&operation))
    {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => return failure(StatusCode::NOT_FOUND, "Install payload unavailable"),
    };
    let length = bytes.len();
    let stream = futures_util::stream::unfold((bytes, credit), |(mut bytes, credit)| async move {
        if bytes.is_empty() {
            return None;
        }
        let chunk = bytes.split_to(bytes.len().min(64 * 1024));
        Some((Ok::<_, std::convert::Infallible>(chunk), (bytes, credit)))
    });
    let mut response = binary(otrios_interfaces::transfers::APP_MEDIA_TYPE, vec![], false);
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, header::HeaderValue::from(length));
    *response.body_mut() = axum::body::Body::from_stream(stream);
    response
}
pub(super) async fn arm(
    State(app): State<Arc<App>>,
    RoutePath(operation): RoutePath<String>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if !bridge(&headers)
        || headers
            .get(header::CONTENT_TYPE)
            .is_none_or(|h| h != "application/json")
        || bytes.len() > 256
    {
        return failure(StatusCode::FORBIDDEN, "Operator bridge required");
    }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return failure(StatusCode::BAD_REQUEST, "Invalid release decision");
    };
    let Some(release) = value
        .as_object()
        .filter(|m| m.len() == 1)
        .and_then(|m| m.get("release"))
        .and_then(Value::as_str)
    else {
        return failure(StatusCode::BAD_REQUEST, "Invalid release decision");
    };
    let mut guard = app.apps.lock().await;
    let (Some(hub), Some(auth)) = (guard.as_mut(), app.auth.as_ref()) else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Install authority unavailable",
        );
    };
    let result = hub.arm_install(auth.host_policy(), &operation, release);
    drop(guard);
    announce(&app).await;
    match result {
        Ok(state) => binary(
            "application/json",
            json!(state).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::CONFLICT,
            "Install review changed or activation already armed; read its current outcome",
        ),
    }
}
