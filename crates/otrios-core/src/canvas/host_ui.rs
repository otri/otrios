// SPDX-License-Identifier: LGPL-3.0-only
//! Trusted local Space controls, rendered identically by native and browser.
//! Publisher UIs never receive this action table or its management capability.
use super::*;
use crate::authoring as a;
use otrios_interfaces::layout::Value;
use otrios_interfaces::spaces::{AppAvailability, Change, Discovery, Role};
use std::collections::BTreeMap;
mod map;
pub(super) use map::{Drag, DragKind};

pub(super) const FIELD: a::StateField = a::StateField::text(1, "entry");
const PAGE: usize = 4;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) enum Pane {
    #[default]
    Home,
    Apps,
    Spaces,
    Data,
    App(String),
    Delete {
        app: String,
        collection: String,
        remove: bool,
    },
    Cleanup,
    Sharing,
    AudienceConfirm {
        space: String,
        audience: otrios_interfaces::spaces::Audience,
    },
    Installed,
    Members,
    Person(String),
    Create,
    Invite,
    SpaceSettings,
    AppSettings(String),
    Profile,
}
#[derive(Default)]
pub(super) struct Model {
    pub pane: Pane,
    pub page: usize,
    pub message: String,
    pub editing: bool,
    pub scroll: f32,
    pub scroll_limit: f32,
    pub drag: Option<Drag>,
    pub destination: Option<(String, Pane, Option<String>)>,
    pub background: Option<otrios_interfaces::spaces::Background>,
    pub field_owner: Option<String>,
    pub form_generation: u64,
    pub form_value: String,
    pub form_pending: Option<(String, String)>,
    pub form_blocked: bool,
}
#[derive(Clone, Debug)]
pub(super) enum HostAction {
    Pane(Pane),
    Page(i8),
    Space(String),
    Launch(String, String),
    Manage(Change),
    Create,
    Invite(Role),
    SignIn,
    SignOut,
    Edit,
    MapSpace(String),
    SpaceBody(String),
    MapApp(String, String),
    Settings(String, Pane),
    Resize(String),
    Background(otrios_interfaces::spaces::Background),
    SaveSpace,
    Unavailable(&'static str),
}
pub(super) struct Rendered {
    pub ui: Ui,
    pub actions: BTreeMap<u8, HostAction>,
    pub map_viewport: Option<u16>,
}
fn short(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.into();
    }
    let mut end = limit.saturating_sub(3);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}
