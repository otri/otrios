// SPDX-License-Identifier: Apache-2.0
//! Library filesystem/signing authoring. All admission remains in covered core.
use super::{bundle_files, read_file, read_tree, sign_payload, write_package, write_tree};
use otrios_core::{
    bundle,
    canvas::{self, Result},
    library,
};
use otrios_interfaces::{
    bundle::UI_PATH,
    canvas::Presentation,
    oib::{
        designer::DefinitionId,
        graph::*,
        library::{self as contract, Export, ExportId, LibraryId, LibraryInfo},
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PackageSource {
    info: LibraryInfo,
    graph: contract::Graph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    component_wat: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceSpec {
    library: LibraryId,
    version: String,
    exports: Vec<ExportSpec>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportSpec {
    id: ExportId,
    name: String,
    definition: DefinitionId,
}

fn json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    serde_json::from_slice(&read_file(Path::new(path), library::MAX_PACKAGE_BYTES)?)
        .map_err(|error| error.to_string())
}
fn write_json(path: &str, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())
}
fn write_library(path: &str, tree: &bundle::BundleTree) -> Result<()> {
    write_package(Path::new(path), tree, "otlib", |tree| {
        library::inspect(tree).map(|_| ())
    })
}

pub(super) fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("library-pack") if args.len()==3 => {
            let mut source: PackageSource=json(&args[1])?;
            source.graph=library::seal_graph(source.graph)?;
            let tree=if let Some(path)=&source.component_wat {
                let base=Path::new(&args[1]).parent().ok_or("Library source parent required")?;
                let relative=Path::new(path);
                if relative.is_absolute() || relative.components().any(|part|matches!(part,std::path::Component::ParentDir)) { return Err("Component WAT must be a relative source path".into()); }
                let path=base.join(relative);
                let text=read_file(&path,library::MAX_COMPONENT_BYTES)?;
                let code=wat::parse_bytes(&text).map_err(|error|error.to_string())?;
                library::pack_with_behavior(&source.info,&source.graph,&code)?
            } else {library::pack(&source.info,&source.graph)?};
            write_library(&args[2],&tree)?;
            println!("PACKED {}",library::inspect(&tree)?.digest); Ok(())
        }
        Some("library-inspect") if args.len()==2 => {
            let summary=library::inspect(&read_tree(Path::new(&args[1]))?)?;
            println!("LIBRARY {} {} {}",summary.info.library.0,summary.info.version,summary.digest); Ok(())
        }
        Some("library-verify") if args.len()==6 && args[2]=="--key" && args[4]=="--openssl" => {
            let key=bundle_files::public_key(Path::new(&args[3]),false,&args[5])?;
            let verified=library::verify(read_tree(Path::new(&args[1]))?,&key)?;
            println!("VERIFIED LIBRARY {} {}",verified.summary().info.library.0,verified.summary().digest); Ok(())
        }
        Some("library-sign") if args.len()==7 && args[3]=="--key" && args[5]=="--openssl" => {
            let key=bundle_files::public_key(Path::new(&args[4]),true,&args[6])?;
            let mut tree=read_tree(Path::new(&args[1]))?;
            let signature=sign_payload(&library::signing_payload(&library::inspect(&tree)?.digest,&key)?,Path::new(&args[4]),&args[6])?;
            library::attach_signature(&mut tree,&key,&signature)?;
            let verified=library::verify(tree.clone(),&key)?;
            write_library(&args[2],&tree)?;
            println!("SIGNED LIBRARY {} {}",verified.summary().info.library.0,verified.summary().digest); Ok(())
        }
        Some("library-source") if args.len()==8 && args[4]=="--key" && args[6]=="--openssl" => {
            let key=bundle_files::public_key(Path::new(&args[5]),false,&args[7])?;
            let app=bundle::verify(read_tree(Path::new(&args[1]))?,&key)?;
            let spec:SourceSpec=json(&args[2])?;
            let ui=canvas::decode_ui(app.file(UI_PATH)?)?;
            let graph=canvas::oib::seed(&ui).and_then(|seed|seed.graph.as_ref()).ok_or("App has no canonical OIB graph")?;
            let mut ids:BTreeSet<_>=spec.exports.iter().map(|export|export.definition).collect();
            loop {
                let before=ids.len();
                for id in ids.clone() {
                    let definition=graph.definitions.iter().find(|def|def.id==id).ok_or("Export source is not a local definition")?;
                    for node in &definition.nodes {if let Some(reference)=&node.reference {ids.insert(reference.definition);}}
                }
                if before==ids.len() {break;}
            }
            let graph=library::seal_graph(contract::Graph {
                behaviors:vec![], format:contract::FORMAT.into(), required_classes:graph.classes.clone(),
                definitions:graph.definitions.iter().filter(|def|ids.contains(&def.id)).cloned().collect(),
                exports:spec.exports.into_iter().map(|e|Export{id:e.id,name:e.name,definition:e.definition,contract:[0;32]}).collect(),
            })?;
            let source=PackageSource {info:LibraryInfo {format:contract::INFO_FORMAT.into(),library:spec.library,version:spec.version,required_features:vec![contract::FEATURE.into()],entry_graph:contract::GRAPH_PATH.into()},graph,component_wat:None};
            library::pack(&source.info,&source.graph)?;
            write_json(&args[3],&source)
        }
        Some("library-bind") if args.len()==9 && args[5]=="--key" && args[7]=="--openssl" => bind(args),
        _ => Err("Use canvas-bundle library-source APP.otapp SPEC.json OUTPUT.json --key PUBLIC --openssl PATH; library-pack SOURCE.json OUTPUT.otlib; library-sign INPUT.otlib OUTPUT.otlib --key PRIVATE --openssl PATH; library-verify LIB.otlib --key PUBLIC --openssl PATH; library-bind APP.otapp LIB.otlib BINDINGS.json OUTPUT.otapp --key PUBLIC --openssl PATH".into())
    }
}

