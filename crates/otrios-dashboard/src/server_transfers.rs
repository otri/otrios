// SPDX-License-Identifier: Apache-2.0
//! Authenticated payload transport; management remains on the shared session.
use super::*;
use axum::body::Body;
use otrios_interfaces::transfers::{DATA_MEDIA_TYPE, DatasetRef, MAX_DATA_ARCHIVE_BYTES};
#[path = "server_app_fetch.rs"]
mod fetch;

/// Native host context, from this authority's explicit Origin configuration.
/// No Space records, credentials or new trust roots are returned.
pub(super) async fn context(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if headers.contains_key(header::ORIGIN)
        || headers
            .get("x-otrios-request")
            .is_none_or(|v| v != "transfers")
    {
        return failure(StatusCode::FORBIDDEN, "Native transfer context required");
    }
    let key = match &app.auth {
        Some(auth) => auth.key(&headers).await,
        None => None,
    };
    if spaces::identity(&app, &key).await.is_none() {
        return failure(StatusCode::FORBIDDEN, "Native admission required");
    }
    let origin = app.origins.iter().find(|origin| {
        otrios_core::transfers::AppSource::parse(&format!(
            "{origin}/apps/sample/{}",
            "0".repeat(64)
        ))
        .is_ok()
    });
    binary(
        "application/json",
        json!({"origin":origin}).to_string().into_bytes(),
        false,
    )
}

/// Intake only: complete verification produces an expiring review, never an
/// install, attachment, trust-root change or executable publication.
pub(super) async fn upload_app(
    State(app): State<Arc<App>>,
    RoutePath(space): RoutePath<String>,
    request: axum::extract::Request,
) -> Response {
    use otrios_core::transfers::{APP_MEDIA_TYPE, AppCandidate, AppSource, MAX_APP_ARCHIVE_BYTES};
    let headers = request.headers();
    let is_url = headers
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v == "text/plain; charset=utf-8");
    if !origin_ok(&app, headers)
        || headers
            .get("x-otrios-request")
            .is_none_or(|v| v != "transfers")
        || !is_url
            && headers
                .get(header::CONTENT_TYPE)
                .is_none_or(|v| v != APP_MEDIA_TYPE)
    {
        return failure(StatusCode::FORBIDDEN, "App intake request rejected");
    }
    let limit = if is_url { 512 } else { MAX_APP_ARCHIVE_BYTES };
    if headers.get(header::CONTENT_LENGTH).is_some_and(|v| {
        v.to_str()
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .is_none_or(|n| n > limit)
    }) {
        return failure(StatusCode::PAYLOAD_TOO_LARGE, "App intake byte budget");
    }
    let key = match &app.auth {
        Some(auth) => auth.key(headers).await,
        None => None,
    };
    let Some(actor) = spaces::identity(&app, &key).await else {
        return failure(StatusCode::FORBIDDEN, "Space Admin admission required");
    };
    let Some(releases) = app.releases.as_ref() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Signed app installation unavailable",
        );
    };
    let target = {
        let guard = app.apps.lock().await;
        match guard.as_ref().ok_or("Authority unavailable").and_then(|h| {
            h.app_import_target(&actor, &space)
                .map_err(|_| "Admission changed")
        }) {
            Ok(target) => target,
            Err(_) => return failure(StatusCode::FORBIDDEN, "Space Admin admission required"),
        }
    };
    let Ok(credit) = app.transfer_jobs.clone().try_acquire_owned() else {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Finish or cancel the current transfer",
        );
    };
    let publisher = match bundle_files::public_key(&app.state_config.bundle_key, false, "openssl") {
        Ok(key) => key,
        Err(_) => {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Publisher policy unavailable",
            );
        }
    };
    let read = async {
        let mut stream = request.into_body().into_data_stream();
        let mut bytes = vec![];
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "App input failed")?;
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > limit)
            {
                return Err("App input byte budget");
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok::<_, &str>(bytes)
    };
    let bytes = match tokio::time::timeout(Duration::from_secs(30), read).await {
        Ok(Ok(bytes)) => bytes,
        _ => {
            return failure(
                StatusCode::BAD_REQUEST,
                "App input incomplete or exceeds its budget",
            );
        }
    };
    let source = if is_url {
        match std::str::from_utf8(&bytes)
            .map_err(|_| "Invalid URL".to_string())
            .and_then(AppSource::parse)
        {
            Ok(source) => Some(source),
            Err(_) => {
                return failure(
                    StatusCode::BAD_REQUEST,
                    "Use an immutable public HTTPS app URL",
                );
            }
        }
    } else {
        None
    };
    // A detached intake task retains the one work credit even if its HTTP client
    // disconnects while DNS, TLS or the blocking archive verifier is finishing.
    let job = tokio::spawn(async move {
        let bytes = match &source {
            Some(source) => fetch::fetch(source).await?,
            None => bytes,
        };
        tokio::task::spawn_blocking(move || {
            (
                AppCandidate::qualify(&bytes, &publisher, source.as_ref()),
                credit,
            )
        })
        .await
        .map_err(|e| e.to_string())
    });
    let (candidate, credit) = match job.await {
        Ok(Ok((Ok(candidate), credit))) => (candidate, credit),
        _ => {
            return failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                "App source, archive, publisher or host compatibility rejected",
            );
        }
    };
    let mut guard = app.apps.lock().await;
    if spaces::identity(&app, &key).await.as_deref() != Some(actor.as_str()) {
        return failure(StatusCode::FORBIDDEN, "App intake admission changed");
    }
    let (Some(hub), Some(auth)) = (guard.as_mut(), app.auth.as_ref()) else {
        return failure(StatusCode::SERVICE_UNAVAILABLE, "App authority unavailable");
    };
    match hub.admit_app_stage(
        &actor,
        auth.host_policy(),
        target,
        candidate,
        credit,
        releases,
    ) {
        Ok(review) => binary(
            "application/json",
            json!(review).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::CONFLICT,
            "App admission, installed version or destination changed",
        ),
    }
}

