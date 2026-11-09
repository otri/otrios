// SPDX-License-Identifier: LGPL-3.0-only
//! Small valid fixtures shared by metadata and durable import tests.
use crate::dashboard::layout::Layout;
use otrios_interfaces::layout::*;

#[cfg(feature = "canvas")]
#[path = "../../../apps/oib-designer/app.rs"]
pub(crate) mod oib_designer;

pub(crate) fn empty_layout() -> Vec<u8> {
    Layout::encode(&Blueprint {
        schema: "otrios-layout-v2".into(),
        composition: None,
        templates: vec![Template {
            id: 1,
            name: "Empty".into(),
            parameters: vec![],
            root: TemplateNode {
                id: 0,
                frame: [
                    Expr::Literal(Value::Number(0.)),
                    Expr::Literal(Value::Number(0.)),
                    Expr::Width,
                    Expr::Height,
                ],
                visible: Expr::Literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                resize: None,
                responder: None,
                semantics: None,
                children: vec![],
            },
        }],
        root: Instance {
            id: 0,
            template: 1,
            frame: [0., 0., 1280., 800.],
            properties: vec![],
            children: vec![],
        },
    })
    .expect("valid empty test layout")
}
