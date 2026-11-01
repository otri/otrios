// SPDX-License-Identifier: LGPL-3.0-only
//! Local map presentation. Only admitted authority metadata supplies its content.
use super::*;
use otrios_interfaces::{
    layout::Expr,
    spaces::{Background, Discovery, Role},
};
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
    pub scroll: f32,
    pub moving: bool,
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
impl Builder {
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
fn finish(
    mut b: Builder,
    draft: &str,
    title: &str,
    form: bool,
    viewport: [f32; 2],
) -> Result<Rendered> {
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
    ui.layout = crate::dashboard::layout::Layout::encode(&blueprint)?;
    let controls = ui
        .handlers
        .iter()
        .filter(|h| h.operation == Operation::Back)
        .map(|h| h.control)
        .collect::<Vec<_>>();
    if controls.len() != b.actions.len() {
        return Err("Host action layout mismatch".into());
    }
    fn clip_map(node: &mut otrios_interfaces::layout::TemplateNode, viewport: [f32; 2]) {
        if node
            .semantics
            .as_ref()
            .is_some_and(|s| s.label == Expr::Literal(Value::Text("Spaces map".into())))
        {
            node.shape = otrios_interfaces::layout::Shape::Canvas { clip: true };
            node.semantics = None;
            fn responders(
                node: &mut otrios_interfaces::layout::TemplateNode,
                parent_y: f32,
                bottom: f32,
            ) {
                let literal = |expr: &Expr| {
                    if let Expr::Literal(Value::Number(n)) = expr {
                        *n
                    } else {
                        0.
                    }
                };
                let y = parent_y + literal(&node.frame[1]);
                let height = literal(&node.frame[3]);
                let body = node
                    .semantics
                    .as_ref()
                    .is_some_and(|s| s.label == Expr::Literal(Value::Text("Space body".into())));
                if body {
                    node.semantics = None;
                }
                if !body && node.responder.is_some() && (y < 0. || y + height > bottom) {
                    node.responder = None;
                    node.semantics = None;
                }
                for child in &mut node.children {
                    responders(child, y, bottom);
                }
            }
            let bottom = if let Expr::Literal(Value::Number(height)) = node.frame[3] {
                height
            } else {
                (viewport[1] - TOP - FOOT).max(1.)
            };
            for child in &mut node.children {
                responders(child, 0., bottom);
            }
        }
        for child in &mut node.children {
            clip_map(child, viewport);
        }
    }
    clip_map(&mut blueprint.templates[0].root, viewport);
    ui.layout = crate::dashboard::layout::Layout::encode(&blueprint)?;
    Ok(Rendered {
        ui,
        actions: controls.into_iter().zip(b.actions).collect(),
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
    if !admin && matches!(model.pane, Pane::SpaceSettings | Pane::AppSettings(_)) {
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
        b.flat(
            "Profile",
            &avatar,
            [w - 76., 22., 52., 52.],
            HostAction::Pane(Pane::Profile),
        );
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
            .filter(|s| s.attached_count > 0 || model.editing && s.role == Some(Role::Admin))
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
            let count = space.apps.len();
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
        for (space, mut frame, columns) in positions {
            frame[1] -= model.scroll;
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
            let title_lines = caption_lines(&space.name, title_width - 20., font);
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
                let mark = app
                    .title
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                let icon = if app.availability == AppAvailability::Available {
                    WHITE
                } else {
                    [220, 221, 213, 255]
                };
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
                let mut tile_nodes = vec![
                    a::rounded_rect(icon)
                        .frame([(tile - 72.) / 2., 0., 72., 72.])
                        .corner_radius(20.0.into()),
                    text(&mark, [(tile - 30.) / 2., 22., 30., 40.], 2.).color(SOFT),
                ];
                for (line, caption) in caption_lines(&app.title, tile - 8., font)
                    .iter()
                    .enumerate()
                {
                    tile_nodes.push(text(
                        caption,
                        [4., 84. + line as f32 * 28., tile - 8., 28.],
                        2.,
                    ));
                }
                card.nodes.push(
                    a::canvas(tile_nodes)
                        .frame([tx, ty, tile, 144.])
                        .semantic_label(&short(&label, 94))
                        .on_press(a::Event::back()),
                );
                card.actions
                    .push(HostAction::MapApp(space.id.clone(), app.app.clone()));
                if editable {
                    card.nodes
                        .push(text("···", [tx + tile - 32., ty + 4., 28., 24.], 1.).color(SOFT));
                }
            }
            if editable {
                card.flat(
                    &format!("Add app: {}", space.name),
                    "+",
                    [12., height - 56., 48., 48.],
                    HostAction::Settings(space.id.clone(), Pane::Installed),
                );
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
            b.flat(
                "Profile",
                &avatar,
                [w - 76., 22., 52., 52.],
                HostAction::Pane(Pane::Profile),
            );
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
        if model.scroll_limit > 0. {
            if model.scroll > 0. {
                b.flat(
                    "Scroll up",
                    "^",
                    [w / 2. - 24., h - 68., 36., 48.],
                    HostAction::Scroll(-1),
                );
            }
            if model.scroll < model.scroll_limit {
                b.flat(
                    "Scroll down",
                    "v",
                    [w / 2. + 16., h - 68., 36., 48.],
                    HostAction::Scroll(1),
                );
            }
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
        if let Some(drag) = model
            .drag
            .as_ref()
            .filter(|d| d.moving && !matches!(d.kind, DragKind::Pan))
        {
            let caption = match &drag.kind {
                DragKind::Space(id) => snapshot
                    .spaces
                    .iter()
                    .find(|s| &s.id == id)
                    .map_or("Space", |s| s.name.as_str()),
                DragKind::App(s, id) => snapshot
                    .spaces
                    .iter()
                    .find(|v| &v.id == s)
                    .and_then(|s| s.apps.iter().find(|a| &a.app == id))
                    .map_or("App", |a| a.title.as_str()),
                DragKind::Resize(_) => "Resize Space",
                DragKind::Pan => "",
            };
            let x = (drag.position[0] - 90.).clamp(0., (w - 180.).max(0.));
            let y = if drag.position[1] + 76. < h - FOOT {
                drag.position[1] + 20.
            } else {
                drag.position[1] - 76.
            };
            b.nodes.push(
                a::canvas(vec![
                    a::surface(WHITE, 14.).pin_edges(0.),
                    text(&short(caption, 24), [12., 14., 156., 32.], 2.),
                ])
                .frame([x, y, 180., 56.]),
            );
        }
        return finish(b, "", "OTriOS", false, viewport);
    }
    let left = if w < 600. { 20. } else { (w - 500.) / 2. };
    let width = (w - left * 2.).min(500.);
    let top = 112.;
    let mut value = draft.to_owned();
    let mut form = false;
    match &model.pane {
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
                        a::canvas(vec![
                            text(
                                if c == Background::None {
                                    "None"
                                } else if chosen == c {
                                    "•"
                                } else {
                                    ""
                                },
                                [4., 10., step - 14., 24.],
                                1.,
                            )
                            .color(if chosen == c {
                                WHITE
                            } else {
                                INK
                            }),
                        ])
                        .frame([x, top + 84., step - 6., 44.])
                        .semantic_label(&label)
                        .on_press(a::Event::back()),
                    );
                    b.actions.push(HostAction::Background(c));
                }
                let used = selected.and_then(|s| {
                    s.apps
                        .iter()
                        .try_fold(0u64, |sum, a| sum.checked_add(a.stored_bytes?))
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
        _ => {}
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
    b.flat(
        "Close",
        "Close",
        [left, (h - 88.).min(top + 544.), 100., 56.],
        HostAction::Pane(Pane::Home),
    );
    finish(b, &value, "OTriOS", form, viewport)
}