pub(super) async fn publications<T: Send + 'static>(
    app: &Arc<App>,
    work: impl FnOnce(&otrios_core::transfers::PublicationStore) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let slot = app.publications.clone();
    let storage = app
        .state_config
        .state
        .clone()
        .ok_or("Public code storage unavailable")?;
    let key = app.state_config.bundle_key.clone();
    tokio::task::spawn_blocking(move || {
        let mut slot = slot
            .lock()
            .map_err(|_| "Public code storage lock unavailable")?;
        if slot.is_none() {
            if !fs::symlink_metadata(&storage)
                .map_err(|e| e.to_string())?
                .is_dir()
            {
                return Err("Public code storage unavailable".into());
            }
            let key = bundle_files::public_key(&key, false, "openssl")?;
            *slot = Some(otrios_core::transfers::PublicationStore::open(
                storage.join("published-apps"),
                key,
            )?);
        }
        work(slot.as_ref().ok_or("Public code storage unavailable")?)
    })
    .await
    .map_err(|e| e.to_string())?
}
async fn public_object(
    app: &Arc<App>,
    app_id: String,
    digest: String,
) -> Result<
    (
        otrios_core::transfers::PublishedApp,
        tokio::sync::OwnedSemaphorePermit,
    ),
    StatusCode,
