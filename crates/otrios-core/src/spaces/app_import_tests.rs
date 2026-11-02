// SPDX-License-Identifier: LGPL-3.0-only
//! Durable install phases and real process exits at the shared ledger boundary.
use super::*;
use crate::{bundle, canvas, transfers::*};
use otrios_interfaces::canvas::{App, CatalogEntry, StateSchema, Ui};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::process::{Command, Stdio};
fn code() -> (Vec<u8>, [u8; 32]) {
    let key = Ed25519KeyPair::from_seed_unchecked(&[39; 32]).unwrap();
    let public = key.public_key().as_ref().try_into().unwrap();
    let ui = Ui {
        format: "otrios-canvas-v2".into(),
        ui_ref: "app:sample/ui".into(),
        title: "Sample".into(),
        layout: vec![],
        state_schema: StateSchema {
            id: "sample-v1".into(),
            fields: vec![],
        },
        selectors: vec![],
        handlers: vec![],
        navigation: vec![],
        behavior: None,
        presentation: None,
    };
    let bytes = canvas::encode_ui(&ui).unwrap();
    let entry = CatalogEntry {
        app: App {
            id: "sample".into(),
            title: "Sample".into(),
            ui_ref: ui.ui_ref,
            state_ref: "app:sample/state".into(),
            status: "canvas-app".into(),
            dependencies: vec![],
        },
        path: "sample".into(),
        private: false,
        ui_hash: canvas::hash(&bytes),
    };
    let mut tree = bundle::pack(&entry, 1, &bytes, None).unwrap();
    let signature = key
        .sign(&bundle::signing_payload(&bundle::inspect(&tree).unwrap().digest, &public).unwrap());
    bundle::attach_signature(&mut tree, &public, signature.as_ref()).unwrap();
    (archive::encode(archive::Kind::App, &tree).unwrap(), public)
}
fn staged(store: &Store) -> (StagedApp, AppPolicy, [u8; 32]) {
    let (bytes, key) = code();
    let registry = store.registry().unwrap();
    let target = AppImportTarget {
        space: registry.spaces[0].id.clone(),
        generation: registry.generation,
        catalog: vec![1; 32],
        release: "a".repeat(64),
        audience: Audience::Closed,
    };
    let stage = AppCandidate::qualify(&bytes, &key, None)
        .unwrap()
        .admit(
            target,
            registry,
            "alice",
            &HostPolicy::new(vec!["alice".into()]).unwrap(),
            None,
            100,
        )
        .unwrap();
    let policy = stage.standalone_policy().unwrap();
    (stage, policy, key)
}
fn authority<'a>(host: &'a HostPolicy, now: u64) -> InstallAuthority<'a> {
    InstallAuthority {
        host,
        catalog: &[1; 32],
        release: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        now,
    }
}
#[test]
fn queued_app_install_revalidates_delayed_authority_and_preserves_current_metadata() {
    let f = super::super::tests::Fixture::new();
    let (mut store, _) = f.seeded();
    let (stage, policy, _) = staged(&store);
    let host = HostPolicy::new(vec!["alice".into()]).unwrap();
    let before = store.registry().unwrap().clone();
    let status = store
        .queue_app_import("alice", stage, &policy, &authority(&host, 101))
        .unwrap();
    assert_eq!(status.phase, Phase::Queued);
    assert_eq!(store.registry().unwrap(), &before);
    assert_eq!(
        store.app_import_status("bob", &status.operation),
        Err(Error::Denied)
    );
    let mut revoked = authority(&host, 102);
    let no_operators = HostPolicy::default();
    revoked.host = &no_operators;
    assert_eq!(
        store.arm_app_import(&status.operation, &"b".repeat(64), &revoked),
        Err(Error::Denied)
    );
    assert_eq!(
        store
            .operator_app_import_state(&status.operation)
            .unwrap()
            .unwrap()
            .phase,
        Phase::Rejected
    );
    assert_eq!(store.registry().unwrap(), &before);
    let (stage, policy, _) = staged(&store);
    let queued = store
        .queue_app_import("alice", stage, &policy, &authority(&host, 103))
        .unwrap();
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: before.generation,
                change: Change::ConfigureSpace {
                    space: before.spaces[0].id.clone(),
                    name: "Newer name".into(),
                    layout: before.spaces[0].layout,
                },
            },
            None,
        )
        .unwrap();
    assert_eq!(
        store.arm_app_import(&queued.operation, &"b".repeat(64), &authority(&host, 104)),
        Err(Error::Conflict)
    );
    assert_eq!(
        store
            .operator_app_import_state(&queued.operation)
            .unwrap()
            .unwrap()
            .phase,
        Phase::Rejected
    );
    assert_eq!(store.registry().unwrap().spaces[0].name, "Newer name");
}
#[test]
fn armed_install_survives_restart_fences_management_and_atomically_attaches_only_exact_code() {
    let f = super::super::tests::Fixture::new();
    let (mut store, binding) = f.seeded();
    let (stage, policy, key) = staged(&store);
    let host = HostPolicy::new(vec!["alice".into()]).unwrap();
    let before = store.registry().unwrap().clone();
    let queued = store
        .queue_app_import("alice", stage, &policy, &authority(&host, 101))
        .unwrap();
    let archive = store.app_install_archive(&queued.operation, &key).unwrap();
    assert!(
        store
            .app_install_archive(&queued.operation, &[0; 32])
            .is_err()
    );
    let digest = store.pending_app_install().unwrap().unwrap().review.digest;
    let armed = store
        .arm_app_import(&queued.operation, &"b".repeat(64), &authority(&host, 102))
        .unwrap();
    assert_eq!(armed.phase, Phase::Armed);
    assert_eq!(
        store.commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: before.generation,
                change: Change::Remove {
                    space: binding.space.clone(),
                    app: binding.app.clone()
                }
            },
            None
        ),
        Err(Error::Unavailable)
    );
    authorize(
        store.registry().unwrap(),
        Some("alice"),
        &binding,
        Operation::Edit,
    )
    .unwrap();
    assert!(
        store
            .finish_app_import(&"a".repeat(64), &digest)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.finish_app_import(&"c".repeat(64), &digest),
        Err(Error::Conflict)
    );
    assert_eq!(
        store.finish_app_import(&"b".repeat(64), &"0".repeat(64)),
        Err(Error::Conflict)
    );
    drop(store);
    let mut store = Store::open(f.path()).unwrap();
    assert_eq!(store.pending_app_install().unwrap().unwrap().state, armed);
    assert_eq!(
        store.app_install_archive(&queued.operation, &key).unwrap(),
        archive
    );
    let applied = store
        .finish_app_import(&"b".repeat(64), &digest)
        .unwrap()
        .unwrap();
    assert_eq!(applied.phase, Phase::Applied);
    assert_eq!(store.registry().unwrap().generation, before.generation + 1);
    assert_eq!(
        store.registry().unwrap().spaces[0].apps[0],
        before.spaces[0].apps[0]
    );
    let imported = store.registry().unwrap().spaces[0].apps[1].clone();
    assert_eq!(imported.policy, policy);
    assert!(imported.attached);
    assert_ne!(imported.collection, binding.collection);
    assert!(
        store
            .finish_app_import(&"b".repeat(64), &digest)
            .unwrap()
            .is_none()
    );
    drop(store);
    let mut store = Store::open(f.path()).unwrap();
    assert_eq!(store.registry().unwrap().spaces[0].apps[1], imported);
    assert_eq!(
        store.app_import_status("alice", &queued.operation).unwrap(),
        Some(applied)
    );
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: before.generation + 1,
                change: Change::LabelApp {
                    space: binding.space,
                    app: "sample".into(),
                    name: "Installed name".into(),
                },
            },
            None,
        )
        .unwrap();
}
#[test]
fn install_latest_decision_is_authoritative_after_real_process_exit_before_primary() {
    const CHILD: &str = "OTRIOS_INSTALL_CRASH_ROOT";
    if let Ok(root) = std::env::var(CHILD) {
        let mut store = Store::open(PathBuf::from(root).join("registry.flex")).unwrap();
        let (stage, policy, _) = staged(&store);
        let host = HostPolicy::new(vec!["alice".into()]).unwrap();
        let queued = store
            .queue_app_import("alice", stage, &policy, &authority(&host, 101))
            .unwrap();
        store
            .arm_app_import(&queued.operation, &"b".repeat(64), &authority(&host, 102))
            .unwrap();
        let install = store.current.imports.last().unwrap().clone();
        let mut next = store.current.clone();
        next.registry = attached_registry(&store.current.registry, &install).unwrap();
        next.imports.last_mut().unwrap().phase = Phase::Applied;
        store
            .publish(next, |step| {
                if step == Boundary::Latest(FileStep::DirectorySynced) {
                    std::process::exit(73);
                }
            })
            .unwrap();
        panic!("did not exit");
    }
    let f = super::super::tests::Fixture::new();
    let (store, binding) = f.seeded();
    let before = store.registry().unwrap().clone();
    drop(store);
    let output = Command::new(std::env::current_exe().unwrap()).arg("--exact").arg("spaces::store::app_imports::tests::install_latest_decision_is_authoritative_after_real_process_exit_before_primary").arg("--nocapture").env(CHILD, &f.0).stdout(Stdio::piped()).stderr(Stdio::piped()).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(73),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let store = Store::open(f.path()).unwrap();
    assert_eq!(store.registry().unwrap().generation, before.generation + 1);
    assert_eq!(
        store.registry().unwrap().spaces[0].apps[0].collection,
        binding.collection
    );
    assert_eq!(
        store.registry().unwrap().spaces[0].apps[1].policy.app,
        "sample"
    );
    assert_eq!(store.current.imports.last().unwrap().phase, Phase::Applied);
}

