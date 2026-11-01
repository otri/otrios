// SPDX-License-Identifier: LGPL-3.0-only
//! Trusted local Space controls, rendered identically by native and browser.
//! Publisher UIs never receive this action table or its management capability.
use super::*;
use crate::authoring as a;
use otrios_interfaces::layout::Value;
use otrios_interfaces::spaces::{AppAvailability, Change, Discovery};
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
    Cleanup,
    AudienceConfirm {
        space: String,
        audience: otrios_interfaces::spaces::Audience,
    },
    Installed,
    Members,
    AccessAudience,
    Create,
    SpaceSettings,
    BackgroundColor,
    AppSettings(String),
    Profile,
    ProfileEdit,
    DeletedSpaces,
    RemoveApp(String),
    Dataset(String),
    DeleteDataset(String),
    RestoreDataset(String),
    DeleteSpace,
    Datasets,
    BeginMove {
        app: String,
        collection: Option<String>,
        destination: Option<String>,
        generation: u64,
        cursor: otrios_interfaces::spaces::DiscoveryCursor,
        catalog: Vec<u8>,
    },
    MoveDestinations,
    MoveReview,
}
#[derive(Clone)]
pub(super) struct MoveDraft {
    pub source: String,
    pub destination: Option<String>,
    pub app: String,
    pub collection: String,
    pub title: String,
    pub attached: bool,
    pub generation: u64,
    pub cursor: otrios_interfaces::spaces::DiscoveryCursor,
    pub catalog: Vec<u8>,
    pub mode: otrios_interfaces::spaces::MoveMode,
    pub operation: String,
}
#[derive(Clone)]
pub(super) struct Review {
    pub space: String,
    pub generation: u64,
    pub collection: Option<String>,
}
#[derive(Clone)]
pub(super) struct AccessDraft {
    pub space: String,
    pub generation: u64,
    pub cursor: otrios_interfaces::spaces::DiscoveryCursor,
    pub catalog: Vec<u8>,
    pub audience: otrios_interfaces::spaces::Audience,
    pub original_audience: otrios_interfaces::spaces::Audience,
    pub members: Vec<otrios_interfaces::spaces::Member>,
    pub original_members: Vec<otrios_interfaces::spaces::Member>,
}

#[derive(Default)]
pub(super) struct Model {
    pub pane: Pane,
    pub page: usize,
    pub map_page: usize,
    pub panel_scroll: f32,
    pub panel_control: Option<u32>,
    pub message: String,
    pub editing: bool,
    pub remove_data: bool,
    pub review: Option<Review>,
    pub moving: Option<MoveDraft>,
    pub access: Option<AccessDraft>,
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
    pub profile_revision: u64,
}
#[derive(Clone, Debug)]
pub(super) enum HostAction {
    Pane(Pane),
    Page(i8),
    Launch(String, String),
    Manage(Change),
    Create,
    SignIn,
    SignOut,
    Edit,
    MapSpace(String),
    SpaceBody(String),
    MapApp(String, String),
    MapDataset(String, String, String),
    MoveDestination(String),
    MoveMode(otrios_interfaces::spaces::MoveMode),
    MoveConfirm,
    AccessAudience(otrios_interfaces::spaces::Audience),
    AccessRole(String),
    AccessRemove(String),
    AccessAdd,
    AccessSave,
    AccessApply,
    ProfileSave,
    BackgroundApply,
    Settings(String, Pane),
    Resize(String),
    Background(otrios_interfaces::spaces::Background),
    ToggleRemoveData,
    Reviewed(Change, u64),
    Unavailable(&'static str),
}
pub(super) struct Rendered {
    pub ui: Ui,
    pub backdrop: Option<otrios_interfaces::scene::SceneNode>,
    pub actions: BTreeMap<u8, HostAction>,
    pub map_viewport: Option<u16>,
    pub panel_viewport: Option<(u16, [f32; 2])>,
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
struct Builder<'a> {
    font: Option<&'a crate::dashboard::font::Font>,
    nodes: Vec<a::Element>,
    actions: Vec<HostAction>,
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
    map::render(snapshot, model, draft, sign_in, sign_out, viewport, font)
}
