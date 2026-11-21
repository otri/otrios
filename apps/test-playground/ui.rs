// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use crate::components::{LaunchIcon, launch_card};
use otrios_interfaces::canvas::Ui;
pub fn description() -> Ui {
    let arrangement = || {
        column(
            24.,
            vec![
                row(16., vec![heading(), place("edit-order", [136., 60.])]),
                wrap_cards(&super::state::CARDS, [384., 176.], [336., 176.], [16., 24.]),
            ],
        )
    };
    AppUi::new("test-playground", "Test Playground", super::state::schema())
        .with_breadcrumbs(&[("OTriOS", "home")])
        .with_composition(arrangement(), arrangement())
        .with_reorder(
            &super::state::CARDS
                .iter()
                .enumerate()
                .map(|(i, key)| (*key, i as u16 + 1))
                .collect::<Vec<_>>(),
        )
        .build(
            "Test Playground",
            vec![
                launch_card(
                    "component-playground",
                    "components",
                    "Components",
                    "Sliders, dial and touch pad",
                    LaunchIcon::Components,
                ),
                launch_card(
                    "layout-lab",
                    "layouts",
                    "Layouts",
                    "Resizable components",
                    LaunchIcon::Layouts,
                ),
                launch_card(
                    "layout-document",
                    "long-document",
                    "Long Document",
                    "Read reflowing text",
                    LaunchIcon::Document,
                ),
                launch_card(
                    "layout-text-scroll",
                    "text-panning",
                    "Text Panning",
                    "Pan in two dimensions",
                    LaunchIcon::Panning,
                ),
                launch_card(
                    "input-lab",
                    "text-input",
                    "Text Input",
                    "Typing and editing",
                    LaunchIcon::Input,
                ),
                launch_card(
                    "animation-lab",
                    "animations-two-d",
                    "2D Animations",
                    "Motion and timing",
                    LaunchIcon::Animation2d,
                ),
                launch_card(
                    "three-d-lab",
                    "animations-three-d",
                    "3D Animations",
                    "Rotation and perspective",
                    LaunchIcon::Animation3d,
                ),
                canvas(vec![
                    surface(CARD, 12.).pin_edges(0.),
                    text(reorder_label())
                        .frame([24., 28., 90., 24.])
                        .font_scale(2.),
                ])
                .frame([0., 0., 136., 80.])
                .key("edit-order")
                .semantic_label("Edit cards")
                .on_press(Event::edit_order()),
            ],
        )
}