#[test]
fn completed_import_receipts_are_bounded_and_eviction_returns_unknown_without_replay() {
    let f = super::super::tests::Fixture::new();
    let (mut store, _) = f.seeded();
    let host = HostPolicy::new(vec!["alice".into()]).unwrap();
    let mut first = None;
    for _ in 0..70 {
        let (stage, policy, _) = staged(&store);
        let queued = store
            .queue_app_import("alice", stage, &policy, &authority(&host, 101))
            .unwrap();
        first.get_or_insert(queued.operation.clone());
        store.cancel_app_import("alice", &queued.operation).unwrap();
    }
    assert_eq!(store.current.imports.len(), 64);
    assert_eq!(
        store
            .app_import_status("alice", first.as_ref().unwrap())
            .unwrap(),
        None
    );
    let last = store.current.imports.last().unwrap().state();
    drop(store);
    let store = Store::open(f.path()).unwrap();
    assert_eq!(store.current.imports.len(), 64);
    assert_eq!(
        store.app_import_status("alice", &last.operation).unwrap(),
        Some(last)
    );
    assert!(store.pending_app_install().unwrap().is_none());
}

#[test]
fn installed_code_reuse_needs_no_signer_and_reattaches_the_exact_confirmed_dataset() {
    let f = super::super::tests::Fixture::new();
    let (mut store, _) = f.seeded();
    let (bytes, key) = code();
    let previous =
        bundle::verify(archive::decode(archive::Kind::App, &bytes).unwrap(), &key).unwrap();
    let host = HostPolicy::default();
    let stage = |store: &Store| {
        let registry = store.registry().unwrap();
        let target = AppImportTarget {
            space: registry.spaces[0].id.clone(),
            generation: registry.generation,
            catalog: vec![1; 32],
            release: "a".repeat(64),
            audience: Audience::Closed,
        };
        AppCandidate::qualify(&bytes, &key, None)
            .unwrap()
            .admit(target, registry, "alice", &host, Some(&previous), 100)
            .unwrap()
    };
    let first = stage(&store);
    let policy = first.standalone_policy().unwrap();
    let outcome = store
        .queue_app_import("alice", first, &policy, &authority(&host, 101))
        .unwrap();
    assert_eq!(outcome.phase, Phase::Applied);
    assert!(store.pending_app_install().unwrap().is_none());
    assert!(!suffix(&store.path, ".app-import.zip").exists());
    let app = store.registry().unwrap().spaces[0].apps[1].clone();
    let data = f
        .storage()
        .join(format!("collection-{}.merkle", app.collection));
    fs::write(&data, b"confirmed data remains byte-identical").unwrap();
    let registry = store.registry().unwrap();
    store
        .commit(
            "alice",
            &Request {
                version: VERSION,
                expected_generation: registry.generation,
                change: Change::Remove {
                    space: registry.spaces[0].id.clone(),
                    app: "sample".into(),
                },
            },
            None,
        )
        .unwrap();
    let next = stage(&store);
    store
        .queue_app_import("alice", next, &policy, &authority(&host, 101))
        .unwrap();
    let attached = &store.registry().unwrap().spaces[0].apps[1];
    assert!(attached.attached);
    assert_eq!(attached.collection, app.collection);
    assert!(attached.attachment_epoch > app.attachment_epoch);
    assert_eq!(
        fs::read(data).unwrap(),
        b"confirmed data remains byte-identical"
    );
    assert_eq!(
        store
            .app_import_status("alice", &outcome.operation)
            .unwrap(),
        Some(outcome)
    );
}
