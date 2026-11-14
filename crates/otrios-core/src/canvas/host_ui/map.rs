// SPDX-License-Identifier: LGPL-3.0-only
//! Local map presentation. Only admitted authority metadata supplies its content.
use super::*;
use otrios_interfaces::{
    layout::Expr,
    spaces::{Background, Discovery, Role},
};
mod access;
mod moves;
mod panel;
mod retained;
mod transfers;
const PAPER: [u8; 4] = [247, 246, 240, 255];
const INK: [u8; 4] = [44, 50, 43, 255];
const SOFT: [u8; 4] = [109, 116, 105, 255];
const WHITE: [u8; 4] = [255, 254, 250, 255];
pub(super) const TOP: f32 = 104.;
pub(super) const FOOT: f32 = 80.;
#[derive(Clone, Debug)]
pub(in crate::canvas) enum DragKind {
    Space(String),
    App(String, String),
    Dataset(String, String, String),
    Resize(String),
    Pan,
}
#[derive(Clone, Debug)]
pub(in crate::canvas) struct Drag {
    pub pointer: u32,
    pub epoch: u32,
    pub generation: u64,
    pub kind: DragKind,
    pub start: [f32; 2],
    pub position: [f32; 2],
    pub moving: bool,
    pub last_tick: u64,
    pub action: Option<HostAction>,
}
fn color(value: Background) -> [u8; 4] {
    match value {
        Background::None => PAPER,
        Background::Sand => [238, 229, 210, 255],
        Background::Sage => [229, 235, 222, 255],
        Background::Stone => [234, 234, 226, 255],
        Background::Sky => [223, 233, 238, 255],
        Background::Lilac => [232, 225, 239, 255],
        Background::Rose => [241, 224, 221, 255],
        Background::Rgb([r, g, b]) => [r, g, b, 255],
    }
}
fn text(value: &str, frame: [f32; 4], scale: f32) -> a::Element {
    a::text(short(value, 94).as_str())
        .frame(frame)
        .font_scale(scale.round().max(1.))
        .color(INK)
}
fn caption_lines(
    value: &str,
    width: f32,
    font: Option<&crate::dashboard::font::Font>,
) -> Vec<String> {
    let fits = |text: &str| {
        font.map_or(text.chars().count() as f32 * 13., |f| {
            f.text_width(text, 2.)
        }) <= width
    };
    let mut rest = value.trim();
    let mut lines = vec![];
    for line in 0..2 {
        if rest.is_empty() {
            break;
        }
        if rest.len() <= 94 && fits(rest) {
            lines.push(rest.into());
            break;
        }
        let mut end = 0;
        for (i, c) in rest.char_indices() {
            let next = i + c.len_utf8();
            let candidate = if line == 1 {
                format!("{}…", &rest[..next])
            } else {
                rest[..next].into()
            };
            if candidate.len() > 94 || !fits(&candidate) {
                break;
            }
            end = next;
        }
        if end == 0 {
            break;
        }
        if line == 1 {
            lines.push(format!("{}…", rest[..end].trim_end()));
            break;
        }
        let prefix = &rest[..end];
        let split = prefix
            .rfind(char::is_whitespace)
            .filter(|i| *i > 0)
            .unwrap_or(end);
        lines.push(rest[..split].trim_end().into());
        rest = rest[split..].trim_start();
    }
    lines
}
fn app_tile(
    app: &str,
    title: &str,
    width: f32,
    available: bool,
    font: Option<&crate::dashboard::font::Font>,
) -> Vec<a::Element> {
    let mut nodes = vec![
        a::rounded_rect(if available {
            WHITE
        } else {
            [220, 221, 213, 255]
        })
        .frame([(width - 72.) / 2., 0., 72., 72.])
        .corner_radius(20.0.into()),
        crate::components::app_icon(app).frame([(width - 48.) / 2., 12., 48., 48.]),
    ];
    for (line, caption) in caption_lines(title, width - 8., font).iter().enumerate() {
        let measured = font.map_or(caption.chars().count() as f32 * 13., |f| {
            f.text_width(caption, 2.)
        });
        let x = ((width - measured) / 2.).max(4.);
        nodes.push(text(
            caption,
            [x, 84. + line as f32 * 28., width - x - 4., 28.],
            2.,
        ));
    }
    nodes
}
impl Builder<'_> {
    fn remove_data(&mut self, checked: bool, frame: [f32; 4]) {
        // Draw the indicator with shared primitives: the bundled font does not
        // contain the former circle glyphs, so both states looked identical.
        let mut nodes = vec![
            a::rounded_rect(if checked { [52, 77, 61, 255] } else { SOFT })
                .frame([10., 16., 24., 24.])
                .corner_radius(4.0.into()),
        ];
        if checked {
            nodes.push(
                a::series(
                    a::ValueExpr::Literal(Expr::Literal(Value::Series(vec![55., 25., 85.]))),
                    WHITE,
                )
                .frame([15., 20., 14., 16.]),
            );
        } else {
            nodes.push(
                a::rounded_rect(WHITE)
                    .frame([12., 18., 20., 20.])
                    .corner_radius(2.0.into()),
            );
        }
        let line = self.font.map_or(32., |f| f.metrics().line_height(2.));
        nodes.push(text(
            "Remove Data",
            [48., (frame[3] - line) / 2., frame[2] - 56., line],
            2.,
        ));
        self.nodes.push(
            a::canvas(nodes)
                .frame(frame)
                .semantic_label("Remove Data")
                .semantic_checked(checked)
                .on_press(a::Event::back()),
        );
        self.actions.push(HostAction::ToggleRemoveData);
    }
    fn profile(&mut self, avatar: &str, width: f32, font: Option<&crate::dashboard::font::Font>) {
        let (x, y) = font.map_or((19.5, 18.), |font| {
            let [left, bottom, right, top] = font.measure_text(avatar, 2.).ink_bounds;
            (
                26. - (left + right) / 2.,
                26. - font.metrics().baseline_from_top(2.) + (bottom + top) / 2.,
            )
        });
        self.nodes.push(
            a::canvas(vec![
                a::circle(color(Background::Sand)).frame([0., 0., 52., 52.]),
                text(avatar, [x, y, (52. - x).max(1.), 32.], 2.),
            ])
            .frame([width - 76., 22., 52., 52.])
            .semantic_label("Profile")
            .on_press(a::Event::back()),
        );
        self.actions.push(HostAction::Pane(Pane::Profile));
    }
    fn flat(&mut self, label: &str, caption: &str, frame: [f32; 4], action: HostAction) {
        let primary = matches!(
            &action,
            HostAction::ProfileSave
                | HostAction::BackgroundApply
                | HostAction::AccessSave
                | HostAction::AccessApply
                | HostAction::MoveConfirm
                | HostAction::Create
                | HostAction::TransferCommit
                | HostAction::TransferUrl
                | HostAction::Reviewed(Change::RestoreDataset { .. }, _)
                | HostAction::Manage(Change::RestoreSpace { .. })
        ) || label == "Done";
        let outline = matches!(label, "Close" | "Cancel")
            || matches!(
                &action,
                HostAction::AccessRole(_) | HostAction::Pane(Pane::AccessAudience)
            );
        let danger = matches!(&action, HostAction::Pane(Pane::DeleteSpace));
        let mut nodes = vec![];
        if primary || outline {
            nodes.push(
                a::rounded_rect(if primary {
                    [52, 77, 61, 255]
                } else {
                    [221, 223, 212, 255]
                })
                .frame([0., 0., frame[2], frame[3]])
                .corner_radius(12.0.into()),
            );
            if !primary {
                nodes.push(
                    a::rounded_rect(WHITE)
                        .frame([1., 1., frame[2] - 2., frame[3] - 2.])
                        .corner_radius(11.0.into()),
                );
            }
        }
        let measured = self.font.map_or(caption.chars().count() as f32 * 13., |f| {
            f.text_width(caption, 2.)
        });
        let x = if primary || outline {
            ((frame[2] - measured) / 2.).max(8.)
        } else {
            10.
        };
        let line = self.font.map_or(32., |f| f.metrics().line_height(2.));
        nodes.push(
            text(
                caption,
                [x, (frame[3] - line) / 2., (frame[2] - x - 8.).max(1.), line],
                2.,
            )
            .color(if primary {
                WHITE
            } else if danger {
                [187, 57, 51, 255]
            } else {
                INK
            }),
        );
        self.nodes.push(
            a::canvas(nodes)
                .frame(frame)
                .semantic_label(&short(label, 94))
                .on_press(a::Event::back()),
        );
        self.actions.push(action);
    }
    fn stats(&mut self, bytes: Option<u64>, users: Option<u16>, frame: [f32; 4]) {
        let [x, y, width, _] = frame;
        for (i, (label, value)) in [
            (
                "Data used",
                bytes.map_or_else(
                    || "Unavailable".into(),
                    |n| stored(Some(n)).replace(" stored", ""),
                ),
            ),
            (
                "Active users",
                users.map_or_else(|| "Unavailable".into(), |n| n.to_string()),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let x = x + i as f32 * width / 2.;
            self.nodes
                .push(text(label, [x, y, width / 2. - 12., 28.], 1.3).color(SOFT));
            self.nodes
                .push(text(&value, [x, y + 36., width / 2. - 12., 40.], 2.));
        }
    }
    fn info(&mut self, label: &str, value: &str, frame: [f32; 4]) {
        let [x, y, width, height] = frame;
        self.nodes
            .push(text(label, [x, y, width * 0.4, height], 1.3).color(SOFT));
        for (line, value) in caption_lines(value, width * 0.58, None)
            .iter()
            .take(2)
            .enumerate()
        {
            let measured = self.font.map_or(value.chars().count() as f32 * 6.5, |f| {
                f.text_width(value, 1.)
            });
            let right = x + width - measured.min(width * 0.58);
            self.nodes.push(text(
                value,
                [right, y + line as f32 * 24., width * 0.58, 24.],
                1.3,
            ));
        }
    }
    fn divider(&mut self, frame: [f32; 4]) {
        self.nodes
            .push(a::rectangle([221, 223, 212, 255]).frame(frame));
    }
    fn name(&mut self, frame: [f32; 4]) {
        self.entry("Name", frame);
    }
    fn entry(&mut self, label: &str, frame: [f32; 4]) {
        self.nodes.push(
            a::canvas(vec![
                a::surface([221, 223, 212, 255], 12.).pin_edges(0.),
                a::surface(PAPER, 11.).pin_edges(1.),
                a::text(FIELD.value())
                    .frame([14., 15., frame[2] - 28., 40.])
                    .font_scale(2.)
                    .color(INK),
            ])
            .frame(frame)
            .semantic_label(label)
            .on_press(a::Event::focus(FIELD)),
        );
    }
}
fn finish(mut b: Builder, draft: &str, title: &str, form: bool, is_map: bool) -> Result<Rendered> {
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
    let flow = || a::column(0., vec![a::heading().minimum([1., 1.])]);
    let mut description =
        a::AppUi::new("spaces-host", title, schema).with_composition(flow(), flow());
    if form {
        description = description.with_presentation(Presentation::Editor { fields: vec![1] });
    }
    let mut ui = description.build("OTriOS", b.nodes);
    // The authoring helper supplies the ordinary app header. This trusted host
    // owns its minimal map header; retain the hidden composition anchor only.
    let mut blueprint = crate::dashboard::layout::Layout::from_bytes(&ui.layout)?
        .blueprint()
        .clone();
    let root = &mut blueprint.templates[0].root;
    root.children[1].visible = Expr::Literal(Value::Boolean(false));
    root.children[2].visible = Expr::Literal(Value::Boolean(false));
    root.children[0].shape = otrios_interfaces::layout::Shape::Superellipse {
        radius: Expr::Literal(Value::Number(0.)),
        exponent: 4.,
        color: Expr::Literal(Value::Color(PAPER)),
    };
    let controls = ui
        .handlers
        .iter()
        .filter(|h| h.operation == Operation::Back)
        .map(|h| h.control)
        .collect::<Vec<_>>();
    if controls.len() != b.actions.len() {
        return Err("Host action layout mismatch".into());
    }
    let mut map_viewport = None;
    if is_map {
        // AppUi's three fixed header nodes precede our map viewport. A label
        // on a noninteractive authoring canvas is deliberately not compiled.
        let map = &mut blueprint.templates[0].root.children[3];
        map_viewport = Some(map.id);
        map.shape = otrios_interfaces::layout::Shape::Canvas { clip: true };
        fn bodies(node: &mut otrios_interfaces::layout::TemplateNode) {
            if node
                .semantics
                .as_ref()
                .is_some_and(|s| s.label == Expr::Literal(Value::Text("Space body".into())))
            {
                node.semantics = None;
            }
            for child in &mut node.children {
                bodies(child);
            }
        }
        for child in &mut map.children {
            bodies(child);
        }
        // Custom dark backgrounds retain readable titles, labels and controls.
        fn contrast(node: &mut otrios_interfaces::layout::TemplateNode, dark: bool) {
            if dark
                && let otrios_interfaces::layout::Shape::Text {
                    color: Expr::Literal(Value::Color(color)),
                    ..
                } = &mut node.shape
                && (*color == INK || *color == SOFT)
            {
                *color = WHITE;
            }
            for child in &mut node.children {
                contrast(child, dark);
            }
        }
        for card in &mut map.children {
            let dark = card.children.first().is_some_and(|fill| {
                matches!(&fill.shape, otrios_interfaces::layout::Shape::Superellipse { color: Expr::Literal(Value::Color([r,g,b,_])), .. } if u32::from(*r)*299 + u32::from(*g)*587 + u32::from(*b)*114 < 128_000)
            });
            contrast(card, dark);
        }
    }
    ui.layout = crate::dashboard::layout::Layout::encode(&blueprint)?;
    Ok(Rendered {
        ui,
        backdrop: None,
        actions: controls.into_iter().zip(b.actions).collect(),
        map_viewport,
        panel_viewport: None,
    })
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
    let [w, h] = viewport;
    let any_admin = snapshot.spaces.iter().any(|s| s.role == Some(Role::Admin));
    if !any_admin {
        model.editing = false;
    }
    let selected = snapshot.selected.as_ref();
    let summary = selected.and_then(|d| snapshot.spaces.iter().find(|s| s.id == d.space));
    let admin = summary.is_some_and(|s| s.role == Some(Role::Admin));
    if !admin
        && matches!(
            model.pane,
            Pane::SpaceSettings
                | Pane::BackgroundColor
                | Pane::AppSettings(_)
                | Pane::Installed
                | Pane::RemoveApp(_)
                | Pane::Dataset(_)
                | Pane::DeleteDataset(_)
                | Pane::RestoreDataset(_)
                | Pane::DeleteSpace
                | Pane::Datasets
                | Pane::Members
                | Pane::AccessAudience
                | Pane::AudienceConfirm { .. }
                | Pane::Transfer
        )
    {
        model.pane = Pane::Home;
    }
    if snapshot.identity.is_none() && matches!(model.pane, Pane::ProfileEdit | Pane::Create) {
        model.pane = Pane::Home;
    }
    let is_map = matches!(model.pane, Pane::Home | Pane::Spaces);
    if is_map {
        model.map_page = model.page;
        let b = map_body(snapshot, model, sign_in, viewport, font);
        return finish(b, "", "OTriOS", false, true);
    }
    let mut background_model = Model {
        editing: model.editing,
        scroll: model.scroll,
        page: model.map_page,
        ..Model::default()
    };
    let backdrop = map_body(snapshot, &mut background_model, sign_in, viewport, font);
    let mut b = Builder {
        font,
        nodes: vec![],
        actions: vec![],
    };

    let left = if w < 600. { 40. } else { (w - 500.) / 2. };
    let width = (w - left * 2.).min(500.);
    let top = 112.;
    let mut value = draft.to_owned();
    let mut form = false;
    match &model.pane {
        Pane::Transfer => {
            if let Some(entry) = transfers::render(&mut b, snapshot, model, [left, top, width, h]) {
                value = if model
                    .field_owner
                    .as_ref()
                    .is_some_and(|s| s.starts_with("transfer:url:"))
                    && !draft.is_empty()
                {
                    draft.into()
                } else {
                    entry
                };
                form = true;
            }
        }
        Pane::Create => {
            form = true;
            b.nodes.push(text("New Space", [left, top, width, 40.], 2.));
            b.entry("Space name", [left, top + 72., width, 64.]);
            b.flat(
                "Create Space",
                "Add",
                [left + width - 100., top + 176., 100., 56.],
                HostAction::Create,
            );
        }
        Pane::Apps => {
            if let Some(detail) = selected {
                b.nodes.push(text(
                    summary.map_or("Apps", |s| s.name.as_str()),
                    [left, top, width, 40.],
                    2.,
                ));
                let apps = detail
                    .apps
                    .iter()
                    .filter(|a| a.attached)
                    .collect::<Vec<_>>();
                for (i, app) in apps.iter().enumerate() {
                    let tile = width / 2.;
                    b.nodes.push(
                        a::canvas(app_tile(
                            &app.app,
                            &app.title,
                            tile,
                            app.launch.is_some(),
                            font,
                        ))
                        .frame([
                            left + (i % 2) as f32 * tile,
                            top + 64. + (i / 2) as f32 * 152.,
                            tile,
                            144.,
                        ])
                        .semantic_label(&app.title)
                        .on_press(a::Event::back()),
                    );
                    b.actions.push(if let Some(launch) = &app.launch {
                        HostAction::Launch(launch.ui_ref.clone(), launch.state_ref.clone())
                    } else {
                        HostAction::Unavailable("This app is unavailable on this device.")
                    });
                }
            }
        }
        Pane::DeletedSpaces => {
            b.nodes
                .push(text("Deleted Spaces", [left, top, width, 40.], 2.));
            let deleted = snapshot
                .spaces
                .iter()
                .filter(|s| s.archived && s.role == Some(Role::Admin))
                .collect::<Vec<_>>();
            for (i, space) in deleted.iter().enumerate() {
                b.nodes.push(text(
                    &space.name,
                    [left, top + 72. + i as f32 * 64., width - 112., 48.],
                    1.5,
                ));
                b.flat(
                    &format!("Restore Space: {}", space.name),
                    "Restore",
                    [left + width - 104., top + 64. + i as f32 * 64., 104., 56.],
                    HostAction::Manage(Change::RestoreSpace {
                        space: space.id.clone(),
                    }),
                );
            }
            if deleted.is_empty() {
                b.nodes
                    .push(text("Nothing here.", [left, top + 72., width, 40.], 1.5).color(SOFT));
            }
        }
        Pane::Cleanup => {
            b.nodes
                .push(text("Data removal", [left, top, width, 40.], 2.));
            if let Some(cleanup) = selected.and_then(|s| s.cleanup.as_ref()) {
                let notes = [
                    format!("{} deleted collections", cleanup.deleted_collections),
                    format!("{} sessions awaiting disposal", cleanup.local_pending),
                    cleanup.backup_until_unix_ms.map_or_else(
                        || "No managed backup copies retained.".into(),
                        |until| format!("Backups expire {}", deadline(until)),
                    ),
                    if cleanup.legacy_qualification_required {
                        "Imported originals await migration qualification.".into()
                    } else {
                        "Imported originals follow the retention policy.".into()
                    },
                    "Offline copies and user exports cannot be verified.".into(),
                ];
                let mut y = top + 72.;
                for note in notes {
                    for line in caption_lines(&note, width, font) {
                        b.nodes.push(text(&line, [left, y, width, 28.], 1.3));
                        y += 28.;
                    }
                    y += 16.;
                }
                if cleanup.storage_unknown {
                    b.nodes.push(text(
                        "Storage inventory needs operator attention.",
                        [left, y, width, 64.],
                        1.3,
                    ));
                }
            }
        }
        Pane::Members | Pane::AccessAudience | Pane::AudienceConfirm { .. } => {
            if let Some(entry) =
                access::render(&mut b, snapshot, model, draft, [left, top, width, h], font)
            {
                value = entry;
                form = true;
            }
        }
        Pane::BeginMove { .. } | Pane::MoveDestinations | Pane::MoveReview => {
            moves::render(&mut b, snapshot, model, [left, top, width, h], font);
        }
        Pane::Installed => {
            b.nodes.push(text("Add App", [left, top, width, 40.], 2.));
            if let Some(selected) = selected {
                let apps = selected
                    .installed
                    .iter()
                    .filter(|app| {
                        !selected
                            .apps
                            .iter()
                            .any(|a| a.attached && a.app == app.policy.app)
                    })
                    .collect::<Vec<_>>();
                let count = apps.len() + 1;
                for position in 0..count {
                    let index = position;
                    let tile = width / 2.;
                    let frame = [
                        left + (position % 2) as f32 * tile,
                        top + 64. + (position / 2) as f32 * 152.,
                        tile,
                        144.,
                    ];
                    if index == 0 {
                        b.nodes.push(
                            a::canvas(app_tile("upload", "Upload", tile, true, font))
                                .frame(frame)
                                .semantic_label("Upload")
                                .on_press(a::Event::back()),
                        );
                        b.actions.push(HostAction::TransferUpload);
                        continue;
                    }
                    let Some(app) = apps.get(index - 1) else {
                        break;
                    };
                    let compatible = selected
                        .apps
                        .iter()
                        .find(|a| a.app == app.policy.app)
                        .is_none_or(|a| a.availability == AppAvailability::Available);
                    let public = summary
                        .is_some_and(|s| s.audience != otrios_interfaces::spaces::Audience::Closed);
                    let allowed = compatible && !(public && app.policy.closed_only);
                    let node =
                        a::canvas(app_tile(&app.policy.app, &app.title, tile, allowed, font))
                            .frame(frame);
                    if allowed {
                        b.nodes.push(
                            node.semantic_label(&short(&app.title, 94))
                                .on_press(a::Event::back()),
                        );
                        b.actions.push(HostAction::Manage(Change::Attach {
                            space: selected.space.clone(),
                            app: app.policy.app.clone(),
                        }));
                    } else {
                        b.nodes.push(node);
                    }
                }
            }
        }
        Pane::Profile => {
            let name = snapshot
                .profile
                .as_ref()
                .map(|p| p.name.as_str())
                .or(snapshot.identity.as_deref())
                .unwrap_or("Visitor");
            b.nodes.push(text(name, [left, top, width, 40.], 2.));
            if snapshot.identity.as_deref() != Some(name) {
                b.nodes.push(
                    text(
                        snapshot.identity.as_deref().unwrap_or(""),
                        [left, top + 48., width, 32.],
                        1.3,
                    )
                    .color(SOFT),
                );
            }
            if snapshot.identity.is_some() {
                b.flat(
                    "Edit profile",
                    "Edit profile",
                    [left, top + 104., width, 56.],
                    HostAction::Pane(Pane::ProfileEdit),
                );
                b.flat(
                    "Deleted Spaces",
                    "Deleted Spaces",
                    [left, top + 168., width, 56.],
                    HostAction::Pane(Pane::DeletedSpaces),
                );
            }
            if sign_out && snapshot.identity.is_some() {
                b.flat(
                    "Sign out",
                    "Sign out",
                    [left, top + 232., width, 56.],
                    HostAction::SignOut,
                );
            }
        }
        Pane::ProfileEdit => {
            b.nodes.push(text("Profile", [left, top, width, 40.], 2.));
            if let Some(identity) = &snapshot.identity {
                form = true;
                let key = format!("profile:{identity}");
                if model.field_owner.as_ref() != Some(&key) {
                    value = snapshot
                        .profile
                        .as_ref()
                        .map_or_else(|| identity.clone(), |p| p.name.clone());
                    model.field_owner = Some(key);
                    model.form_value = value.clone();
                    model.profile_revision = snapshot.profile.as_ref().map_or(0, |p| p.revision);
                }
                b.entry("Display name", [left, top + 72., width, 64.]);
                b.flat(
                    "Cancel",
                    "Cancel",
                    [left + width - 216., top + 176., 100., 56.],
                    HostAction::Pane(Pane::Profile),
                );
                b.flat(
                    "Save",
                    "Save",
                    [left + width - 100., top + 176., 100., 56.],
                    HostAction::ProfileSave,
                );
            }
        }
        Pane::SpaceSettings => {
            if let Some(space) = summary {
                form = true;
                if model.field_owner.as_deref() != Some(&space.id) {
                    value = space.name.clone();
                    model.field_owner = Some(space.id.clone());
                    model.form_value = value.clone();
                    model.form_generation = snapshot.generation;
                    model.form_blocked = false;
                }
                b.name([left, top, width, 64.]);
                let palette = [
                    Background::None,
                    Background::Sand,
                    Background::Sage,
                    Background::Stone,
                    Background::Sky,
                    Background::Lilac,
                    Background::Rose,
                ];
                let chosen = model.background.unwrap_or(space.layout.background);
                let columns = ((width + 8.) / 56.).floor().clamp(1., 8.) as usize;
                let palette_shift = ((palette.len() + 1).div_ceil(columns) - 1) as f32 * 56.;
                let step = width / columns as f32;
                for (i, c) in palette.into_iter().enumerate() {
                    let label = format!("Background: {c:?}");
                    let x = left + (i % columns) as f32 * step;
                    let y = top + 84. + (i / columns) as f32 * 56.;
                    b.nodes.push(
                        a::rounded_rect(if chosen == c { INK } else { color(c) })
                            .frame([x, y, step - 6., 44.])
                            .corner_radius(14.0.into()),
                    );
                    b.nodes.push(
                        a::rounded_rect(color(c))
                            .frame([x + 3., y + 3., step - 12., 38.])
                            .corner_radius(11.0.into()),
                    );
                    b.nodes.push(
                        a::canvas(vec![text(
                            if c == Background::None { "None" } else { "" },
                            [4., 10., step - 14., 24.],
                            1.,
                        )])
                        .frame([x, y, step - 6., 44.])
                        .semantic_label(&label)
                        .on_press(a::Event::back()),
                    );
                    b.actions.push(HostAction::Background(c));
                }
                let i = palette.len();
                let custom = if matches!(chosen, Background::Rgb(_)) {
                    chosen
                } else {
                    Background::Sage
                };
                let x = left + (i % columns) as f32 * step;
                let y = top + 84. + (i / columns) as f32 * 56.;
                b.nodes.push(
                    a::rounded_rect(color(custom))
                        .frame([x, y, step - 6., 44.])
                        .corner_radius(14.0.into()),
                );
                let [r, g, blue, _] = color(custom);
                let ink =
                    if u32::from(r) * 299 + u32::from(g) * 587 + u32::from(blue) * 114 < 128_000 {
                        WHITE
                    } else {
                        INK
                    };
                let plus_width = font.map_or(18., |f| f.text_width("+", 2.));
                b.nodes.push(
                    a::canvas(vec![
                        text(
                            "+",
                            [
                                ((step - 6. - plus_width) / 2.).max(0.),
                                8.,
                                plus_width + 2.,
                                28.,
                            ],
                            2.,
                        )
                        .color(ink),
                    ])
                    .frame([x, y, step - 6., 44.])
                    .semantic_label("Custom background color")
                    .on_press(a::Event::back()),
                );
                b.actions.push(HostAction::Pane(Pane::BackgroundColor));
                let used = selected.and_then(|s| {
                    s.apps
                        .iter()
                        .map(|a| (&a.collection, a.stored_bytes))
                        .chain(s.retained.iter().map(|d| (&d.collection, d.stored_bytes)))
                        .collect::<BTreeMap<_, _>>()
                        .values()
                        .try_fold(0u64, |sum, bytes| sum.checked_add((*bytes)?))
                });
                b.stats(
                    used,
                    selected.and_then(|s| s.active_users),
                    [left, top + palette_shift + 156., width, 76.],
                );
                b.info(
                    "Access",
                    match space.audience {
                        otrios_interfaces::spaces::Audience::Closed => "Private",
                        otrios_interfaces::spaces::Audience::PublicView => "Public · Read only",
                        otrios_interfaces::spaces::Audience::PublicComment => {
                            "Public · Signed-in comments"
                        }
                        otrios_interfaces::spaces::Audience::PublicEdit => {
                            "Public · Anyone can edit"
                        }
                    },
                    [left, top + palette_shift + 260., width, 40.],
                );
                b.info(
                    "Apps",
                    &space.attached_count.to_string(),
                    [left, top + palette_shift + 310., width, 32.],
                );
                let count = selected.map_or(0, |s| s.retained.len());
                if count > 0 {
                    b.info(
                        "Retained datasets",
                        &count.to_string(),
                        [left, top + palette_shift + 356., width, 32.],
                    );
                }
                b.divider([left, top + palette_shift + 412., width, 1.]);
                b.flat(
                    "People & access",
                    "People & access",
                    [left, top + palette_shift + 440., width, 50.],
                    HostAction::Pane(Pane::Members),
                );
                if space.archived {
                    b.flat(
                        "Restore Space",
                        "Restore Space",
                        [left, top + palette_shift + 496., width, 50.],
                        HostAction::Manage(Change::RestoreSpace {
                            space: space.id.clone(),
                        }),
                    );
                } else {
                    b.flat(
                        "Delete Space",
                        "Delete Space",
                        [left, top + palette_shift + 496., width, 50.],
                        HostAction::Pane(Pane::DeleteSpace),
                    );
                }
            }
        }
        Pane::BackgroundColor => {
            if let Some(space) = summary {
                form = true;
                let key = format!("color:{}", space.id);
                if model.field_owner.as_deref() != Some(&key) {
                    let [r, g, b, _] = color(space.layout.background);
                    value = format!("#{r:02X}{g:02X}{b:02X}");
                    model.form_value = value.clone();
                    model.field_owner = Some(key);
                    model.form_generation = snapshot.generation;
                    model.form_blocked = false;
                }
                b.nodes
                    .push(text("Background color", [left, top, width, 40.], 2.));
                b.entry("Hex color", [left, top + 72., width, 64.]);
                b.flat(
                    "Cancel",
                    "Cancel",
                    [left + width - 216., top + 176., 100., 56.],
                    HostAction::Pane(Pane::SpaceSettings),
                );
                b.flat(
                    "Apply color",
                    "Apply",
                    [left + width - 100., top + 176., 100., 56.],
                    HostAction::BackgroundApply,
                );
            }
        }
        Pane::AppSettings(id) => {
            if let Some(app) = selected.and_then(|s| s.apps.iter().find(|a| &a.app == id)) {
                form = true;
                let key = format!("{}:{}", summary.unwrap().id, id);
                if model.field_owner.as_deref() != Some(&key) {
                    value = app.title.clone();
                    model.field_owner = Some(key);
                    model.form_value = value.clone();
                    model.form_generation = snapshot.generation;
                    model.form_blocked = false;
                }
                b.name([left, top, width, 64.]);
                b.nodes.push(
                    text(&app.app, [left + 8., top + 74., width - 16., 32.], 1.3).color(SOFT),
                );
                b.stats(
                    app.stored_bytes,
                    app.active_users,
                    [left, top + 132., width, 76.],
                );
                let code =
                    selected.and_then(|s| s.installed.iter().find(|p| p.policy.app == app.app));
                b.info(
                    "App size",
                    &code.and_then(|p| p.code_bytes).map_or_else(
                        || "Unavailable".into(),
                        |n| stored(Some(n)).replace(" stored", ""),
                    ),
                    [left, top + 244., width, 32.],
                );
                b.info(
                    "Revision",
                    &code
                        .and_then(|p| p.revision)
                        .map_or_else(|| "Unavailable".into(), |n| n.to_string()),
                    [left, top + 286., width, 32.],
                );
                b.info(
                    "Space",
                    &summary.unwrap().name,
                    [left, top + 328., width, 40.],
                );
                b.divider([left, top + 386., width, 1.]);
                for (i, (label, action)) in [
                    ("Share App", HostAction::TransferShare(app.app.clone())),
                    (
                        "Export Data",
                        HostAction::TransferExport(app.app.clone(), app.collection.clone()),
                    ),
                    (
                        "Replace Data",
                        HostAction::TransferReplace(app.app.clone(), app.collection.clone()),
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    b.flat(
                        label,
                        label,
                        [left, top + 408. + i as f32 * 60., width, 56.],
                        action,
                    );
                }
            }
        }
        _ => {
            if let Pane::Dataset(cid) = &model.pane
                && let Some(data) =
                    selected.and_then(|s| s.retained.iter().find(|d| d.collection == *cid))
            {
                form = true;
                let key = format!("{}:dataset:{cid}", selected.unwrap().space);
                if model.field_owner.as_ref() != Some(&key) {
                    value = data.title.clone();
                    model.field_owner = Some(key);
                    model.form_value = value.clone();
                    model.form_generation = snapshot.generation;
                    model.form_blocked = false;
                }
            }
            retained::render(&mut b, snapshot, model, [left, top, width, h], font);
        }
    }
    if !model.message.is_empty() {
        for (line, caption) in caption_lines(&model.message, width, font)
            .iter()
            .enumerate()
        {
            b.nodes.push(
                text(
                    caption,
                    [
                        left,
                        (h - 152.).min(top + 464.) + line as f32 * 28.,
                        width,
                        28.,
                    ],
                    2.,
                )
                .color([187, 57, 51, 255]),
            );
        }
    }
    if !retained::is_confirmation(&model.pane)
        && !matches!(
            model.pane,
            Pane::ProfileEdit
                | Pane::BackgroundColor
                | Pane::Members
                | Pane::AccessAudience
                | Pane::AudienceConfirm { .. }
                | Pane::BeginMove { .. }
                | Pane::MoveDestinations
                | Pane::MoveReview
                | Pane::Transfer
        )
    {
        b.flat(
            "Close",
            "Close",
            [left, (h - 88.).min(top + 544.), 100., 56.],
            HostAction::Pane(Pane::Home),
        );
    }
    panel::finish(
        backdrop,
        b,
        &value,
        form,
        (viewport, [left - 24., top - 24.]),
        background_model.scroll,
        &caption_lines(&model.message, width, font),
    )
}

fn map_body<'a>(
    snapshot: &Discovery,
    model: &mut Model,
    sign_in: bool,
    viewport: [f32; 2],
    font: Option<&'a crate::dashboard::font::Font>,
) -> Builder<'a> {
    let [w, h] = viewport;
    let any_admin = snapshot.spaces.iter().any(|s| s.role == Some(Role::Admin));
    let mut b = Builder {
        font,
        nodes: vec![],
        actions: vec![],
    };
    let mut content = Builder {
        font,
        nodes: vec![],
        actions: vec![],
    };
    let shown = snapshot
        .spaces
        .iter()
        .filter(|s| {
            !s.archived && (s.attached_count > 0 || model.editing && s.role == Some(Role::Admin))
        })
        .collect::<Vec<_>>();
    let margin = if w < 500. { 16. } else { 28. };
    let gap = 20.;
    let available = (w - margin * 2.).max(1.);
    let cols = ((available + gap) / 160.).floor().clamp(1., 6.) as usize;
    // Six Spaces plus the existing 16-tile budget keep worst-case tall
    // layouts inside scene/control/coordinate limits, including one column.
    // Ordinary maps scroll together instead of paging at four Spaces.
    let page_size = PAGE;
    let mut pages = vec![vec![]];
    let mut tiles = 0;
    for space in &shown {
        let count = (space.apps.len()
            + if model.editing {
                space.retained.len()
            } else {
                0
            })
        .clamp(1, 16);
        if pages
            .last()
            .is_some_and(|page| !page.is_empty() && (page.len() == page_size || tiles + count > 16))
        {
            pages.push(vec![]);
            tiles = 0;
        }
        pages.last_mut().unwrap().push(*space);
        tiles += count;
    }
    let foot = if pages.len() > 1 { 136. } else { FOOT };
    model.page = model.page.min(pages.len() - 1);
    let cell = (available - gap * (cols - 1) as f32) / cols as f32;
    let mut positions = vec![];
    let mut skyline = vec![0.; cols];
    for space in &pages[model.page] {
        let width = usize::from(space.layout.width).min(cols);
        let count = (space.apps.len()
            + if model.editing {
                space.retained.len()
            } else {
                0
            })
        .min(otrios_interfaces::spaces::MAX_INSTALLED_APPS);
        let rows = count
            .max(1)
            .div_ceil(width)
            .max(usize::from(space.layout.height));
        let height = 68.
            + rows as f32 * 152.
            + if model.editing && space.role == Some(Role::Admin) {
                60.
            } else {
                16.
            };
        let (column, y) = (0..=cols - width)
            .map(|c| (c, skyline[c..c + width].iter().copied().fold(0., f32::max)))
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .unwrap();
        for v in &mut skyline[column..column + width] {
            *v = y + height + gap;
        }
        positions.push((
            *space,
            [
                margin + column as f32 * (cell + gap),
                y,
                cell * width as f32 + gap * (width - 1) as f32,
                height,
            ],
            width,
        ));
    }
    let total = skyline.into_iter().fold(0., f32::max);
    // The fixed controls float over the map. Padding belongs to the scrolling
    // content, so its final item can clear the controls at the end of a scroll.
    model.scroll_limit = (total + foot - (h - TOP)).max(0.);
    model.scroll = model.scroll.clamp(0., model.scroll_limit);
    for (space, frame, columns) in positions {
        let [x, y, width, height] = frame;
        let editable = model.editing && space.role == Some(Role::Admin);
        let mut card = Builder {
            font,
            nodes: vec![],
            actions: vec![],
        };
        card.nodes.push(
            a::rounded_rect(color(space.layout.background))
                .frame([0., 0., width, height])
                .corner_radius(24.0.into()),
        );
        let title_width = width - if editable { 64. } else { 16. };
        let displayed_name = if space.archived {
            format!("{} (deleted)", space.name)
        } else {
            space.name.clone()
        };
        let title_lines = caption_lines(&displayed_name, title_width - 20., font);
        let title_nodes = title_lines
            .iter()
            .enumerate()
            .map(|(line, value)| {
                text(
                    value,
                    [
                        10.,
                        if title_lines.len() == 1 {
                            12.
                        } else {
                            line as f32 * 28.
                        },
                        title_width - 20.,
                        28.,
                    ],
                    2.,
                )
            })
            .collect();
        card.nodes.push(
            a::canvas(title_nodes)
                .frame([8., 4., title_width, 56.])
                .semantic_label(&short(&format!("Space: {}", space.name), 94))
                .on_press(a::Event::back()),
        );
        card.actions.push(HostAction::MapSpace(space.id.clone()));
        if editable {
            card.flat(
                &format!("Space settings: {}", space.name),
                "•••",
                [width - 56., 4., 48., 48.],
                HostAction::Settings(space.id.clone(), Pane::SpaceSettings),
            );
        }
        let tile = (width - 24.) / columns as f32;
        for (index, app) in space.apps.iter().enumerate() {
            let tx = 12. + (index % columns) as f32 * tile;
            let ty = 64. + (index / columns) as f32 * 152.;
            let label = if snapshot
                .spaces
                .iter()
                .flat_map(|s| &s.apps)
                .filter(|a| a.title == app.title)
                .take(2)
                .count()
                > 1
            {
                format!("{} in {}", app.title, space.name)
            } else {
                app.title.clone()
            };
            let tile_nodes = app_tile(
                &app.app,
                &app.title,
                tile,
                app.availability == AppAvailability::Available,
                font,
            );
            card.nodes.push(
                a::canvas(tile_nodes)
                    .frame([tx, ty, tile, 144.])
                    .semantic_label(&short(&label, 94))
                    .on_press(a::Event::back()),
            );
            card.actions
                .push(HostAction::MapApp(space.id.clone(), app.app.clone()));
            if editable {
                card.minus(
                    &format!("Delete App: {} in {}", app.title, space.name),
                    [tx + tile - 44., ty - 8., 44., 44.],
                    HostAction::Settings(space.id.clone(), Pane::RemoveApp(app.app.clone())),
                );
            }
        }
        if editable {
            for (offset, data) in space
                .retained
                .iter()
                .take(
                    otrios_interfaces::spaces::MAX_INSTALLED_APPS.saturating_sub(space.apps.len()),
                )
                .enumerate()
            {
                let index = space.apps.len() + offset;
                let tx = 12. + (index % columns) as f32 * tile;
                let ty = 64. + (index / columns) as f32 * 152.;
                let title = format!("{} (data)", data.title);
                card.nodes.push(
                    a::canvas(retained::ghost_tile(&data.app, &title, tile, font))
                        .frame([tx, ty, tile, 144.])
                        .semantic_label(&short(&format!("{} in {}", title, space.name), 94))
                        .on_press(a::Event::back()),
                );
                card.actions.push(HostAction::MapDataset(
                    space.id.clone(),
                    data.app.clone(),
                    data.collection.clone(),
                ));
                card.minus(
                    &format!("Delete data: {} in {}", data.title, space.name),
                    [tx + tile - 44., ty - 8., 44., 44.],
                    HostAction::Settings(
                        space.id.clone(),
                        Pane::DeleteDataset(data.collection.clone()),
                    ),
                );
            }
            if !space.retained.is_empty() {
                card.flat(
                    "All retained data",
                    "Data…",
                    [72., height - 56., (width - 128.).max(48.), 48.],
                    HostAction::Settings(space.id.clone(), Pane::Datasets),
                );
            }
        }
        if editable {
            if !space.archived {
                card.flat(
                    &format!("Add app: {}", space.name),
                    "+",
                    [12., height - 56., 48., 48.],
                    HostAction::Settings(space.id.clone(), Pane::Installed),
                );
            }
            card.flat(
                &format!("Resize: {}", space.name),
                "",
                [width - 48., height - 48., 44., 44.],
                HostAction::Resize(space.id.clone()),
            );
            card.nodes
                .push(a::rectangle(SOFT).frame([width - 18., height - 30., 2., 14.]));
            card.nodes
                .push(a::rectangle(SOFT).frame([width - 30., height - 18., 14., 2.]));
        }
        // A clipped map viewport keeps scrolled tiles clear of the header/footer.
        content.nodes.push(
            a::canvas(card.nodes)
                .frame([x, y, width, height])
                .semantic_label("Space body")
                .on_press(a::Event::back()),
        );
        content
            .actions
            .push(HostAction::SpaceBody(space.id.clone()));
        content.actions.extend(card.actions);
    }
    b.nodes.push(
        a::canvas(content.nodes)
            .frame([0., TOP, w, (h - TOP).max(1.)])
            .semantic_label("Spaces map"),
    );
    b.actions.extend(content.actions);
    // Only the header masks content. Footer controls float above the edge fade.
    b.nodes.push(a::rectangle(PAPER).frame([0., 0., w, TOP]));
    b.nodes.push(text("OTriOS", [28., 26., 168., 42.], 3.));
    // Profile is appended again here so overflow masking cannot hide it.
    if let Some(identity) = &snapshot.identity {
        let avatar = snapshot
            .profile
            .as_ref()
            .map_or(identity.as_str(), |p| p.name.as_str())
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        b.profile(&avatar, w, font);
    } else if sign_in {
        b.flat(
            "Sign in",
            "Sign in",
            [w - 136., 22., 112., 52.],
            HostAction::SignIn,
        );
    }
    if shown.is_empty() {
        b.nodes.push(text(
            "No apps yet.",
            [margin, TOP + 20., available, 40.],
            2.,
        ));
    }
    if model.page > 0 {
        b.flat(
            "Previous",
            "<",
            [margin, h - 124., 48., 48.],
            HostAction::Page(-1),
        );
    }
    if model.page + 1 < pages.len() {
        b.flat(
            "Next",
            ">",
            [margin + 56., h - 124., 48., 48.],
            HostAction::Page(1),
        );
    }
    if model.editing {
        b.flat(
            "Add Space",
            "+ Space",
            [margin, h - 68., 112., 48.],
            HostAction::Pane(Pane::Create),
        );
    }
    if any_admin {
        b.flat(
            if model.editing { "Done" } else { "Edit" },
            if model.editing { "Done" } else { "Edit" },
            [w - 100., h - 68., 80., 48.],
            HostAction::Edit,
        );
    }
    if !model.message.is_empty() {
        b.nodes.push(text(
            &model.message,
            [margin, h - 108., (available - 100.).max(1.), 32.],
            1.5,
        ));
    }
    b
}
