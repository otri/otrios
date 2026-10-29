// SPDX-License-Identifier: Apache-2.0
//! Offline bundle filesystem tooling; validation lives in LGPL otrios-core.
use otrios_core::{
    bundle::{self, BundleTree, MAX_FILE_BYTES},
    canvas::{self, Result},
};
use std::{
    fs,
    path::{Path, PathBuf},
};
mod bundle_files;
use bundle_files::{read_file, read_tree};
#[path = "../../../tools/canvas/authoring.rs"]
pub mod authoring;
#[path = "../../../tools/canvas/components.rs"]
pub mod components;

fn sign_tree(mut tree: BundleTree, private: &Path, openssl: &str) -> Result<BundleTree> {
    let key = bundle_files::public_key(private, true, openssl)?;
    let work = WorkDir::new()?;
    fs::write(
        work.0.join("payload"),
        bundle::signing_payload(&bundle::inspect(&tree)?.digest, &key)?,
    )
    .map_err(|e| e.to_string())?;
    let output = std::process::Command::new(openssl)
        .args(["pkeyutl", "-sign", "-rawin", "-inkey"])
        .arg(private)
        .arg("-in")
        .arg(work.0.join("payload"))
        .arg("-out")
        .arg(work.0.join("signature"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Bundle signing failed".into());
    }
    bundle::attach_signature(
        &mut tree,
        &key,
        &fs::read(work.0.join("signature")).map_err(|e| e.to_string())?,
    )?;
    bundle::verify(tree.clone(), &key)?;
    Ok(tree)
}

fn compose(
    base: &Path,
    source: &Path,
    output: &Path,
    key: &Path,
    private: &Path,
    openssl: &str,
) -> Result<()> {
    use otrios_interfaces::{
        bundle::*,
        canvas::{Catalog, CatalogEntry},
    };
    use std::collections::BTreeMap;
    let key = bundle_files::public_key(key, false, openssl)?;
    if key != bundle_files::public_key(private, true, openssl)? {
        return Err("Registry signing and publisher keys differ".into());
    }
    let baseline = canvas::decode_catalog(&read_file(base, MAX_FILE_BYTES)?)?;
    let mut catalog = Catalog {
        format: "otrios-app-catalog-v1".into(),
        apps: vec![],
    };
    let mut bundles = BTreeMap::new();
    for directory in fs::read_dir(source).map_err(|e| e.to_string())? {
        let directory = directory.map_err(|e| e.to_string())?;
        if bundles.len() >= 16 {
            return Err("Installed app count limit".into());
        }
        let verified = bundle::verify(read_tree(&directory.path())?, &key)?;
        let info = &verified.summary().info;
        if directory.file_name().to_str() != Some(&format!("{}.otapp", info.app.id)) {
            return Err("Installed folder differs from app identity".into());
        }
        let old = baseline
            .apps
            .iter()
            .find(|entry| entry.app.id == info.app.id);
        if let Some(old) = old
            && (old.app.state_ref == info.app.state_ref && old.app.ui_ref != info.app.ui_ref
                || old.private != info.private)
        {
            return Err("Install changed existing identity/admission policy".into());
        }
        catalog.apps.push(CatalogEntry {
            app: info.app.clone(),
            path: old
                .map(|entry| entry.path.clone())
                .unwrap_or_else(|| info.app.id.clone()),
            private: info.private,
            ui_hash: canvas::hash(verified.file(UI_PATH)?),
        });
        bundles.insert(info.app.id.clone(), verified);
    }
    // Preserve catalog ordering for stable navigation and the first/root entry.
    catalog.apps.sort_by_key(|entry| {
        (
            baseline
                .apps
                .iter()
                .position(|old| old.app.id == entry.app.id)
                .unwrap_or(usize::MAX),
            entry.app.id.clone(),
        )
    });
    let home = catalog
        .apps
        .iter()
        .find(|entry| entry.path.is_empty())
        .ok_or("Missing Home entry")?
        .clone();
    if catalog
        .apps
        .iter()
        .filter(|entry| entry.path.is_empty())
        .count()
        != 1
    {
        return Err("Ambiguous Home entry".into());
    }
    let old_home = bundles.get(&home.app.id).ok_or("Missing Home bundle")?;
    if bundle::encode_info(&old_home.summary().info)?.as_slice() != old_home.file(INFO_PATH)? {
        return Err("Extended Home metadata requires a compatible registry composer".into());
    }
    let home_ui = canvas::decode_ui(old_home.file(UI_PATH)?)?;
    if home_ui.behavior.is_some() || !home_ui.state_schema.fields.is_empty() {
        return Err("Unsupported Home execution/state contract".into());
    }
    let entries: Vec<_> = catalog
        .apps
        .iter()
        .filter(|entry| !entry.path.is_empty() && !entry.path.contains('/'))
        .collect();
    let keys: Vec<_> = entries.iter().map(|entry| entry.app.id.as_str()).collect();
    let arrangement = || {
        authoring::column(
            24.,
            vec![
                authoring::heading(),
                authoring::wrap_cards(&keys, [384., 176.], [336., 176.], [16., 24.]),
            ],
        )
    };
    let ui = authoring::AppUi::new(&home.app.id, &home.app.title, home_ui.state_schema)
        .with_composition(arrangement(), arrangement())
        .build(
            "OTriOS",
            entries
                .iter()
                .map(|entry| {
                    let icon = if entry.private {
                        components::LaunchIcon::Monitoring
                    } else if entry
                        .app
                        .dependencies
                        .iter()
                        .any(|value| value.starts_with("session:chat"))
                    {
                        components::LaunchIcon::Chat
                    } else {
                        components::LaunchIcon::Playground
                    };
                    if entry.private {
                        components::private_launch_card(
                            &entry.app.id,
                            &entry.path,
                            &entry.app.title,
                            icon,
                        )
                    } else {
                        components::launch_card(
                            &entry.app.id,
                            &entry.path,
                            &entry.app.title,
                            "",
                            icon,
                        )
                    }
                })
                .collect(),
        );
    let bytes = canvas::encode_ui(&ui)?;
    let mut entry = home.clone();
    entry.ui_hash = canvas::hash(&bytes);
    let old_digest = old_home.summary().digest.clone();
    let revision = old_home
        .summary()
        .info
        .revision
        .checked_add(1)
        .ok_or("Home revision overflow")?;
    let mut candidate = bundle::pack(&entry, revision, &bytes, None)?;
    for (path, data) in old_home.files().filter(|(path, _)| {
        path.starts_with("Resources/")
            || path.starts_with("Dependencies/")
            || path.starts_with("Notices/")
    }) {
        candidate = bundle::with_payload(candidate, path, data)?;
    }
    let tree = if bytes == old_home.file(UI_PATH)? {
        old_home
            .files()
            .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
            .collect()
    } else {
        sign_tree(candidate, private, openssl)?
    };
    let updated = bundle::verify(tree, &key)?;
    catalog
        .apps
        .iter_mut()
        .find(|entry| entry.app.id == home.app.id)
        .ok_or("Missing Home")?
        .ui_hash = canvas::hash(updated.file(UI_PATH)?);
    bundles.insert(home.app.id.clone(), updated);
    let bytes = canvas::encode_catalog(&catalog)?;
    for entry in &catalog.apps {
        let bundle = &bundles[&entry.app.id];
        let mut client = canvas::Client::default();
        client.admit_catalog(&bytes)?;
        client.mount_local(
            bundle.file(UI_PATH)?,
            &entry.app.state_ref,
            if bundle.summary().info.behavior.is_some() {
                bundle.file(CODE_PATH)?
            } else {
                &[]
            },
            &[],
        )?;
        let ui = canvas::decode_ui(bundle.file(UI_PATH)?)?;
        for destination in ui.navigation {
            if !catalog
                .apps
                .iter()
                .any(|entry| entry.app.ui_ref == destination.ui_ref)
            {
                return Err("Unresolved app navigation dependency".into());
            }
        }
    }
    // Assemble only under a newly owned private directory. Publication uses the
    // existing signed-release atomic activation and expected-baseline lock.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(output).map_err(|e| e.to_string())?;
    let result = (|| {
        for (id, bundle) in &bundles {
            let tree = bundle
                .files()
                .map(|(path, bytes)| (path.to_string(), bytes.to_vec()))
                .collect();
            write_tree(&output.join(format!("{id}.otapp")), &tree)?;
        }
        fs::write(output.join("catalog.flex"), bytes).map_err(|e| e.to_string())?;
        println!(
            "COMPOSED {} apps; previous Home bundle {}",
            bundles.len(),
            old_digest
        );
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

struct WorkDir(pub PathBuf);
impl WorkDir {
    fn new() -> Result<Self> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("otrios-bundle-{}-{nonce}", std::process::id()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_tree(destination: &Path, tree: &BundleTree) -> Result<()> {
    if destination.extension().and_then(|s| s.to_str()) != Some("otapp") {
        return Err("Bundle output must end in .otapp".into());
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err("Bundle output already exists".into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid bundle output")?;
    let temporary = parent.join(format!(".{name}.pending-{}", std::process::id()));
    // A new sibling staging directory; never reuse or clean up somebody else's output.
    let mut directory = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(&temporary).map_err(|e| e.to_string())?;
    let result = (|| {
        for (path, bytes) in tree {
            let output = temporary.join(path);
            fs::create_dir_all(output.parent().ok_or("Missing bundle file parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(output, bytes).map_err(|e| e.to_string())?;
        }
        bundle::inspect(&read_tree(&temporary)?)?;
        if fs::symlink_metadata(destination).is_ok() {
            return Err("Bundle output appeared during staging".into());
        }
        fs::rename(&temporary, destination).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn report(tree: &BundleTree) -> Result<()> {
    let result = bundle::inspect(tree)?;
    println!(
        "{}: {} (revision {})",
        result.info.app.id, result.info.app.title, result.info.revision
    );
    println!("bundle-sha256: {}", result.digest);
    println!(
        "{} files; {} bytes; private: {}",
        result.files, result.bytes, result.info.private
    );
    println!(
        "integrity: verified; publisher: unauthenticated; installable: no (use verify before admission)"
    );
    println!(
        "declared dependencies (unresolved): {:?}",
        result.info.app.dependencies
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("materialize") if args.len()==8 && args[4]=="--key" && args[6]=="--openssl" => {
            use otrios_interfaces::bundle::*;
            let key=bundle_files::public_key(Path::new(&args[5]),false,&args[7])?;
            let catalog=read_file(Path::new(&args[1]),MAX_FILE_BYTES)?;
            let entries=canvas::decode_catalog(&catalog)?;
            canvas::Client::default().admit_catalog(&catalog)?;
            let output=Path::new(&args[3]);
            fs::create_dir(output).map_err(|e|e.to_string())?;
            let result=(|| {
                for entry in entries.apps {
                    let verified=bundle::verify(read_tree(&Path::new(&args[2]).join(format!("{}.otapp",entry.app.id)))?,&key)?;
                    if verified.summary().info.app!=entry.app || verified.summary().info.private!=entry.private || canvas::hash(verified.file(UI_PATH)?)!=entry.ui_hash { return Err("Catalog differs from bundle".into()); }
                    let folder=output.join(&entry.app.id).join("package");fs::create_dir_all(&folder).map_err(|e|e.to_string())?;
                    fs::write(folder.join("app.flex"),canvas::encode_app(&entry.app)?).map_err(|e|e.to_string())?;
                    fs::write(folder.join("ui.flex"),verified.file(UI_PATH)?).map_err(|e|e.to_string())?;
                    if verified.summary().info.behavior.is_some() {fs::write(folder.join("behavior.wasm"),verified.file(CODE_PATH)?).map_err(|e|e.to_string())?;}
                }
                fs::write(output.join("catalog.flex"),catalog).map_err(|e|e.to_string())?;Ok(())
            })();
            if result.is_err() {let _=fs::remove_dir_all(output);}result
        }
        Some("compare") if args.len() == 7 && args[3] == "--key" && args[5] == "--openssl" => {
            let key = bundle_files::public_key(Path::new(&args[4]), false, &args[6])?;
            let before = bundle::verify(read_tree(Path::new(&args[1]))?, &key)?;
            let after = bundle::verify(read_tree(Path::new(&args[2]))?, &key)?;
            bundle::compatible_install(&before, &after)?;
            println!("COMPATIBLE {}", after.summary().info.app.id);
            Ok(())
        }
        Some("bump") if args.len() == 8 && args[4] == "--key" && args[6] == "--openssl" => {
            let key = bundle_files::public_key(Path::new(&args[5]), false, &args[7])?;
            let before = bundle::verify(read_tree(Path::new(&args[2]))?, &key)?;
            let mut current = bundle::with_revision(read_tree(Path::new(&args[1]))?, before.summary().info.revision)?;
            for (path,data) in before.files().filter(|(path,_)|path.starts_with("Resources/")||path.starts_with("Dependencies/")||path.starts_with("Notices/")) {
                if !current.contains_key(path) { current=bundle::with_payload(current,path,data)?; }
            }
            if bundle::inspect(&current)?.info.app.id != before.summary().info.app.id { return Err("App identity differs".into()); }
            let current = if bundle::inspect(&current)?.digest == before.summary().digest { current }
                else { bundle::with_revision(current, before.summary().info.revision.checked_add(1).ok_or("App revision overflow")?)? };
            write_tree(Path::new(&args[3]), &current)
        }
        Some("compose") if args.len() == 10 && args[4] == "--key" && args[6] == "--sign-key" && args[8] == "--openssl" =>
            compose(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3]), Path::new(&args[5]), Path::new(&args[7]), &args[9]),
        Some("inspect") if args.len() == 2 => report(&read_tree(Path::new(&args[1]))?),
        Some("verify") if args.len() == 6 && args[2] == "--key" && args[4] == "--openssl" => {
            let key = bundle_files::public_key(Path::new(&args[3]), false, &args[5])?;
            let verified = bundle::verify(read_tree(Path::new(&args[1]))?, &key)?;
            println!("VERIFIED {} {}", verified.summary().info.app.id, verified.summary().digest);
            Ok(())
        }
        Some("sign") if args.len() == 7 && args[3] == "--key" && args[5] == "--openssl" => {
            let tree = sign_tree(read_tree(Path::new(&args[1]))?, Path::new(&args[4]), &args[6])?;
            let key = bundle_files::public_key(Path::new(&args[4]), true, &args[6])?;
            let verified = bundle::verify(tree.clone(), &key)?;
            write_tree(Path::new(&args[2]), &tree)?;
            println!("SIGNED {} {}", verified.summary().info.app.id, verified.summary().digest);
            Ok(())
        }
        Some("export") if args.len() == 6 && args[4] == "--revision" => {
            // Explicit app ID is one path component, never a caller-supplied filesystem path.
            if args[2].is_empty() || args[2].len() > 64
                || !args[2].bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err("Invalid app ID".into());
            }
            let root = Path::new(&args[1]);
            let catalog = canvas::decode_catalog(&read_file(&root.join("catalog.flex"), MAX_FILE_BYTES)?)?;
            let mut entries = catalog.apps.iter().filter(|entry| entry.app.id == args[2]);
            let entry = entries.next().ok_or("App absent from catalog")?;
            if catalog.format != "otrios-app-catalog-v1" || entries.next().is_some() {
                return Err("Unsupported or ambiguous app catalog".into());
            }
            let source = root.join(&args[2]).join("package");
            let app = canvas::decode_app(&read_file(&source.join("app.flex"), MAX_FILE_BYTES)?)?;
            if app != entry.app {
                return Err("App differs from catalog".into());
            }
            let ui = read_file(&source.join("ui.flex"), MAX_FILE_BYTES)?;
            let code = match fs::symlink_metadata(source.join("behavior.wasm")) {
                Ok(_) => Some(read_file(&source.join("behavior.wasm"), MAX_FILE_BYTES)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            let revision = args[5].parse().map_err(|_| "Invalid bundle revision")?;
            let tree = bundle::pack(entry, revision, &ui, code.as_deref())?;
            let tree = bundle::with_payload(tree,"Notices/Apache-2.0.txt",include_bytes!("../../../LICENSES/Apache-2.0.txt"))?;
            write_tree(Path::new(&args[3]), &tree)?;
            report(&tree)
        }
        _ => Err("Use canvas-bundle inspect BUNDLE; export APPS APP_ID OUTPUT.otapp --revision N; sign INPUT OUTPUT.otapp --key PRIVATE --openssl PATH; verify BUNDLE --key PUBLIC --openssl PATH".into()),
    }
}
