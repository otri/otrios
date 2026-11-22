// SPDX-License-Identifier: LGPL-3.0-only
//! Shell presentation preferences are local and never mutate authored app state.
use super::*;
use oib_workspace::Appearance;
use otrios_interfaces::scene::{Color, Primitive};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Preference {
    #[default]
    System,
    Light,
    Dark,
}
impl Preference {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err("Unknown appearance preference".into()),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
    pub fn resolve(self, system: Appearance) -> Appearance {
        match self {
            Self::System => system,
            Self::Light => Appearance::Light,
            Self::Dark => Appearance::Dark,
        }
    }
}
pub(super) fn present(root: &mut SceneNode, mount: &Mount) {
    let Some(trail) = &mount.breadcrumbs else {
        return;
    };
    let p = mount.shell_appearance.palette();
    let preview = mount
        .designer
        .as_ref()
        .and_then(|_| super::super::oib::seed(&mount.ui))
        .and_then(|seed| seed.instance_surface.as_ref())
        .map(|surface| u32::from(surface.preview_viewport));
    let role = |color: Color, text: bool, control: bool| -> Color {
        use otrios_interfaces::theme::*;
        if trail.host {
            match color {
                [247, 246, 240, 255] => p.background,
                [255, 254, 250, 255] => p.surface,
                [44, 50, 43, 255] => p.text,
                [109, 116, 105, 255] => p.muted,
                [52, 77, 61, 255] => p.focus,
                [221, 223, 212, 255] => {
                    if control {
                        p.line
                    } else {
                        p.divider
                    }
                }
                [238, 229, 210, 255] if mount.shell_appearance == Appearance::Dark => {
                    [48, 44, 37, 255]
                }
                [229, 235, 222, 255] if mount.shell_appearance == Appearance::Dark => {
                    [39, 47, 39, 255]
                }
                [234, 234, 226, 255] if mount.shell_appearance == Appearance::Dark => {
                    [44, 45, 42, 255]
                }
                [223, 233, 238, 255] if mount.shell_appearance == Appearance::Dark => {
                    [35, 45, 52, 255]
                }
                [232, 225, 239, 255] if mount.shell_appearance == Appearance::Dark => {
                    [44, 38, 51, 255]
                }
                [241, 224, 221, 255] if mount.shell_appearance == Appearance::Dark => {
                    [51, 38, 38, 255]
                }
                _ => color,
            }
        } else {
            match color {
                BG => p.background,
                CARD => p.surface,
                TEXT => p.text,
                MUTED => p.muted,
                LINE => {
                    if control {
                        p.line
                    } else {
                        p.divider
                    }
                }
                BRASS if text || control => p.focus,
                COPPER if text && mount.shell_appearance == Appearance::Light => p.focus,
                _ => color,
            }
        }
    };
    fn walk(
        node: &mut SceneNode,
        preview: Option<u32>,
        host: bool,
        inherited_control: bool,
        chart: bool,
        appearance: Appearance,
        role: &impl Fn(Color, bool, bool) -> Color,
    ) {
        if preview == Some(node.id) {
            return;
        }
        // Launcher icons retain their gold/copper/charcoal identity in both modes.
        if host
            && node.frame.width == 48.
            && node.frame.height == 48.
            && matches!(node.primitive, Primitive::Canvas { .. })
            && node.responder.is_none()
        {
            return;
        }
        let control = inherited_control
            || matches!(
                node.responder,
                Some(TouchResponder::Activate(_) | TouchResponder::Drag(_))
            );
        let quantitative = chart && (3_000_000_000..4_000_000_000).contains(&node.id);
        match &mut node.primitive {
            Primitive::Text { color, .. } => *color = role(*color, true, control),
            Primitive::Superellipse { color, .. }
            | Primitive::Path { color, .. }
            | Primitive::Circle { color }
            | Primitive::Triangles { color, .. } => {
                if quantitative
                    && appearance == Appearance::Light
                    && *color == otrios_interfaces::theme::COPPER
                {
                    *color = [137, 78, 46, 255];
                } else {
                    *color = role(*color, false, control || quantitative);
                }
            }
            _ => {}
        }
        for child in &mut node.children {
            walk(child, preview, host, control, chart, appearance, role);
        }
    }
    walk(
        root,
        preview,
        trail.host,
        false,
        matches!(mount.ui.presentation, Some(Presentation::View { .. })),
        mount.shell_appearance,
        &role,
    );
}
impl Client {
    pub fn appearance_preference(&self) -> &'static str {
        self.shell_preference.name()
    }
    pub fn appearance_is_dark(&self) -> bool {
        self.shell_preference.resolve(self.system_appearance) == Appearance::Dark
    }
    pub fn set_appearance(&mut self, preference: &str, system_dark: bool) -> Result<bool> {
        let preference = Preference::parse(preference)?;
        let system = if system_dark {
            Appearance::Dark
        } else {
            Appearance::Light
        };
        if preference == self.shell_preference && system == self.system_appearance {
            self.pending_appearance = None;
            return Ok(false);
        }
        if self.input_busy() {
            self.pending_appearance = Some((preference, system_dark));
            return Ok(false);
        }
        self.pending_appearance = None;
        self.shell_preference = preference;
        self.system_appearance = system;
        let appearance = preference.resolve(system);
        for mount in self
            .active
            .iter_mut()
            .chain(self.qualified.iter_mut().map(|q| &mut q.mount))
        {
            mount.shell_appearance = appearance;
            if let Some(trail) = &mut mount.breadcrumbs {
                trail.preference = preference;
            }
            if let Some(designer) = &mut mount.designer {
                designer.chrome_appearance = appearance;
                designer.revision = None;
            }
            if mount.designer.is_some() {
                mount.refresh()?;
            }
            mount.invalidate();
        }
        Ok(true)
    }
}
