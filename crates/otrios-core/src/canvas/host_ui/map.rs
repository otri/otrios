// SPDX-License-Identifier: LGPL-3.0-only
//! Local map presentation. Only admitted authority metadata supplies its content.
use super::*;
use otrios_interfaces::{
    layout::Expr,
    spaces::{Background, Discovery, Role},
};
mod retained;
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
impl Builder {
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
        self.nodes.push(
            a::canvas(vec![text(
                caption,
                [10., 12., (frame[2] - 20.).max(1.), 32.],
                2.,
            )])
            .frame(frame)
            .semantic_label(&short(label, 94))
            .on_press(a::Event::back()),
        );
        self.actions.push(action);
    }
    fn name(&mut self, frame: [f32; 4]) {
        self.nodes.push(
            a::canvas(vec![
                a::surface(WHITE, 12.).pin_edges(0.),
                a::text(FIELD.value())
                    .frame([14., 15., frame[2] - 28., 40.])
                    .font_scale(2.)
                    .color(INK),
            ])
            .frame(frame)
            .semantic_label("Name")
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
    }
    ui.layout = crate::dashboard::layout::Layout::encode(&blueprint)?;
    Ok(Rendered {
        ui,
        actions: controls.into_iter().zip(b.actions).collect(),
        map_viewport,
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
                | Pane::AppSettings(_)
                | Pane::Installed
                | Pane::RemoveApp(_)
                | Pane::Dataset(_)
                | Pane::DeleteDataset(_)
                | Pane::RestoreDataset(_)
                | Pane::DeleteSpace
                | Pane::Datasets
        )
    {
        model.pane = Pane::Home;
    }
    let mut b = Builder {
        nodes: vec![],
        actions: vec![],
    };
    b.nodes.push(text("OTriOS", [28., 26., 168., 42.], 3.));
    let avatar = snapshot
        .identity
        .as_ref()
        .and_then(|s| s.chars().next())
        .map(|s| s.to_uppercase().to_string());
    if let Some(avatar) = avatar {
        b.profile(&avatar, w, font);
    } else if sign_in {
        b.flat(
            "Sign in",
            "Sign in",
            [w - 136., 22., 112., 52.],
            HostAction::SignIn,
        );
    }
    let is_map = matches!(model.pane, Pane::Home | Pane::Spaces);
    if is_map {
        b.nodes.clear();
        b.actions.clear();
        let mut content = Builder {
            nodes: vec![],
            actions: vec![],
        };
        let shown = snapshot
            .spaces
            .iter()
            .filter(|s| {
                !s.archived && s.attached_count > 0 || model.editing && s.role == Some(Role::Admin)
            })
            .collect::<Vec<_>>();
        let margin = if w < 500. { 16. } else { 28. };
        let gap = 20.;
        let available = (w - margin * 2.).max(1.);
        let cols = ((available + gap) / 160.).floor().clamp(1., 6.) as usize;
        // Keep even four maximally tall previews within the layout coordinate
        // budget; a one-column viewport uses three Spaces per page.
        let page_size = if cols == 1 { 3 } else { PAGE };
        let foot = if shown.len() > page_size { 136. } else { FOOT };
        model.page = model.page.min(shown.len().saturating_sub(1) / page_size);
        let cell = (available - gap * (cols - 1) as f32) / cols as f32;
        let mut positions = vec![];
        let mut skyline = vec![0.; cols];
        for space in shown.iter().skip(model.page * page_size).take(page_size) {
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
        model.scroll_limit = (total - (h - TOP - foot)).max(0.);
        model.scroll = model.scroll.clamp(0., model.scroll_limit);
        for (space, frame, columns) in positions {
            let [x, y, width, height] = frame;
            let editable = model.editing && space.role == Some(Role::Admin);
            let mut card = Builder {
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
                        otrios_interfaces::spaces::MAX_INSTALLED_APPS
                            .saturating_sub(space.apps.len()),
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
                    card.actions.push(HostAction::Settings(
                        space.id.clone(),
                        Pane::Dataset(data.collection.clone()),
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
                .frame([0., TOP, w, (h - TOP - foot).max(1.)])
                .semantic_label("Spaces map"),
        );
        b.actions.extend(content.actions);
        // Fixed header and footer stay clear of the clipped map content.
        b.nodes.push(a::rectangle(PAPER).frame([0., 0., w, TOP]));
        b.nodes.push(text("OTriOS", [28., 26., 168., 42.], 3.));
        b.nodes
            .push(a::rectangle(PAPER).frame([0., h - foot, w, foot]));
        // Profile is appended again here so overflow masking cannot hide it.
        if let Some(identity) = &snapshot.identity {
            let avatar = identity
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
                "‹",
                [margin, h - 124., 48., 48.],
                HostAction::Page(-1),
            );
        }
        if (model.page + 1) * page_size < shown.len() {
            b.flat(
                "Next",
                "›",
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
        return finish(b, "", "OTriOS", false, true);
    }
    let left = if w < 600. { 20. } else { (w - 500.) / 2. };
    let width = (w - left * 2.).min(500.);
    let top = 112.;
    let mut value = draft.to_owned();
    let mut form = false;
    match &model.pane {
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
                    let tile = width / 2.;
                    let node =
                        a::canvas(app_tile(&app.policy.app, &app.title, tile, allowed, font))
                            .frame([
                                left + (index % 2) as f32 * tile,
                                top + 64. + (index / 2) as f32 * 152.,
                                tile,
                                144.,
                            ]);
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
                if apps.is_empty() {
                    b.nodes.push(text(
                        "All installed apps are attached.",
                        [left, top + 72., width, 64.],
                        2.,
                    ));
                }
                if model.page > 0 {
                    b.flat(
                        "Previous",
                        "‹",
                        [left, top + 380., 48., 48.],
                        HostAction::Page(-1),
                    );
                }
                if (model.page + 1) * PAGE < apps.len() {
                    b.flat(
                        "Next",
                        "›",
                        [left + 56., top + 380., 48., 48.],
                        HostAction::Page(1),
                    );
                }
            }
        }
        Pane::Profile => {
            b.nodes.push(text(
                snapshot.identity.as_deref().unwrap_or("Visitor"),
                [left, top, width, 40.],
                2.,
            ));
            if sign_out && snapshot.identity.is_some() {
                b.flat(
                    "Sign out",
                    "Sign out",
                    [left, top + 72., width, 56.],
                    HostAction::SignOut,
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
                let step = width / 7.;
                for (i, c) in palette.into_iter().enumerate() {
                    let label = format!("Background: {c:?}");
                    let x = left + i as f32 * step;
                    b.nodes.push(
                        a::rounded_rect(if chosen == c { INK } else { color(c) })
                            .frame([x, top + 84., step - 6., 44.])
                            .corner_radius(14.0.into()),
                    );
                    b.nodes.push(
                        a::rounded_rect(color(c))
                            .frame([x + 3., top + 87., step - 12., 38.])
                            .corner_radius(11.0.into()),
                    );
                    b.nodes.push(
                        a::canvas(vec![text(
                            if c == Background::None { "None" } else { "" },
                            [4., 10., step - 14., 24.],
                            1.,
                        )])
                        .frame([x, top + 84., step - 6., 44.])
                        .semantic_label(&label)
                        .on_press(a::Event::back()),
                    );
                    b.actions.push(HostAction::Background(c));
                }
                let used = selected.and_then(|s| {
                    s.apps
                        .iter()
                        .map(|a| (&a.collection, a.stored_bytes))
                        .chain(s.retained.iter().map(|d| (&d.collection, d.stored_bytes)))
                        .collect::<BTreeMap<_, _>>()
                        .values()
                        .try_fold(0u64, |sum, bytes| sum.checked_add((*bytes)?))
                });
                b.nodes.push(text(
                    &format!(
                        "Data used   {}",
                        used.map_or_else(
                            || "Unavailable".into(),
                            |v| stored(Some(v)).replace(" stored", "")
                        )
                    ),
                    [left, top + 156., width, 32.],
                    1.5,
                ));
                b.nodes.push(text(
                    "Active users   Unavailable",
                    [left, top + 194., width, 32.],
                    1.5,
                ));
                for (i, (label, pane)) in [
                    ("People & access", Pane::Members),
                    ("Sharing", Pane::Sharing),
                    ("Apps & Data", Pane::Data),
                    ("Cleanup", Pane::Cleanup),
                ]
                .into_iter()
                .enumerate()
                {
                    b.flat(
                        label,
                        label,
                        [left, top + 244. + i as f32 * 54., width, 50.],
                        HostAction::Pane(pane),
                    );
                }
                if space.archived {
                    b.flat(
                        "Restore Space",
                        "Restore Space",
                        [left, top + 464., width, 50.],
                        HostAction::Manage(Change::RestoreSpace {
                            space: space.id.clone(),
                        }),
                    );
                } else {
                    b.flat(
                        "Delete Space",
                        "Delete Space",
                        [left, top + 464., width, 50.],
                        HostAction::Pane(Pane::DeleteSpace),
                    );
                }
                b.flat(
                    "Save",
                    "Save",
                    [left + width - 100., (h - 88.).min(top + 544.), 100., 56.],
                    HostAction::SaveSpace,
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
                b.nodes.push(text(
                    &format!(
                        "Data used   {}",
                        stored(app.stored_bytes).replace(" stored", "")
                    ),
                    [left, top + 132., width, 32.],
                    1.5,
                ));
                b.nodes.push(text(
                    "Active users   Unavailable",
                    [left, top + 172., width, 32.],
                    1.5,
                ));
                for (i, (label, message)) in [
                    (
                        "Share App",
                        "App sharing is not available on this host yet.",
                    ),
                    (
                        "Export Data",
                        "Data export is not available for this app yet.",
                    ),
                    (
                        "Replace Data",
                        "Data replacement is not available for this app yet.",
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    b.flat(
                        label,
                        label,
                        [left, top + 232. + i as f32 * 60., width, 56.],
                        HostAction::Unavailable(message),
                    );
                }
            }
        }
        _ => {
            retained::render(&mut b, snapshot, model, [left, top, width, h], font);
        }
    }
    if !model.message.is_empty() {
        for (line, caption) in caption_lines(&model.message, width, font)
            .iter()
            .enumerate()
        {
            b.nodes.push(text(
                caption,
                [
                    left,
                    (h - 152.).min(top + 464.) + line as f32 * 28.,
                    width,
                    28.,
                ],
                2.,
            ));
        }
    }
    if !retained::is_confirmation(&model.pane) {
        b.flat(
            "Close",
            "Close",
            [left, (h - 88.).min(top + 544.), 100., 56.],
            HostAction::Pane(Pane::Home),
        );
    }
    finish(b, &value, "OTriOS", form, false)
}