fn bind(args: &[String]) -> Result<()> {
    let key = bundle_files::public_key(Path::new(&args[6]), false, &args[8])?;
    let app = bundle::verify(read_tree(Path::new(&args[1]))?, &key)?;
    let lib = library::verify(read_tree(Path::new(&args[2]))?, &key)?;
    let bindings: Vec<ImportBinding> = json(&args[3])?;
    let id = &lib.summary().info.library;
    if bindings.is_empty() || bindings.iter().any(|binding| &binding.library != id) {
        return Err("Bindings must name the supplied library".into());
    }
    let mut ui = canvas::decode_ui(app.file(UI_PATH)?)?;
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        return Err("App has no canonical OIB graph".into());
    };
    let graph = seed.graph.as_mut().ok_or("Missing canonical OIB graph")?;
    let imports = graph.imports.get_or_insert_with(|| Imports {
        format: IMPORTS_FORMAT.into(),
        locks: vec![],
        bindings: vec![],
    });
    let old: Vec<_> = imports
        .bindings
        .iter()
        .filter(|binding| &binding.library == id)
        .cloned()
        .collect();
    if !old.is_empty() && old != bindings {
        return Err("Updating a library must preserve its stable import bindings".into());
    }
    let targets: BTreeSet<_> = bindings
        .iter()
        .flat_map(|binding| {
            binding
                .definitions
                .iter()
                .map(|definition| definition.target)
        })
        .collect();
    if old.is_empty() {
        graph
            .definitions
            .retain(|definition| !targets.contains(&definition.id));
    }
    let mut exports = vec![];
    for binding in &bindings {
        let export = lib
            .graph()
            .exports
            .iter()
            .find(|export| export.id == binding.export)
            .ok_or("Unknown library export")?;
        if !exports
            .iter()
            .any(|locked: &LockedExport| locked.export == export.id)
        {
            exports.push(LockedExport {
                export: export.id,
                contract: export.contract,
            });
        }
    }
    exports.sort_by_key(|export| export.export);
    imports.bindings.retain(|binding| &binding.library != id);
    imports.bindings.extend(bindings);
    imports.bindings.sort_by_key(|binding| binding.id);
    imports.locks.retain(|lock| &lock.library != id);
    imports.locks.push(LibraryLock {
        library: id.clone(),
        revision: lib.revision(),
        exports,
    });
    imports.locks.sort_by(|a, b| a.library.cmp(&b.library));
    seed.resolved_libraries.retain(|body| &body.library != id);
    seed.resolved_libraries.push(ResolvedLibrary {
        library: id.clone(),
        revision: lib.revision(),
        graph: lib.graph().clone(),
        component: lib.component().cloned(),
    });
    seed.resolved_libraries
        .sort_by(|a, b| a.library.cmp(&b.library));
    let locks = imports.locks.clone();
    let revision = app
        .summary()
        .info
        .revision
        .checked_add(1)
        .ok_or("App revision exhausted")?;
    let libs = vec![lib];
    // The strict bundle helper retains and verifies already pinned dependencies.
    let candidate = bundle::with_oib_ui_and_libraries(
        app.files()
            .map(|(path, bytes)| (path.to_owned(), bytes.to_vec()))
            .collect(),
        &ui,
        &locks,
        &libs,
        &key,
    )?;
    let candidate = bundle::with_revision(candidate, revision)?;
    write_tree(Path::new(&args[4]), &candidate)?;
    println!("BOUND {}", bundle::inspect(&candidate)?.digest);
    Ok(())
}