> {
    let permit = app
        .public_transfer_jobs
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;
    let (object, permit) = publications(app, move |store| {
        Ok((store.read(&app_id, &digest)?, permit))
    })
    .await
    .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok((object.ok_or(StatusCode::NOT_FOUND)?, permit))
}
fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub(super) async fn app_page(
    State(app): State<Arc<App>>,
    RoutePath((app_id, digest)): RoutePath<(String, String)>,
) -> Response {
    let (published, _credit) = match public_object(&app, app_id, digest).await {
        Ok(object) => object,
        Err(status) => return failure(status, "App share unavailable"),
    };
    let info = &published.summary;
    let title = html(&info.info.app.title);
    let id = html(&info.info.app.id);
    let path = format!("/apps/{}/{}", info.info.app.id, info.digest);
    let body = format!(
        r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{title} · OTriOS</title><style>html{{font-family:system-ui,sans-serif;color:#263630;background:#f4f5ef}}body{{margin:0}}main{{max-width:34rem;margin:8vh auto;padding:2rem}}p{{line-height:1.6}}.id{{overflow-wrap:anywhere;color:#52665e}}.download{{display:inline-block;background:#294f43;color:white;border-radius:.8rem;padding:1rem 1.5rem;text-decoration:none;font-weight:600}}footer{{margin-top:3rem;border-top:1px solid #cbd4cb;padding-top:1.2rem}}a{{color:#294f43}}</style><main><p>OTriOS</p><h1>{title}</h1><p class="id">{id}</p><p>This download contains app code only. Space data, account credentials and access grants are excluded.</p><p><a class="download" href="{path}/download" download="{id}.otapp.zip">Download App</a></p><p>In OTriOS, open Edit, choose Add App and Upload, then select the downloaded file or paste this page’s URL. Installation requires the receiving host’s publisher trust and authorization.</p><footer><strong>Get OTriOS</strong><p><a href="https://github.com/otri/otrios">Project source</a> · <a href="https://github.com/otri/otrios/blob/main/docs/FRAME_HOSTING.md">Hosting guide</a></p></footer></main></html>"#
    );
    let mut response = binary("text/html; charset=utf-8", body.into_bytes(), false);
    response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, header::HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"));
    response
}
pub(super) async fn app_download(
    State(app): State<Arc<App>>,
    RoutePath((app_id, digest)): RoutePath<(String, String)>,
) -> Response {
    let (published, credit) = match public_object(&app, app_id, digest).await {
        Ok(object) => object,
        Err(status) => return failure(status, "App share unavailable"),
    };
    let filename = format!(
        "attachment; filename=\"{}.otapp.zip\"",
        published.summary.info.app.id
    );
    let bytes = Bytes::from(published.archive);
    let length = bytes.len();
    let stream = futures_util::stream::unfold((bytes, credit), |(mut bytes, credit)| async move {
        if bytes.is_empty() {
            return None;
        }
        let chunk = bytes.split_to(bytes.len().min(64 * 1024));
        Some((Ok::<_, std::convert::Infallible>(chunk), (bytes, credit)))
    });
    let mut response = binary(otrios_interfaces::transfers::APP_MEDIA_TYPE, vec![], false);
    *response.body_mut() = Body::from_stream(stream);
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, header::HeaderValue::from(length));
    if let Ok(filename) = header::HeaderValue::from_str(&filename) {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, filename);
    }
    response
}

pub(super) async fn upload_data(
    State(app): State<Arc<App>>,
    RoutePath((space, app_id, collection)): RoutePath<(String, String, String)>,
    request: axum::extract::Request,
) -> Response {
    let headers = request.headers();
    if !origin_ok(&app, headers)
        || headers
            .get("x-otrios-request")
            .is_none_or(|v| v != "transfers")
        || headers
            .get(header::CONTENT_TYPE)
            .is_none_or(|v| v != DATA_MEDIA_TYPE)
    {
        return failure(StatusCode::FORBIDDEN, "Transfer request rejected");
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        match length.to_str().ok().and_then(|v| v.parse::<usize>().ok()) {
            Some(n) if n <= MAX_DATA_ARCHIVE_BYTES => (),
            _ => {
                return failure(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Data upload exceeds its budget",
                );
            }
        }
    }
    let key = match &app.auth {
        Some(auth) => auth.key(headers).await,
        None => None,
    };
    let Some(actor) = spaces::identity(&app, &key).await else {
        return failure(StatusCode::FORBIDDEN, "Dataset Admin admission required");
    };
    let Ok(credit) = app.transfer_jobs.clone().try_acquire_owned() else {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "Finish or cancel the current transfer",
        );
    };
    let target = {
        let guard = app.apps.lock().await;
        let Some(hub) = guard.as_ref() else {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Data authority unavailable",
            );
        };
        match hub.replacement_target(&actor, &space, &app_id, &collection) {
            Ok(target) => target,
            Err(_) => {
                return failure(
                    StatusCode::FORBIDDEN,
                    "Dataset admission or codec unavailable",
                );
            }
        }
    };
    // Authenticate and reserve work before consuming a hostile request body.
    // The absolute deadline also bounds a sender that dribbles valid chunks.
    let read = async {
        let mut stream = request.into_body().into_data_stream();
        let mut bytes = vec![];
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
            if bytes
                .len()
                .checked_add(chunk.len())
                .is_none_or(|n| n > MAX_DATA_ARCHIVE_BYTES)
            {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok::<_, StatusCode>(bytes)
    };
    let bytes = match tokio::time::timeout(Duration::from_secs(30), read).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(status)) => return failure(status, "Data upload rejected"),
        Err(_) => return failure(StatusCode::REQUEST_TIMEOUT, "Data upload timed out"),
    };
    let (candidate, credit) = match tokio::task::spawn_blocking(move || {
        (
            otrios_core::transfers::DataCandidate::qualify(target, &bytes),
            credit,
        )
    })
    .await
    {
        Ok((Ok(candidate), credit)) => (candidate, credit),
        _ => {
            return failure(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Data payload is incomplete or incompatible",
            );
        }
    };
    let mut guard = app.apps.lock().await;
    if spaces::identity(&app, &key).await.as_deref() != Some(actor.as_str()) {
        return failure(StatusCode::FORBIDDEN, "Dataset admission changed");
    }
    let Some(hub) = guard.as_mut() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Data authority unavailable",
        );
    };
    match hub.admit_data_stage(&actor, candidate, credit) {
        Ok(review) => binary(
            "application/json",
            json!(review).to_string().into_bytes(),
            false,
        ),
        Err(_) => failure(
            StatusCode::CONFLICT,
            "Dataset changed during upload; review current state",
        ),
    }
}

