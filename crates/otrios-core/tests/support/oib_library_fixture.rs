// SPDX-License-Identifier: LGPL-3.0-only
//! Synthetic already-verified immutable library closure for runtime tests.
#[path = "../../../../apps/oib-designer/app.rs"]
mod app;
use otrios_core::{
    canvas::{self, oib},
    library,
};
use otrios_interfaces::{
    canvas::{Presentation, Ui},
    oib::{
        Seed,
        designer::*,
        graph::{
            self, DefinitionBinding, ImportBinding, Imports, LibraryLock, LockedExport,
            ResolvedLibrary,
        },
        library::{Export, ExportId, Graph, LibraryId},
    },
};
pub(crate) fn seed(ui: &Ui) -> &Seed {
    oib::seed(ui).unwrap()
}
pub(crate) fn seed_mut(ui: &mut Ui) -> &mut Seed {
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        unreachable!()
    };
    seed
}
fn id() -> LibraryId {
    LibraryId(format!("{}:controls", "0".repeat(64)))
}
pub(crate) fn imported() -> Ui {
    assert_eq!(app::manifest().id, "oib-designer");
    let mut ui = app::ui(&[]);
    let source = seed_mut(&mut ui);
    let graph = source.graph.as_mut().unwrap();
    let definitions = graph
        .definitions
        .iter()
        .filter(|definition| matches!(definition.id.0, 12 | 13))
        .cloned()
        .collect();
    let library = library::seal_graph(Graph {
        styles: None,
        behaviors: vec![],
        format: otrios_interfaces::oib::library::FORMAT.into(),
        required_classes: graph.classes.clone(),
        definitions,
        exports: vec![
            Export {
                id: ExportId(1),
                name: "Slider".into(),
                definition: DefinitionId(12),
                contract: [0; 32],
            },
            Export {
                id: ExportId(2),
                name: "Toggle".into(),
                definition: DefinitionId(13),
                contract: [0; 32],
            },
        ],
    })
    .unwrap();
    graph
        .definitions
        .retain(|definition| !matches!(definition.id.0, 12 | 13));
    graph.imports = Some(Imports {
        format: graph::IMPORTS_FORMAT.into(),
        locks: vec![LibraryLock {
            library: id(),
            revision: [1; 32],
            exports: library
                .exports
                .iter()
                .map(|export| LockedExport {
                    export: export.id,
                    contract: export.contract,
                })
                .collect(),
        }],
        bindings: vec![
            ImportBinding {
                id: 70,
                library: id(),
                export: ExportId(1),
                definitions: vec![DefinitionBinding {
                    source: DefinitionId(12),
                    target: DefinitionId(12),
                }],
            },
            ImportBinding {
                id: 71,
                library: id(),
                export: ExportId(2),
                definitions: vec![DefinitionBinding {
                    source: DefinitionId(13),
                    target: DefinitionId(13),
                }],
            },
        ],
    });
    source.resolved_libraries = vec![ResolvedLibrary {
        library: id(),
        revision: [1; 32],
        graph: library,
        component: None,
    }];
    ui
}
pub(crate) fn admit(ui: &Ui) -> Ui {
    canvas::decode_ui(&canvas::encode_ui(ui).unwrap()).unwrap()
}
pub(crate) fn update(ui: &Ui, edit: impl FnOnce(&mut Graph)) -> Ui {
    let mut result = ui.clone();
    let source = seed_mut(&mut result);
    edit(&mut source.resolved_libraries[0].graph);
    source.resolved_libraries[0].graph =
        library::seal_graph(source.resolved_libraries[0].graph.clone()).unwrap();
    source.resolved_libraries[0].revision = [2; 32];
    let pin = &mut source
        .graph
        .as_mut()
        .unwrap()
        .imports
        .as_mut()
        .unwrap()
        .locks[0];
    pin.revision = [2; 32];
    pin.exports = source.resolved_libraries[0]
        .graph
        .exports
        .iter()
        .map(|export| LockedExport {
            export: export.id,
            contract: export.contract,
        })
        .collect();
    result
}