fn stored(value: Option<u64>) -> String {
    match value {
        Some(n) if n >= 1024 * 1024 => format!("{:.1} MiB stored", n as f64 / (1024. * 1024.)),
        Some(n) if n >= 1024 => format!("{:.1} KiB stored", n as f64 / 1024.),
        Some(n) => format!("{n} bytes stored"),
        None => "Storage usage unavailable".into(),
    }
}
fn deadline(unix_ms: u64) -> String {
    let days = (unix_ms / 86_400_000) as i64 + 719_468;
    let era = days / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let minute = unix_ms / 60_000;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minute / 60 % 24,
        minute % 60
    )
}
fn availability(status: AppAvailability) -> &'static str {
    match status {
        AppAvailability::Available => "Available",
        AppAvailability::Unavailable => "Unavailable on this device",
        AppAvailability::Incompatible => "Data needs a compatible app version",
    }
}
struct Builder {
    nodes: Vec<a::Element>,
    actions: Vec<HostAction>,
}
impl Builder {
    fn text(&mut self, value: &str, y: f32) {
        self.nodes.push(
            a::text(short(value, 96).as_str())
                .frame([24., y, 1232., 32.])
                .pin_left(24.)
                .pin_right(24.)
                .font_scale(2.),
        );
    }
    fn button(&mut self, label: &str, frame: [f32; 4], action: HostAction) {
        self.nodes.push(
            a::button(&short(label, 80))
                .frame(frame)
                .on_press(a::Event::back()),
        );
        self.actions.push(action);
    }
    fn row(&mut self, label: &str, detail: &str, index: usize, action: Option<HostAction>) {
        let y = 240. + index as f32 * 88.;
        if let Some(action) = action {
            self.nodes.push(
                a::button(&short(label, 32))
                    .frame([24., y, 1232., 48.])
                    .pin_left(24.)
                    .pin_right(24.)
                    .on_press(a::Event::back()),
            );
            self.actions.push(action);
        } else {
            self.text(label, y);
        }
        self.nodes.push(
            a::text(short(detail, 96).as_str())
                .frame([32., y + 52., 1216., 28.])
                .pin_left(32.)
                .pin_right(32.)
                .font_scale(2.),
        );
    }
    fn field(&mut self, label: &str) {
        self.text(label, 224.);
        self.nodes.push(
            a::canvas(vec![
                a::surface(a::CARD, 10.).pin_edges(0.),
                a::text(FIELD.value())
                    .frame([16., 20., 1200., 40.])
                    .pin_left(16.)
                    .pin_right(16.),
            ])
            .frame([24., 264., 1232., 80.])
            .pin_left(24.)
            .pin_right(24.)
            .semantic_label(label)
            .on_press(a::Event::focus(FIELD)),
        );
    }
    fn pages(&mut self, page: usize, count: usize) {
        if page > 0 {
            self.button("Previous", [24., 640., 192., 64.], HostAction::Page(-1));
        }
        if (page + 1) * PAGE < count {
            self.button("Next", [232., 640., 192., 64.], HostAction::Page(1));
        }
    }
}
pub(super) fn render(
    snapshot: &Discovery,
    model: &mut Model,
    draft: &str,
    sign_in: bool,
    sign_out: bool,
    viewport: [f32; 2],
    font: Option<&crate::dashboard::font::Font>,
) -> Result<Rendered> {
    if matches!(
        model.pane,
        Pane::Home
            | Pane::Spaces
            | Pane::SpaceSettings
            | Pane::AppSettings(_)
            | Pane::Profile
            | Pane::Installed
    ) {
        return map::render(snapshot, model, draft, sign_in, sign_out, viewport, font);
    }
    let selected = snapshot.selected.as_ref();
    let summary = selected.and_then(|s| snapshot.spaces.iter().find(|v| v.id == s.space));
    let admin = summary.is_some_and(|s| s.role == Some(Role::Admin));
    if selected.is_none() && !matches!(model.pane, Pane::Spaces | Pane::Create) {
        model.pane = Pane::Spaces;
        model.page = 0;
    }
    if !admin
        && matches!(
            model.pane,
            Pane::Data
                | Pane::Delete { .. }
                | Pane::Cleanup
                | Pane::Sharing
                | Pane::AudienceConfirm { .. }
                | Pane::Installed
                | Pane::Members
                | Pane::Person(_)
                | Pane::Invite
        )
    {
        model.pane = Pane::Home;
        model.page = 0;
    }
    if snapshot.identity.is_none() && matches!(model.pane, Pane::Create) {
        model.pane = Pane::Spaces;
    }
    if let Pane::Delete {
        app, collection, ..
    } = &model.pane
        && selected
            .and_then(|s| s.apps.iter().find(|a| &a.app == app))
            .is_none_or(|a| &a.collection != collection)
    {
        model.pane = Pane::Data;
        model.message = "Data changed; review the collection again.".into();
    }
    if let Pane::AudienceConfirm { space, .. } = &model.pane
        && selected.is_none_or(|selected| &selected.space != space)
    {
        model.pane = Pane::Home;
    }
    let title = match model.pane {
        Pane::Home => "Home",
        Pane::Apps => "Apps",
        Pane::Spaces => "Spaces",
        Pane::Data => "Apps & Data",
        Pane::App(_) => "App data",
        Pane::Delete { .. } => "Confirm permanent deletion",
        Pane::Cleanup => "Deletion cleanup",
        Pane::Sharing => "Space sharing",
        Pane::AudienceConfirm { .. } => "Confirm public access",
        Pane::Installed => "Add app",
        Pane::Members => "Members",
        Pane::Person(_) => "Membership",
        Pane::Create => "Create Space",
        Pane::Invite => "Add member",
        Pane::SpaceSettings | Pane::AppSettings(_) | Pane::Profile => unreachable!(),
    };
    let mut b = Builder {
        nodes: vec![],
        actions: vec![],
    };
    b.text(
        &summary.map_or_else(|| "Your Spaces".into(), |s| short(&s.name, 32)),
        112.,
    );
    b.button("Home", [24., 160., 192., 64.], HostAction::Pane(Pane::Home));
    if matches!(model.pane, Pane::Home | Pane::Spaces) {
        b.button(
            "Spaces",
            [232., 160., 192., 64.],
            HostAction::Pane(Pane::Spaces),
        );
        if matches!(model.pane, Pane::Home) && admin {
            b.button(
                "Apps & Data",
                [440., 160., 216., 64.],
                HostAction::Pane(Pane::Data),
            );
        }
    }
    match &model.pane {
        Pane::Spaces => {
            let count = snapshot.spaces.len();
            model.page = model.page.min(count.saturating_sub(1) / PAGE);
            for (index, space) in snapshot
                .spaces
                .iter()
                .skip(model.page * PAGE)
                .take(PAGE)
                .enumerate()
            {
                b.row(
                    &space.name,
                    match space.role {
                        Some(Role::Admin) => "Admin",
                        Some(Role::Member) => "Member",
                        None => "Visitor",
                    },
                    index,
                    Some(HostAction::Space(space.id.clone())),
                );
            }
            if count == 0 {
                b.text(
                    if snapshot.identity.is_some() {
                        "No Spaces yet."
                    } else {
                        "Sign in to see your Spaces."
                    },
                    256.,
                );
            }
            b.pages(model.page, count);
            if snapshot.identity.is_some() {
                b.button(
                    "Create Space",
                    [440., 640., 216., 64.],
                    HostAction::Pane(Pane::Create),
                );
            }
        }
        Pane::Home | Pane::Apps | Pane::Data => {
            if let Some(selected) = selected {
                let data = matches!(model.pane, Pane::Data);
                let mut apps = selected
                    .apps
                    .iter()
                    .filter(|app| app.attached || data)
                    .collect::<Vec<_>>();
                apps.sort_by_key(|app| !app.attached);
                model.page = model.page.min(apps.len().saturating_sub(1) / PAGE);
                for (index, app) in apps.iter().skip(model.page * PAGE).take(PAGE).enumerate() {
                    let detail = if app.attached {
                        availability(app.availability).into()
                    } else {
                        format!("Data from removed apps · {}", stored(app.stored_bytes))
                    };
                    let action = if data {
                        Some(HostAction::Pane(Pane::App(app.app.clone())))
                    } else {
                        app.launch
                            .as_ref()
                            .map(|p| HostAction::Launch(p.ui_ref.clone(), p.state_ref.clone()))
                    };
                    b.row(&app.title, &detail, index, action);
                }
                if apps.is_empty() {
                    b.text("No apps in this Space.", 256.);
                }
                b.pages(model.page, apps.len());
                if admin {
                    b.button(
                        "Add app",
                        [440., 640., 192., 64.],
                        HostAction::Pane(Pane::Installed),
                    );
                    if data {
                        b.button(
                            "Sharing",
                            [24., 728., 216., 48.],
                            HostAction::Pane(Pane::Sharing),
                        );
                        b.button(
                            "Cleanup",
                            [440., 160., 216., 64.],
                            HostAction::Pane(Pane::Cleanup),
                        );
                        b.button(
                            "Members",
                            [232., 160., 192., 64.],
                            HostAction::Pane(Pane::Members),
                        );
                    }
                }
            }
        }
        Pane::App(app) => {
            if let Some((selected, app)) =
                selected.and_then(|s| s.apps.iter().find(|a| &a.app == app).map(|a| (s, a)))
            {
                b.text(&app.title, 240.);
                b.text(availability(app.availability), 288.);
                b.text(&stored(app.stored_bytes), 336.);
                if let Some(p) = &app.launch {
                    b.button(
                        "Open",
                        [24., 400., 192., 64.],
                        HostAction::Launch(p.ui_ref.clone(), p.state_ref.clone()),
                    );
                }
                if admin {
                    let change = if app.attached {
                        Change::Remove {
                            space: selected.space.clone(),
                            app: app.app.clone(),
                        }
                    } else {
                        Change::Attach {
                            space: selected.space.clone(),
                            app: app.app.clone(),
                        }
                    };
                    if app.attached || app.availability == AppAvailability::Available {
                        b.button(
                            if app.attached {
                                "Remove app"
                            } else {
                                "Add app"
                            },
                            [232., 400., 216., 64.],
                            HostAction::Manage(change),
                        );
                    }
                    b.text("Removing an app retains its data in this Space.", 496.);
                    if selected.cleanup.is_some() {
                        b.button(
                            "Delete data",
                            [24., 540., 280., 56.],
                            HostAction::Pane(Pane::Delete {
                                app: app.app.clone(),
                                collection: app.collection.clone(),
                                remove: false,
                            }),
                        );
                        if app.attached {
                            b.button(
                                "Remove & delete",
                                [328., 540., 312., 56.],
                                HostAction::Pane(Pane::Delete {
                                    app: app.app.clone(),
                                    collection: app.collection.clone(),
                                    remove: true,
                                }),
                            );
                        }
                    }
                }
            }
        }
        Pane::Delete {
            app,
            collection,
            remove,
        } => {
            if let Some((selected, app)) =
                selected.and_then(|s| s.apps.iter().find(|a| &a.app == app).map(|a| (s, a)))
            {
                b.text(
                    &format!("Space: {}", summary.map_or("", |s| s.name.as_str())),
                    240.,
                );
                b.text(&format!("App: {}", app.title), 288.);
                b.text(
                    "Deletes this collection's content, assets and history.",
                    352.,
                );
                b.text(
                    if *remove {
                        "The app will also be removed from this Space."
                    } else {
                        "A fresh, empty collection replaces this one."
                    },
                    400.,
                );
                b.text(
                    "Backups and qualified originals expire under the seven-day policy.",
                    448.,
                );
                b.text(
                    "Offline device copies and user exports cannot be verified.",
                    496.,
                );
                b.button(
                    "Cancel",
                    [24., 560., 216., 64.],
                    HostAction::Pane(Pane::App(app.app.clone())),
                );
                if selected.cleanup.is_some() {
                    b.button(
                        "Permanently delete",
                        [264., 560., 336., 64.],
                        HostAction::Manage(Change::DeleteData {
                            space: selected.space.clone(),
                            app: app.app.clone(),
                            collection: collection.clone(),
                            remove: *remove,
                        }),
                    );
                }
            }
        }
        Pane::Cleanup => {
            if let Some(cleanup) = selected.and_then(|s| s.cleanup.as_ref()) {
                b.text(
                    &format!("Deleted collections: {}", cleanup.deleted_collections),
                    240.,
                );
                b.text(
                    &if cleanup.local_pending > 0 {
                        format!(
                            "Authority/session cleanup pending: {}",
                            cleanup.local_pending
                        )
                    } else {
                        "Deleted collection files and sessions cleared.".into()
                    },
                    288.,
                );
                b.text(
                    &match cleanup.backup_until_unix_ms {
                        Some(until) => format!("Backup copies expire: {}", deadline(until)),
                        None if cleanup.storage_unknown => {
                            "Backup retention could not be verified.".into()
                        }
                        None => "No managed backup copies retained.".into(),
                    },
                    352.,
                );
                b.text(
                    &if cleanup.legacy_qualification_required {
                        "Imported originals await migration qualification.".into()
                    } else if let Some(until) = cleanup.legacy_until_unix_ms {
                        format!("Imported originals expire: {}", deadline(until))
                    } else if cleanup.storage_unknown {
                        "Migration-original retention is unverified.".into()
                    } else {
                        "No imported originals retained.".into()
                    },
                    400.,
                );
                if cleanup.storage_unknown {
                    b.text("Storage inventory needs operator attention.", 448.);
                }
                b.text(
                    "Offline device copies and user exports cannot be verified.",
                    496.,
                );
            } else {
                b.text("Cleanup status unavailable.", 240.);
            }
            b.button(
                "Apps & Data",
                [24., 640., 240., 64.],
                HostAction::Pane(Pane::Data),
            );
        }
        Pane::Sharing => {
            if let Some(selected) = selected {
                use otrios_interfaces::spaces::Audience;
                let audience = summary.map_or(Audience::Closed, |s| s.audience);
                b.text(
                    match audience {
                        Audience::Closed => "Closed: only Members and Admins have access.",
                        Audience::PublicView => "Public view: visitors read; Members collaborate.",
                        Audience::PublicComment => {
                            "Public view and comment: signed-in visitors may comment."
                        }
                    },
                    240.,
                );
                b.button(
                    "Keep closed",
                    [24., 320., 240., 64.],
                    HostAction::Manage(Change::Audience {
                        space: selected.space.clone(),
                        audience: Audience::Closed,
                    }),
                );
                b.button(
                    "Public view",
                    [24., 416., 280., 64.],
                    HostAction::Pane(Pane::AudienceConfirm {
                        space: selected.space.clone(),
                        audience: Audience::PublicView,
                    }),
                );
                b.button(
                    "Public view & comment",
                    [24., 512., 400., 64.],
                    HostAction::Pane(Pane::AudienceConfirm {
                        space: selected.space.clone(),
                        audience: Audience::PublicComment,
                    }),
                );
                b.text("Sharing applies to every attached app in this Space.", 624.);
                b.text(
                    "Closed-only apps must be removed before making it public.",
                    672.,
                );
            }
        }
        Pane::AudienceConfirm { space, audience } => {
            b.text(
                &format!("Space: {}", summary.map_or("", |s| s.name.as_str())),
                240.,
            );
            b.text(
                "Anyone will be able to view its attached app content.",
                320.,
            );
            b.text(
                if *audience == otrios_interfaces::spaces::Audience::PublicComment {
                    "Signed-in visitors may comment where the app supports it."
                } else {
                    "Visitors cannot edit documents or submit comments."
                },
                368.,
            );
            b.text("Members continue editing and moderating content.", 416.);
            b.button(
                "Cancel",
                [24., 512., 216., 64.],
                HostAction::Pane(Pane::Sharing),
            );
            b.button(
                "Make public",
                [264., 512., 280., 64.],
                HostAction::Manage(Change::Audience {
                    space: space.clone(),
                    audience: *audience,
                }),
            );
        }
        Pane::Installed => {
            if let Some(selected) = selected {
                let apps = selected
                    .installed
                    .iter()
                    .filter(|p| {
                        !selected
                            .apps
                            .iter()
                            .any(|a| a.attached && a.app == p.policy.app)
                    })
                    .collect::<Vec<_>>();
                model.page = model.page.min(apps.len().saturating_sub(1) / PAGE);
                for (index, app) in apps.iter().skip(model.page * PAGE).take(PAGE).enumerate() {
                    let compatible = selected
                        .apps
                        .iter()
                        .find(|a| a.app == app.policy.app)
                        .is_none_or(|a| a.availability == AppAvailability::Available);
                    let public = summary
                        .is_some_and(|s| s.audience != otrios_interfaces::spaces::Audience::Closed);
                    let allowed = compatible && !(public && app.policy.closed_only);
                    b.row(
                        &app.title,
                        if allowed {
                            "Add to this Space"
                        } else {
                            "Needs a compatible app or Space"
                        },
                        index,
                        allowed.then(|| {
                            HostAction::Manage(Change::Attach {
                                space: selected.space.clone(),
                                app: app.policy.app.clone(),
                            })
                        }),
                    );
                }
                if apps.is_empty() {
                    b.text("All installed apps are attached.", 256.);
                }
                b.pages(model.page, apps.len());
            }
        }
        Pane::Members => {
            if let Some(members) = selected.and_then(|s| s.members.as_ref()) {
                model.page = model.page.min(members.len().saturating_sub(1) / PAGE);
                for (index, member) in members
                    .iter()
                    .skip(model.page * PAGE)
                    .take(PAGE)
                    .enumerate()
                {
                    b.row(
                        &member.identity,
                        if member.role == Role::Admin {
                            "Admin"
                        } else {
                            "Member"
                        },
                        index,
                        Some(HostAction::Pane(Pane::Person(member.identity.clone()))),
                    );
                }
                b.pages(model.page, members.len());
                b.button(
                    "Add member",
                    [440., 640., 216., 64.],
                    HostAction::Pane(Pane::Invite),
                );
            }
        }
        Pane::Person(identity) => {
            if let Some((selected, members)) =
                selected.and_then(|s| s.members.as_ref().map(|m| (s, m)))
                && let Some(member) = members.iter().find(|m| &m.identity == identity)
            {
                for index in 0..2 {
                    let line = member
                        .identity
                        .chars()
                        .skip(index * 48)
                        .take(48)
                        .collect::<String>();
                    if !line.is_empty() {
                        b.nodes.push(
                            a::text(line.as_str())
                                .frame([24., 240. + index as f32 * 32., 1232., 32.])
                                .pin_left(24.)
                                .pin_right(24.)
                                .font_scale(2.),
                        );
                    }
                }
                let last = member.role == Role::Admin
                    && members.iter().filter(|m| m.role == Role::Admin).count() == 1;
                if last {
                    b.text("A Space needs at least one Admin.", 304.);
                } else {
                    let role = if member.role == Role::Admin {
                        Role::Member
                    } else {
                        Role::Admin
                    };
                    b.button(
                        if role == Role::Admin {
                            "Make Admin"
                        } else {
                            "Make Member"
                        },
                        [24., 336., 216., 64.],
                        HostAction::Manage(Change::Membership {
                            space: selected.space.clone(),
                            identity: identity.clone(),
                            role: Some(role),
                        }),
                    );
                    b.button(
                        "Remove member",
                        [256., 336., 248., 64.],
                        HostAction::Manage(Change::Membership {
                            space: selected.space.clone(),
                            identity: identity.clone(),
                            role: None,
                        }),
                    );
                }
            }
        }
        Pane::Create => {
            b.field("Space name");
            b.button("Create Space", [24., 368., 216., 64.], HostAction::Create);
        }
        Pane::Invite => {
            b.field("Member identity");
            b.button(
                "Add Member",
                [24., 368., 216., 64.],
                HostAction::Invite(Role::Member),
            );
            b.button(
                "Add Admin",
                [256., 368., 216., 64.],
                HostAction::Invite(Role::Admin),
            );
        }
        Pane::SpaceSettings | Pane::AppSettings(_) | Pane::Profile => unreachable!(),
    }
    let form = matches!(model.pane, Pane::Create | Pane::Invite);
    if !model.message.is_empty() {
        b.text(
            &model.message,
            if form {
                440.
            } else if model.pane == Pane::Sharing {
                728.
            } else if matches!(model.pane, Pane::Delete { .. }) {
                672.
            } else {
                596.
            },
        );
    }
    if matches!(model.pane, Pane::Home | Pane::Spaces) {
        if sign_out && snapshot.identity.is_some() {
            b.nodes.push(
                a::button("Sign out")
                    .frame([24., 728., 192., 48.])
                    .visible(a::session_controls_visible())
                    .on_press(a::Event::back()),
            );
            b.actions.push(HostAction::SignOut);
        } else if sign_in && snapshot.identity.is_none() {
            b.button("Sign in", [24., 728., 192., 48.], HostAction::SignIn);
        }
    }
    if form {
        b.nodes.push(
            crate::components::keyboard()
                .pin_left(0.)
                .pin_right(0.)
                .pin_bottom(0.),
        );
    }
    let mut schema = a::schema("host-spaces-form-v1", &[FIELD]);
    schema.fields[0].default = Value::Text(draft.into());
    let flow = || a::column(0., vec![a::heading()]);
    let mut description =
        a::AppUi::new("spaces-host", title, schema).with_composition(flow(), flow());
    if form {
        description = description.with_presentation(Presentation::Editor { fields: vec![1] });
    }
    let ui = description.build(title, b.nodes);
    let controls = ui
        .handlers
        .iter()
        .filter(|h| h.operation == Operation::Back)
        .map(|h| h.control)
        .collect::<Vec<_>>();
    if controls.len() != b.actions.len() {
        return Err("Host action layout mismatch".into());
    }
    Ok(Rendered {
        ui,
        actions: controls.into_iter().zip(b.actions).collect(),
        map_viewport: None,
    })
}
