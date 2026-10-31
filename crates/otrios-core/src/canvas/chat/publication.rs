// SPDX-License-Identifier: LGPL-3.0-only
//! Selected confirmed dispatches; publication never exports presence, drafts,
//! authority bindings or a reference to another collection's asset namespace.
#[cfg(not(target_arch = "wasm32"))]
use super::super::assets::Assets;
use super::*;
use crate::merkle::Hash;

impl Room {
    pub fn selected_message(&self, sequence: u64, expected: Hash) -> Result<Message> {
        if !self.healthy() {
            return Err("Room requires journal recovery".into());
        }
        let snapshot = self.history.snapshot()?;
        let row = snapshot.record(sequence).map_err(err)?;
        if row.hash() != expected {
            return Err("Selected dispatch has changed".into());
        }
        super::super::read_message(&row)
    }
    pub(super) fn redact_message(&mut self, sequence: u64, expected: Hash, now: u64) -> Result<()> {
        self.selected_message(sequence, expected)?;
        let prepared = self.history.prepare_removal(sequence, expected)?;
        self.persist(
            &Entry::Redacted {
                sequence,
                revision: expected,
            },
            now,
        )?;
        if let Err(error) = self.history.commit(prepared) {
            self.failed = true;
            for connection in self.connections.values() {
                connection.admission.store(false, Ordering::Release);
            }
            return Err(error);
        }
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_attachment(
        &self,
        sequence: u64,
        revision: Hash,
        index: u8,
        storage: &Assets,
    ) -> Result<(super::super::Asset, Vec<u8>)> {
        let message = self.selected_message(sequence, revision)?;
        let asset = message
            .assets
            .get(index as usize)
            .ok_or("Unknown dispatch attachment")?
            .clone();
        Ok((asset.clone(), storage.read(&asset)?))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct Publication<'a> {
    pub source: &'a otrios_interfaces::spaces::Binding,
    pub destination: &'a otrios_interfaces::spaces::Binding,
    pub actor: &'a str,
    pub sequence: u64,
    pub revision: Hash,
    pub epoch: u64,
    pub nonce: &'a str,
}
/// Caller serializes admission, both rooms and asset storage under its authority
/// lock and rechecks authenticated identity through delivery. The Space runtime's
/// publication_destination validates current source Read and destination Edit.
#[cfg(not(target_arch = "wasm32"))]
pub fn publish(
    source: &Room,
    destination: &mut Room,
    source_assets: &Assets,
    destination_assets: &Assets,
    request: Publication<'_>,
    now: u64,
) -> Result<u64> {
    if !crate::spaces::valid_identity(request.actor)
        || request.source.collection == request.destination.collection
        || request.source.app != request.destination.app
        || request.source.collection != source_assets.collection()
        || request.destination.collection != destination_assets.collection()
        || !destination.healthy()
    {
        return Err("Invalid publication ownership".into());
    }
    if !super::super::valid_hex(request.nonce, 16)
        || request.epoch > now / EPOCH
        || request.epoch.saturating_add(2).saturating_mul(EPOCH) <= now
    {
        return Err("Publication expired; review and publish again".into());
    }
    let selected = source.selected_message(request.sequence, request.revision)?;
    let digest = super::super::super::hash(
        &flexbuffers::to_vec((
            "otrios-publication-v1",
            request.source,
            request.destination,
            request.sequence,
            request.revision,
            &selected,
        ))
        .map_err(err)?,
    );
    let participant = format!("publication:{}", request.actor);
    let receipt_key = (participant.clone(), request.epoch, request.nonce.to_owned());
    if let Some(receipt) = destination.receipts.get(&receipt_key) {
        return if receipt.digest == digest {
            Ok(receipt.sequence)
        } else {
            Err("Publication nonce conflicts with its original selection".into())
        };
    }
    destination.prune(now);
    if destination.receipts.len() >= MAX_RECEIPTS {
        return Err("Publication receipt capacity".into());
    }
    let assets = selected
        .assets
        .iter()
        .map(|asset| source_assets.copy_to(destination_assets, asset))
        .collect::<Result<Vec<_>>>()?;
    let message = Message {
        sequence: destination
            .sequence
            .checked_add(1)
            .ok_or("Room sequence exhausted")?,
        at: now,
        sender: Profile {
            id: random(16)?,
            name: selected.sender.name,
            avatar: selected.sender.avatar,
        },
        kind: selected.kind,
        text: selected.text,
        nonce: request.nonce.into(),
        assets,
    };
    let receipt = Receipt {
        participant,
        epoch: request.epoch,
        nonce: request.nonce.into(),
        digest,
        sequence: message.sequence,
    };
    destination.persist_message(
        &message,
        &Entry::Sent {
            message: message.clone(),
            receipt: receipt.clone(),
        },
        now,
    )?;
    destination.receipts.insert(receipt_key, receipt);
    Ok(message.sequence)
}

#[cfg(not(target_arch = "wasm32"))]
impl Room {
    pub fn send_attachment(
        &mut self,
        connection: u64,
        command: Command,
        storage: &Assets,
        now: u64,
    ) -> Result<Vec<Delivery>> {
        let Command::SendAsset {
            epoch,
            nonce,
            text,
            name,
            bytes,
        } = command
        else {
            return Err("Expected an attachment command".into());
        };
        if bytes.is_empty() || bytes.len() > super::super::MAX_ASSET_BYTES {
            return Err("Attachment byte limit".into());
        }
        let asset = super::super::Asset {
            name: name.clone(),
            bytes: bytes.len() as u32,
            sha256: super::super::super::hex(&super::super::super::hash(&bytes)),
        };
        super::super::validate_asset(&asset)?;
        self.send_prepared(
            connection,
            PreparedSend {
                epoch,
                nonce,
                body: text,
                assets: vec![asset],
            },
            now,
            || {
                storage.put(&name, &bytes)?;
                Ok(())
            },
        )
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::fs;
    const NOW: u64 = 1_000_000;
    struct Fixture {
        root: PathBuf,
        source: Room,
        destination: Room,
        source_assets: Assets,
        destination_assets: Assets,
        source_binding: otrios_interfaces::spaces::Binding,
        destination_binding: otrios_interfaces::spaces::Binding,
    }
    fn binding(space: char, collection: char) -> otrios_interfaces::spaces::Binding {
        otrios_interfaces::spaces::Binding {
            version: 1,
            space: space.to_string().repeat(32),
            app: "brass-relay".into(),
            collection: collection.to_string().repeat(32),
            schema: "e".repeat(64),
            admission_epoch: 1,
            attachment_epoch: 1,
        }
    }
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("otrios-publication-{}", random(16).unwrap()));
            fs::create_dir(&root).unwrap();
            let source_binding = binding('a', 'c');
            let destination_binding = binding('b', 'd');
            let source_assets = Assets::new(&root, &source_binding.collection).unwrap();
            let destination_assets = Assets::new(&root, &destination_binding.collection).unwrap();
            let mut source = Room::open(
                Some(root.join(format!(
                    "collection-{}.data/chat-primary.journal",
                    source_binding.collection
                ))),
                NOW,
            )
            .unwrap();
            let destination = Room::open(
                Some(root.join(format!(
                    "collection-{}.data/chat-primary.journal",
                    destination_binding.collection
                ))),
                NOW,
            )
            .unwrap();
            source
                .join_admitted(1, "private-source-authority-identity", true, NOW)
                .unwrap();
            source
                .send_attachment(
                    1,
                    Command::SendAsset {
                        epoch: NOW / EPOCH,
                        nonce: "f".repeat(32),
                        text: "Selected private dispatch".into(),
                        name: "diagram.bin".into(),
                        bytes: vec![0, 1, 255, 42],
                    },
                    &source_assets,
                    NOW,
                )
                .unwrap();
            Self {
                root,
                source,
                destination,
                source_assets,
                destination_assets,
                source_binding,
                destination_binding,
            }
        }
        fn revision(&self, sequence: u64) -> Hash {
            self.source
                .history
                .snapshot()
                .unwrap()
                .record(sequence)
                .unwrap()
                .hash()
        }
        fn copy(&mut self, sequence: u64, nonce: &str) -> Result<u64> {
            let revision = self.revision(sequence);
            publish(
                &self.source,
                &mut self.destination,
                &self.source_assets,
                &self.destination_assets,
                Publication {
                    source: &self.source_binding,
                    destination: &self.destination_binding,
                    actor: "publisher",
                    sequence,
                    revision,
                    epoch: NOW / EPOCH,
                    nonce,
                },
                NOW,
            )
        }
        fn finish(self) {
            let Self {
                root,
                source,
                destination,
                ..
            } = self;
            drop(source);
            drop(destination);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn selected_publication_copies_owned_bytes_without_private_links_and_survives_source_deletion()
    {
        let mut fixture = Fixture::new();
        let nonce = "1".repeat(32);
        assert_eq!(fixture.copy(1, &nonce).unwrap(), 1);
        let row = fixture
            .destination
            .history
            .snapshot()
            .unwrap()
            .record(1)
            .unwrap();
        let copy = fixture.destination.selected_message(1, row.hash()).unwrap();
        let source = fixture
            .source
            .selected_message(1, fixture.revision(1))
            .unwrap();
        assert_eq!(copy.text, source.text);
        assert_eq!(copy.assets, source.assets);
        assert_ne!(copy.sender.id, source.sender.id);
        assert_ne!(copy.nonce, source.nonce);
        let public = serde_json::to_string(&copy).unwrap();
        for private in [
            &fixture.source_binding.collection,
            &fixture.source_binding.space,
            &source.sender.id,
            &source.nonce,
            "private-source-authority-identity",
        ] {
            assert!(!public.contains(private));
        }
        fixture
            .source
            .handle(
                1,
                Command::Profile {
                    name: "Changed private profile".into(),
                    avatar: 7,
                },
                NOW,
            )
            .unwrap();
        fixture
            .source
            .handle(
                1,
                Command::Send {
                    epoch: NOW / EPOCH,
                    nonce: "2".repeat(32),
                    text: "Later private change".into(),
                },
                NOW,
            )
            .unwrap();
        assert_eq!(
            fixture.destination.selected_message(1, row.hash()).unwrap(),
            copy
        );
        assert_eq!(fixture.destination.history.snapshot().unwrap().len(), 1);
        // Updating the destination requires another explicit publication.
        assert_eq!(fixture.copy(2, &"3".repeat(32)).unwrap(), 2);
        let Fixture {
            root,
            source,
            destination,
            destination_assets,
            source_binding,
            destination_binding,
            ..
        } = fixture;
        drop(source);
        drop(destination);
        fs::remove_dir_all(root.join(format!("collection-{}.data", source_binding.collection)))
            .unwrap();
        let reopened = Room::open(
            Some(root.join(format!(
                "collection-{}.data/chat-primary.journal",
                destination_binding.collection
            ))),
            NOW,
        )
        .unwrap();
        assert_eq!(reopened.selected_message(1, row.hash()).unwrap(), copy);
        assert_eq!(
            reopened
                .read_attachment(1, row.hash(), 0, &destination_assets)
                .unwrap()
                .1,
            [0, 1, 255, 42]
        );
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retries_are_durable_and_conflicting_or_stale_selections_cannot_publish() {
        let mut fixture = Fixture::new();
        let nonce = "4".repeat(32);
        fixture.copy(1, &nonce).unwrap();
        assert_eq!(fixture.copy(1, &nonce).unwrap(), 1);
        fixture
            .source
            .handle(
                1,
                Command::Send {
                    epoch: NOW / EPOCH,
                    nonce: "5".repeat(32),
                    text: "Different selection".into(),
                },
                NOW,
            )
            .unwrap();
        assert!(fixture.copy(2, &nonce).is_err());
        let revision = fixture.revision(1);
        fixture
            .source
            .handle(
                1,
                Command::Redact {
                    sequence: 1,
                    revision,
                },
                NOW,
            )
            .unwrap();
        assert!(
            publish(
                &fixture.source,
                &mut fixture.destination,
                &fixture.source_assets,
                &fixture.destination_assets,
                Publication {
                    source: &fixture.source_binding,
                    destination: &fixture.destination_binding,
                    actor: "publisher",
                    sequence: 1,
                    revision,
                    epoch: NOW / EPOCH,
                    nonce: &"6".repeat(32)
                },
                NOW
            )
            .is_err()
        );
        assert_eq!(fixture.destination.history.snapshot().unwrap().len(), 1);
        fixture.finish();
    }
    #[test]
    fn moderation_preserves_sequence_receipts_and_pins_without_resurrecting_removed_messages() {
        let mut fixture = Fixture::new();
        fixture.copy(1, &"7".repeat(32)).unwrap();
        let pinned = fixture.destination.history.snapshot().unwrap();
        let revision = pinned.record(1).unwrap().hash();
        fixture
            .destination
            .join_admitted(2, "visitor", false, NOW)
            .unwrap();
        let command = Command::Redact {
            sequence: 1,
            revision,
        };
        assert!(fixture.destination.handle(2, command.clone(), NOW).is_err());
        fixture.destination.set_admin(2, true);
        fixture.destination.handle(2, command.clone(), NOW).unwrap();
        assert!(fixture.destination.handle(2, command, NOW).is_err());
        assert!(fixture.destination.history.snapshot().unwrap().is_empty());
        assert!(
            fixture
                .destination
                .read_attachment(1, revision, 0, &fixture.destination_assets)
                .is_err()
        );
        assert!(
            pinned.record(1).is_ok(),
            "a previously admitted immutable pin remains bounded"
        );
        // Repeating a committed publication nonce returns its receipt, never a new row.
        assert_eq!(fixture.copy(1, &"7".repeat(32)).unwrap(), 1);
        assert!(fixture.destination.history.snapshot().unwrap().is_empty());
        fixture.destination.force_compaction();
        fixture
            .destination
            .handle(
                2,
                Command::Send {
                    epoch: NOW / EPOCH,
                    nonce: "8".repeat(32),
                    text: "After removal".into(),
                },
                NOW,
            )
            .unwrap();
        assert_eq!(fixture.destination.sequence, 2);
        let Fixture {
            root,
            source,
            destination,
            destination_binding,
            ..
        } = fixture;
        drop(source);
        drop(destination);
        let recovered = Room::open(
            Some(root.join(format!(
                "collection-{}.data/chat-primary.journal",
                destination_binding.collection
            ))),
            NOW,
        )
        .unwrap();
        assert!(recovered.history.snapshot().unwrap().record(1).is_err());
        assert_eq!(recovered.sequence, 2);
        assert!(recovered.history.snapshot().unwrap().record(2).is_ok());
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn failed_asset_integrity_and_uncertain_journal_do_not_publish_partial_content() {
        let mut fixture = Fixture::new();
        let message = fixture
            .source
            .selected_message(1, fixture.revision(1))
            .unwrap();
        let blob = fixture.root.join(format!(
            "collection-{}.data/asset-{}.blob",
            fixture.source_binding.collection, message.assets[0].sha256
        ));
        fs::write(&blob, [0, 1, 254, 42]).unwrap();
        assert!(fixture.copy(1, &"9".repeat(32)).is_err());
        assert!(fixture.destination.history.snapshot().unwrap().is_empty());
        fs::write(&blob, [0, 1, 255, 42]).unwrap();
        // Model a crash after owned bytes are durable but before their message.
        fixture
            .source_assets
            .copy_to(&fixture.destination_assets, &message.assets[0])
            .unwrap();
        fixture.destination.fail_journal_writes();
        assert!(fixture.copy(1, &"a".repeat(32)).is_err());
        assert!(fixture.destination.history.snapshot().unwrap().is_empty());
        assert!(!fixture.destination.healthy());
        assert!(
            fixture.destination_assets.read(&message.assets[0]).is_ok(),
            "bounded orphan bytes convey no readable message reference"
        );
        fixture.finish();
    }
    #[test]
    fn legacy_journal_upgrade_is_atomic_and_preserves_confirmed_dispatches() {
        let mut fixture = Fixture::new();
        // An old message without attachments has the exact old serialized shape.
        let path = fixture.root.join(format!(
            "collection-{}.data/chat-primary.journal",
            fixture.destination_binding.collection
        ));
        fixture
            .destination
            .join_admitted(2, "member", true, NOW)
            .unwrap();
        fixture
            .destination
            .handle(
                2,
                Command::Send {
                    epoch: NOW / EPOCH,
                    nonce: "b".repeat(32),
                    text: "Confirmed legacy dispatch".into(),
                },
                NOW,
            )
            .unwrap();
        let Fixture {
            root,
            source,
            destination,
            ..
        } = fixture;
        drop(source);
        drop(destination);
        let mut bytes = fs::read(&path).unwrap();
        bytes[..8].copy_from_slice(b"OTCHAT01");
        fs::write(&path, &bytes).unwrap();
        let mut recovered = Room::open(Some(path.clone()), NOW).unwrap();
        assert!(recovered.journal.as_ref().unwrap().legacy);
        recovered.join_admitted(3, "member", true, NOW).unwrap();
        assert_eq!(&fs::read(&path).unwrap()[..8], b"OTCHAT01");
        recovered
            .handle(
                3,
                Command::Send {
                    epoch: NOW / EPOCH,
                    nonce: "c".repeat(32),
                    text: "Confirmed new dispatch".into(),
                },
                NOW,
            )
            .unwrap();
        assert_eq!(&fs::read(&path).unwrap()[..8], b"OTCHAT02");
        assert!(!recovered.journal.as_ref().unwrap().legacy);
        drop(recovered);
        let recovered = Room::open(Some(path), NOW).unwrap();
        let messages = recovered
            .history
            .snapshot()
            .unwrap()
            .records()
            .map(|(_, row)| super::super::super::read_message(row).unwrap().text)
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            ["Confirmed legacy dispatch", "Confirmed new dispatch"]
        );
        drop(recovered);
        fs::remove_dir_all(root).unwrap();
    }
}