pub(super) async fn download_data(
    State(app): State<Arc<App>>,
    RoutePath((space, app_id, collection)): RoutePath<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let key = match &app.auth {
        Some(auth) => auth.key(&headers).await,
        None => None,
    };
    let Some(actor) = spaces::identity(&app, &key).await else {
        return failure(StatusCode::FORBIDDEN, "Dataset Admin admission required");
    };
    // A slow reader holds this credit until its body finishes or disconnects.
    // This bounds both expensive serialization and outstanding private payloads.
    let Ok(permit) = app.transfer_jobs.clone().try_acquire_owned() else {
        return failure(StatusCode::TOO_MANY_REQUESTS, "Transfer busy; retry later");
    };
    let (selected, pin) = {
        let guard = app.apps.lock().await;
        let Some(hub) = guard.as_ref() else {
            return failure(
                StatusCode::SERVICE_UNAVAILABLE,
                "Data authority unavailable",
            );
        };
        let Ok(selected) = hub.export_selection(&actor, &space, &app_id, &collection) else {
            return failure(StatusCode::FORBIDDEN, "Dataset admission changed");
        };
        let pin = match hub.pin_export(&actor, &selected) {
            Ok(pin) => pin,
            Err(error) => {
                eprintln!("Portable export unavailable: {error}");
                return failure(
                    StatusCode::CONFLICT,
                    "Portable export unavailable for this dataset",
                );
            }
        };
        (selected, pin)
    };
    // A blocking serializer cannot hold admission or delay control hello/upgrades.
    // Keep the credit inside the blocking task as well: cancelling an HTTP
    // request cannot release it while detached serialization still owns memory.
    let (bytes, permit) = match tokio::task::spawn_blocking(move || (pin.encode(), permit)).await {
        Ok((Ok(bytes), permit)) => (Bytes::from(bytes), permit),
        other => {
            eprintln!(
                "Portable export serialization failed: {}",
                match other {
                    Ok((Err(e), _)) => e,
                    Err(e) => e.to_string(),
                    _ => unreachable!(),
                }
            );
            return failure(
                StatusCode::CONFLICT,
                "Portable export unavailable for this dataset",
            );
        }
    };
    if !admitted(&app, &key, &actor, &selected).await {
        return failure(StatusCode::FORBIDDEN, "Dataset admission changed");
    }
    let length = bytes.len();
    let stream = futures_util::stream::unfold(
        (app, key, actor, selected, bytes, permit),
        |(app, key, actor, selected, mut bytes, permit)| async move {
            if bytes.is_empty() {
                return None;
            }
            if !admitted(&app, &key, &actor, &selected).await {
                return Some((
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Dataset admission changed",
                    )),
                    (app, key, actor, selected, Bytes::new(), permit),
                ));
            }
            let chunk = bytes.split_to(bytes.len().min(64 * 1024));
            Some((Ok(chunk), (app, key, actor, selected, bytes, permit)))
        },
    );
    let mut response = binary(DATA_MEDIA_TYPE, vec![], false);
    *response.body_mut() = Body::from_stream(stream);
    response
        .headers_mut()
        .insert(header::CONTENT_LENGTH, header::HeaderValue::from(length));
    response.headers_mut().insert(
        header::VARY,
        header::HeaderValue::from_static("Cookie, Authorization"),
    );
    if let Ok(value) =
        header::HeaderValue::from_str(&format!("attachment; filename=\"{app_id}.otdata\""))
    {
        response
            .headers_mut()
            .insert(header::CONTENT_DISPOSITION, value);
    }
    response
}
async fn admitted(app: &App, key: &Option<String>, actor: &str, selected: &DatasetRef) -> bool {
    let guard = app.apps.lock().await;
    spaces::identity(app, key).await.as_deref() == Some(actor)
        && guard
            .as_ref()
            .is_some_and(|hub| hub.check_export(actor, selected).is_ok())
}
