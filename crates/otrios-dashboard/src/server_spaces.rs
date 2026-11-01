// SPDX-License-Identifier: Apache-2.0
//! HTTP authentication/serialization only; management and admission live in core.
use super::*;
use otrios_core::spaces as core;
use otrios_interfaces::spaces::{self as contract, Change, Role};
#[path = "server_spaces_session.rs"]
mod session;
pub(super) use self::session::{Packet, Peer, subscribe};

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
        let mut value = json!({"id":s.id,"name":s.name,"archived":s.archived,"layout":s.layout,"audience":s.audience,"role":role,
            "apps":s.apps.iter().filter(|a| a.attached || role == Some(Role::Admin)).collect::<Vec<_>>()});
        if role == Some(Role::Admin) {
            value["members"] = json!(s.members); value["cleanup"] = json!(hub.cleanup(&s.id));
            value["retained"] = json!(registry.retained.iter().filter(|d| d.space == s.id).collect::<Vec<_>>());
        }
        value
    }).collect::<Vec<_>>();
    response(
        json!({"version":contract::VERSION,"generation":registry.generation,"identity":identity,"spaces":records}),
    )
}
pub async fn manage(State(app): State<Arc<App>>, headers: HeaderMap, bytes: Bytes) -> Response {
    if bytes.len() > contract::MAX_COMMAND_BYTES {
        return error(core::Error::Capacity);
    }
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
    let mut guard = app.apps.lock().await;
    if identity(&app, &key).await.as_deref() != Some(actor.as_str()) {
        return error(core::Error::Denied);
    }
    let Some(hub) = guard.as_mut() else {
        return error(core::Error::Unavailable);
    };
    match manage_request(hub, &actor, &request) {
        Ok(generation) => {
            announce(&app).await;
            response(json!({"version":contract::VERSION,"generation":generation}))
        }
        Err(e) => error(e),
    }
}

pub(super) fn manage_request(
    hub: &mut apps::Hub,
    actor: &str,
    request: &contract::Request,
) -> std::result::Result<u64, core::Error> {
    let policy =
        if let Change::Attach { app, .. } | Change::RestoreDataset { app, .. } = &request.change {
            match hub.app_policy(app) {
                Ok(policy) => Some(policy),
                Err(_) => return Err(core::Error::Unavailable),
            }
        } else {
            None
        };
    hub.manage_space(actor, request, policy.as_ref())
}