/// Reserved independently signed library dependency roots are rebuilt whole.
pub(super) fn library_path(path: &str) -> bool {
    let mut parts = path.split('/');
    parts.next() == Some("Dependencies")
        && parts.next().is_some_and(|name| {
            name.strip_suffix(".otlib").is_some_and(|hash| {
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        })
}

/// Full builds keep an installed app's exact libraries unless the publisher
/// explicitly changes its pins. Local Rust fixtures do not replace library data.
pub(super) fn inherit_libraries(
    tree: bundle::BundleTree,
    previous: &bundle::BundleTree,
    key: &[u8; 32],
) -> Result<bundle::BundleTree> {
    // A deliberately prepared candidate owns its explicit pins. Inheritance is
    // only for the ordinary full-build source which has no dependency lock.
    let current = bundle::inspect(&tree)?;
    if !current.info.libraries.is_empty() {
        let mut libraries = vec![];
        for lock in &current.info.libraries {
            let prefix = format!("Dependencies/{}.otlib/", canvas::hex(&lock.revision));
            let nested = tree
                .iter()
                .filter_map(|(path, bytes)| {
                    path.strip_prefix(&prefix)
                        .map(|path| (path.to_owned(), bytes.clone()))
                })
                .collect();
            libraries.push(library::verify(nested, key)?);
        }
        let ui = canvas::decode_ui(tree.get(UI_PATH).ok_or("Missing current UI")?)?;
        return bundle::with_oib_ui_and_libraries(
            tree,
            &ui,
            &current.info.libraries,
            &libraries,
            key,
        );
    }
    let info = bundle::decode_info(
        previous
            .get(otrios_interfaces::bundle::INFO_PATH)
            .ok_or("Missing predecessor metadata")?,
    )?;
    if info.libraries.is_empty() {
        return Ok(tree);
    }
    let previous = bundle::verify(previous.clone(), key)?;
    let old_ui = canvas::decode_ui(previous.file(UI_PATH)?)?;
    let old_seed = canvas::oib::seed(&old_ui).ok_or("Missing pinned library source")?;
    let imports = old_seed
        .graph
        .as_ref()
        .and_then(|graph| graph.imports.as_ref())
        .ok_or("Missing pinned import bindings")?;
    let mut ui = canvas::decode_ui(tree.get(UI_PATH).ok_or("Missing current UI")?)?;
    let Some(Presentation::Oib(seed)) = &mut ui.presentation else {
        return Err("Full build removed a pinned OIB document".into());
    };
    let graph = seed
        .graph
        .as_mut()
        .ok_or("Missing canonical current graph")?;
    let targets: BTreeSet<_> = imports
        .bindings
        .iter()
        .flat_map(|binding| binding.definitions.iter().map(|binding| binding.target))
        .collect();
    graph
        .definitions
        .retain(|definition| !targets.contains(&definition.id));
    graph.imports = Some(imports.clone());
    seed.resolved_libraries = old_seed.resolved_libraries.clone();
    let mut libraries = vec![];
    for lock in &info.libraries {
        let prefix = format!("Dependencies/{}.otlib/", canvas::hex(&lock.revision));
        let nested = previous
            .files()
            .filter_map(|(path, bytes)| {
                path.strip_prefix(&prefix)
                    .map(|path| (path.to_owned(), bytes.to_vec()))
            })
            .collect();
        libraries.push(library::verify(nested, key)?);
    }
    bundle::with_oib_ui_and_libraries(tree, &ui, &info.libraries, &libraries, key)
}
