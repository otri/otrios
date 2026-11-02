// SPDX-License-Identifier: LGPL-3.0-only
//! Shared modal surface. The map remains visible but never admits input behind it.
use super::*;
use otrios_interfaces::layout::Shape;
use otrios_interfaces::scene::SceneNode;

fn number(value: &Expr) -> Result<f32> {
    match value {
        Expr::Literal(Value::Number(n)) => Ok(*n),
        _ => Err("Panel geometry must be local".into()),
    }
}
fn disable(node: &mut SceneNode) {
    node.id += 2_000_000_000;
    node.responder = None;
    node.semantics = None;
    for child in &mut node.children {
        disable(child);
    }
}
pub(super) fn finish(
    backdrop: Builder,
    panel: Builder,
    draft: &str,
    form: bool,
    geometry: ([f32; 2], [f32; 2]),
    map_scroll: f32,
    messages: &[String],
) -> Result<Rendered> {
    let ([w, h], origin) = geometry;
    let width = w - origin[0] * 2.;
    // A populated map and the software keyboard each fit their own bounded
    // layout. Compose the read-only scene instead of serializing both into one
    // app layout (which would exceed the 128 KiB admission limit).
    let background = super::finish(backdrop, "", "OTriOS", false, true)?;
    let layout = crate::dashboard::layout::Layout::from_bytes(&background.ui.layout)?;
    let mut backdrop = layout
        .resolve_viewport(&BTreeMap::new(), [w, h])?
        .root()
        .clone();
    if let Some(map) = backdrop.children.get_mut(3) {
        for child in &mut map.children {
            child.frame.y -= map_scroll;
        }
    }
    disable(&mut backdrop);
    let b = Builder {
        font: panel.font,
        nodes: vec![
            a::rectangle([44, 50, 43, 85]).frame([0., 0., w, h]),
            a::rounded_rect(WHITE)
                .frame([origin[0], origin[1], width, 1.])
                .corner_radius(22.0.into()),
            a::canvas(panel.nodes).frame([origin[0], origin[1], width, 1.]),
        ],
        actions: panel.actions,
    };
    let mut rendered = super::finish(b, draft, "OTriOS", form, false)?;
    rendered.backdrop = Some(backdrop);
    let mut blueprint = crate::dashboard::layout::Layout::from_bytes(&rendered.ui.layout)?
        .blueprint()
        .clone();
    let root = &mut blueprint.templates[0].root;
    let panel = &mut root.children[5];
    let mut bottom = 0_f32;
    let close = panel.children.iter().position(|node| {
        node.semantics
            .as_ref()
            .is_some_and(|s| s.label == Expr::Literal(Value::Text("Close".into())))
    });
    let mut errors = vec![];
    for (index, child) in panel.children.iter_mut().enumerate() {
        let x = number(&child.frame[0])? - origin[0];
        let y = number(&child.frame[1])? - origin[1];
        child.frame[0] = Expr::Literal(Value::Number(x));
        child.frame[1] = Expr::Literal(Value::Number(y));
        let message = matches!(&child.shape, Shape::Text { value: Expr::Literal(Value::Text(value)), .. } if !value.is_empty() && messages.contains(value));
        if message {
            errors.push(index);
        }
        if Some(index) != close && !message {
            bottom = bottom.max(y + number(&child.frame[3])?);
        }
    }
    if !errors.is_empty() {
        bottom += 16.;
    }
    for index in errors {
        let child = &mut panel.children[index];
        child.frame[1] = Expr::Literal(Value::Number(bottom));
        bottom += number(&child.frame[3])?;
    }
    if let Some(index) = close {
        let close = &mut panel.children[index];
        let inset = number(&close.frame[0])?;
        close.frame[0] = Expr::Literal(Value::Number(width - inset - number(&close.frame[2])?));
        close.frame[1] = Expr::Literal(Value::Number(bottom + 24.));
        bottom += 24. + number(&close.frame[3])?;
    }
    let content = [width, bottom + 24.];
    let height = content[1].min((h - 48.).max(1.));
    let y = ((h - height) / 2.).clamp(24., 88.);
    panel.frame[1] = Expr::Literal(Value::Number(y));
    panel.frame[3] = Expr::Literal(Value::Number(height));
    panel.shape = Shape::Canvas { clip: true };
    rendered.panel_viewport = Some((panel.id, content));
    let surface = &mut root.children[4];
    surface.frame[1] = Expr::Literal(Value::Number(y));
    surface.frame[3] = Expr::Literal(Value::Number(height));
    rendered.ui.layout = crate::dashboard::layout::Layout::encode(&blueprint)?;
    Ok(rendered)
}