pub(super) fn private_metadata(path: &Path, limit: usize) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("Invalid private configuration".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("Private configuration must be mode 0600".into());
        }
    }
    Ok(())
}
pub(super) fn private_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    private_metadata(path, limit)?;
    let mut bytes = vec![];
    fs::File::open(path)
        .and_then(|f| f.take(limit as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Private configuration limit".into());
    }
    Ok(bytes)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("Missing provisioning {field}"))
}
fn entries<'a>(value: &'a Value, field: &str, max: usize) -> Result<&'a [Value], String> {
    let values = value[field]
        .as_array()
        .ok_or_else(|| format!("Missing provisioning {field}"))?;
    if values.len() > max {
        return Err("Provisioning count limit".into());
    }
    Ok(values)
}
fn apply(hub: &mut apps::Hub, actor: &str, change: Change) -> Result<(), String> {
    let policy = if let Change::Attach { app, .. } | Change::RestoreDataset { app, .. } = &change {
        Some(hub.app_policy(app)?)
    } else {
        None
    };
    hub.manage_space(
        actor,
        &contract::Request {
            version: contract::VERSION,
            expected_generation: hub.registry().map_err(|e| e.to_string())?.generation,
            change,
        },
        policy.as_ref(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// Explicit offline operator command. No listeners are bound while it runs.
/// A durable pending marker prevents partial provisioning from serving empty data.
pub(super) async fn provision(app: &App, path: &Path) -> Result<(), String> {
    let raw = private_bytes(path, 65536)?;
    let config: Value =
        serde_json::from_slice(&raw).map_err(|_| "Invalid provisioning manifest")?;
    let version = config["version"]
        .as_u64()
        .ok_or("Invalid provisioning version")?;
    if !matches!(version, 1 | 2) {
        return Err("Unsupported provisioning version".into());
    }
    let chat_import = config["spaces"].as_array().is_some_and(|spaces| {
        spaces.iter().any(|space| {
            space["apps"]
                .as_array()
                .is_some_and(|apps| apps.iter().any(|app| app.get("legacy_chat_root").is_some()))
        })
    });
    if version == 1 && chat_import {
        // Older v1 provisioners ignore unknown import fields. A version they
        // reject must guard a plan that would otherwise create an empty room.
        return Err("Legacy chat import requires provisioning manifest version 2".into());
    }
    let digest = otrios_core::canvas::hex(&otrios_core::canvas::hash(&raw));
    let mut guard = app.apps.lock().await;
    let hub = guard.as_mut().ok_or("Authority recovery unavailable")?;
    let registry = hub.registry_path()?.to_path_buf();
    let pending = registry.with_extension("provisioning");
    let done = registry.with_extension(format!("provisioned-{digest}.json"));
    let parent = registry.parent().ok_or("Invalid registry path")?;
    if pending.exists() && private_bytes(&pending, 64)? != digest.as_bytes() {
        return Err(
            "Resume the pending provisioning manifest before changing configuration".into(),
        );
    }
    if done.exists() {
        let record: Value =
            serde_json::from_slice(&private_bytes(&done, contract::MAX_REGISTRY_BYTES)?)
                .map_err(|_| "Invalid provisioning record")?;
        if record["manifest_sha256"] != digest {
            return Err("Provisioning record mismatch".into());
        }
        for space in entries(&record, "spaces", contract::MAX_SPACES)? {
            for import in entries(space, "imports", contract::MAX_APPS)? {
                hub.recover_legacy_original(import)?;
            }
        }
        if pending.exists() {
            fs::remove_file(&pending).map_err(|e| e.to_string())?;
            fs::File::open(parent)
                .and_then(|d| d.sync_all())
                .map_err(|e| e.to_string())?;
        }
        println!("{}", record);
        return Ok(());
    }
    persist(&pending, digest.as_bytes()).map_err(|e| e.to_string())?;
    let mut records = vec![];
    let mut names = std::collections::BTreeSet::new();
    for item in entries(&config, "spaces", contract::MAX_SPACES)? {
        let name = text(item, "name")?;
        if !names.insert(name) {
            return Err("Duplicate provisioning Space name".into());
        }
        let admins = entries(item, "admins", contract::MAX_MEMBERS)?;
        let actor = admins
            .first()
            .and_then(Value::as_str)
            .filter(|id| core::valid_identity(id))
            .ok_or("Explicit bootstrap Admin required")?;
        let matches = hub
            .registry()
            .map_err(|e| e.to_string())?
            .spaces
            .iter()
            .filter(|s| s.name == name)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            return Err("Ambiguous provisioning Space name".into());
        }
        let space = if let Some(s) = matches.first() {
            s.id.clone()
        } else {
            apply(hub, actor, Change::Create { name: name.into() })?;
            hub.registry()
                .map_err(|e| e.to_string())?
                .spaces
                .last()
                .ok_or("Missing created Space")?
                .id
                .clone()
        };
        for (values, role) in [
            (admins, Role::Admin),
            (
                entries(item, "members", contract::MAX_MEMBERS)?,
                Role::Member,
            ),
        ] {
            for identity in values {
                let identity = identity
                    .as_str()
                    .filter(|id| core::valid_identity(id))
                    .ok_or("Invalid provisioned identity")?;
                let s = hub
                    .registry()
                    .map_err(|e| e.to_string())?
                    .spaces
                    .iter()
                    .find(|s| s.id == space)
                    .ok_or("Missing Space")?;
                if core::role(s, Some(identity)) != Some(role) {
                    apply(
                        hub,
                        actor,
                        Change::Membership {
                            space: space.clone(),
                            identity: identity.into(),
                            role: Some(role),
                        },
                    )?;
                }
            }
        }
        let mut imports = vec![];
        for entry in entries(item, "apps", contract::MAX_APPS)? {
            let id = text(entry, "id")?;
            let attached = hub
                .registry()
                .map_err(|e| e.to_string())?
                .spaces
                .iter()
                .find(|s| s.id == space)
                .is_some_and(|s| s.apps.iter().any(|a| a.attached && a.policy.app == id));
            if !attached {
                apply(
                    hub,
                    actor,
                    Change::Attach {
                        space: space.clone(),
                        app: id.into(),
                    },
                )?;
            }
            if let Some(source) = entry["legacy_state_ref"].as_str() {
                if entry.get("legacy_roots").is_some() {
                    return Err("Choose one legacy root declaration".into());
                }
                imports.push(hub.import_legacy(actor, &space, id, source)?);
            }
            if let Some(roots) = entry.get("legacy_roots") {
                let roots = roots
                    .as_array()
                    .filter(|r| r.len() <= contract::MAX_ROOTS)
                    .ok_or("Invalid legacy roots")?;
                let mut seen = std::collections::BTreeSet::new();
                for root in roots {
                    let source = root
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 128)
                        .ok_or("Invalid legacy root")?;
                    if !seen.insert(source) {
                        return Err("Duplicate legacy root".into());
                    }
                    imports.push(hub.import_legacy(actor, &space, id, source)?);
                }
            }
            if let Some(root) = entry.get("legacy_chat_root") {
                let root = root
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .ok_or("Invalid legacy chat root")?;
                imports.push(hub.import_legacy_chat(actor, &space, id, root)?);
            }
        }
        records.push(json!({"name":name,"space":space,"imports":imports}));
    }
    if let Some(services) = config["services"].as_array() {
        if services.len() > 64 {
            return Err("Service binding limit".into());
        }
        for service in services {
            let name = text(service, "space")?;
            let s = hub
                .registry()
                .map_err(|e| e.to_string())?
                .spaces
                .iter()
                .find(|s| s.name == name)
                .ok_or("Missing service Space")?;
            let admin = s
                .members
                .iter()
                .find(|m| m.role == Role::Admin)
                .ok_or("Missing service Space Admin")?;
            let grant = hub.service_binding(
                &admin.identity,
                &s.id,
                text(service, "app")?,
                text(service, "identity")?,
            )?;
            let output = Path::new(text(service, "output")?);
            let bytes = serde_json::to_vec(&grant).map_err(|e| e.to_string())?;
            if output.exists() {
                let existing: contract::ServiceBinding =
                    serde_json::from_slice(&private_bytes(output, 8192)?)
                        .map_err(|_| "Invalid existing service binding")?;
                if existing != grant {
                    return Err("Refusing to overwrite a different service binding".into());
                }
            } else {
                persist(output, &bytes).map_err(|e| e.to_string())?;
            }
        }
    }
    let record = json!({"version":1,"manifest_sha256":digest,"spaces":records});
    persist(&done, record.to_string().as_bytes()).map_err(|e| e.to_string())?;
    fs::remove_file(pending).map_err(|e| e.to_string())?;
    fs::File::open(parent)
        .and_then(|d| d.sync_all())
        .map_err(|e| e.to_string())?;
    println!("{record}");
    Ok(())
}
