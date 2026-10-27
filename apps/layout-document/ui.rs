// SPDX-License-Identifier: Apache-2.0
use crate::authoring::*;
use otrios_interfaces::canvas::Ui;

pub fn description() -> Ui {
    let mut source = String::with_capacity(1024 * 1024 + 1024);
    let mut paragraph = 0;
    while source.len() < 1024 * 1024 {
        paragraph += 1;
        source.push_str(&format!("Paragraph {paragraph:04}. "));
        for _ in 0..1 + paragraph % 9 {
            source.push_str("The quick brown fox jumps over the lazy dog. A long document keeps its original text while each paragraph reflows to the available width. Drag to read; retained chunks move through a clipped viewport. jgyÉ - bearings, accents and descenders. ");
        }
        source.push_str("\n\n");
    }
    let resource =
        otrios_core::canvas::document::encode_resource(&source).expect("bounded lab resource");
    let composition = || {
        column(
            12.,
            vec![
                row(
                    12.,
                    vec![
                        place("$heading", [650., 64.])
                            .minimum([240., 64.])
                            .flex_width(),
                        place("back", [140., 64.]),
                        place("home", [140., 64.]),
                    ],
                ),
                place("hint", [1184., 24.]).minimum([1., 24.]).flex_width(),
                place("page", [1184., 600.])
                    .minimum([240., 100.])
                    .flex_width()
                    .flex_height(),
                place("footer", [1184., 24.])
                    .minimum([1., 24.])
                    .flex_width(),
            ],
        )
    };
    AppUi::new(
        "layout-document",
        "Long document",
        schema("layout-document-state-v1", &[]),
    )
    .with_composition(composition(), composition())
    .with_text_document("page", resource, 2, WHITE)
    .build(
        "Long document",
        vec![
            button("Back").on_press(Event::back()).key("back"),
            button("Home")
                .navigate_to(Destination::app("home"))
                .key("home"),
            text("EXPERIMENT - 1 MiB of wrapped text. Drag to read; release to coast.")
                .color(MUTED)
                .key("hint"),
            canvas(vec![]).frame([0., 0., 1184., 600.]).key("page"),
            text(" ").key("footer"),
        ],
    )
}
