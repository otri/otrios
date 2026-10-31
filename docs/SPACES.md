<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces management contract

The covered `otrios_core::spaces` library implements registry transitions and
admission. The authority additionally supports an explicit managed mode via
`--spaces-registry /ABS/PRIVATE/spaces.flex`. The live Frame has not been cut over.
Legacy mode remains for state directories that have never been bound to Spaces;
it does not implement Space admission. Managed mode requires durable app storage.
Its private `state.apps/spaces.registry` marker records the registry path. Later
starts recover that binding even if the CLI flag is omitted, and reject attempts
to substitute another registry. Do not remove that marker to roll back code.

In managed mode, `/spaces` lists admitted Spaces and accepts versioned management
requests with an allowed Origin and `X-OTriOS-Request: spaces`. It never returns
closed Space records to strangers, or membership lists to ordinary Members.
Clients select a Space with `/session?space=SPACE_ID`. The authority resolves
the installed app's admitted collection, scopes its checkpoint/journal and writer
receipts, and checks reads and writes against the current binding. Membership
changes revoke staged/active mounts, and queued payloads revalidate under the same
management lock before delivery. Native loopback grants no managed access.

State protocol `otrios-canvas-sync-v11` includes a version-1 presentation binding
in each managed mount: collection identity and effective edit/comment/moderation
actions. These describe authority decisions and are never credentials. A client
resumes pending RAM intents only when both collection identity and effective
actions match its previous mount. A replacement collection starts at receipt
sequence zero; identical app and state references cannot restore old edits into
it. Read admission does not allocate durable writer receipts even for editable
schemas. The authority still checks every write independently.

Aggregate presentations accompany the initial mount, so a managed candidate is
qualified with its first usable view before acceptance. A missing or invalid
view rejects the candidate and retains the previous activation. The release
control v1 opening envelope remains unchanged across this protocol upgrade.

A qualified managed candidate stays invisible until the authority replies to
AcceptMount with Activated under current policy. Revocation between qualification
and acceptance clears the staged candidate and cannot produce that acknowledgement.
Queued activation replies revalidate before delivery. The previous view remains
visible while qualification/activation is pending; denied, disconnected or locally
cancelled candidates are discarded. RAM edits made in a reconnecting view during
the round trip are restored only into the same collection with the same actions.
Legacy/local mounts retain their existing handshake until explicit cutover.

## Discovery on the shared session

Management/discovery v1 uses channel 5 of the same negotiated session. Both hosts
use the covered RAM-only observer; it sends Watch after each reconnect. The
browser/native state protocol is v11; v10 introduced discovery, and v11 adds the
managed activation acknowledgement.
Release discovery remains on its unchanged control v1 envelope.

Watch selects metadata for one Space and includes the previous authority/revision
cursor where available. Every Watch explicitly replaces the snapshot through
resync. Observation is registered before initial capture. The
authority publishes catalog and management revisions under its admission lock,
then emits complete, filtered snapshots. A changed incarnation or skipped revision
explicitly marks resync. Mailbox overflow requires resync; no unbounded queue or
second WebSocket is introduced. Snapshot delivery rechecks current identity and
revision under the management lock, preventing an older queued roster from being
delivered after revocation acknowledges.

Snapshots contain up to 64 visible Space summaries and metadata for the selected
Space only: at most 128 app rows, 128 Admin-visible member records and 16 verified
installed launch records. They never contain document payloads or credentials.
Retained collection rows, storage totals and installed-app management metadata
are Admin-only. Storage totals cover managed checkpoints, journals and aggregate
presentations; unreadable totals remain unknown. Available metadata is not a
completed launch: state, code and first-view qualification still precede acceptance.
An unavailable or schema-incompatible app keeps its collection identity.

Commands are at most 8 KiB; events are at most 128 KiB, with structural bounds
before FlexBuffers deserialization. Only one management intent is outstanding per
host observer. A lost reply/reconnect marks its outcome unknown and refreshes the
snapshot; it never automatically replays Create or a destructive action. Every
management path uses the same generation/role checks.

The shared native/browser Home includes a Space picker and launch entries. Admins
can use Apps & Data to inspect attached/retained collections, add/remove apps and
manage members. Local forms use the shared keyboard; their drafts never enter app
state. These are trusted host controls with a private action table: a publisher
cannot invoke them by reusing labels or event numbers. A changed admission snapshot
redacts held management frames before renderer credit returns.

Quit waits for connected edits to be confirmed and gestures to finish, then leaves
that participant's activation and returns to Home. Removing an attachment durably
closes its admission and drops healthy cached rooms/presentations while retaining
checkpoints, journals and the collection identity. Other Spaces are unaffected.
Admin-visible retained data can be reattached to recover its original values,
including after authority restart. A failed durable room stays poisoned rather
than being reopened as an empty collection. Active and staged participants pin
their rooms. At the 256-room cache limit, a new mount may evict only a healthy,
durable room with no participants; revisiting it recovers its checkpoint/journal.
Leaving releases the pin, while an uncertain persistence outcome remains poisoned
until restart even if the underlying filesystem becomes writable again.
These controls do not claim local
uninstall, complete auxiliary-lane lifecycle or physical Frame qualification.

This increment admits Create, Membership, Attach, Remove and Closed audience changes.
Deletion and public audience changes remain unavailable through HTTP until their
lifecycle integrations are complete. Global chat/collection/ink lanes
and legacy presentation reads are disabled in managed mode; they cannot be used
as an unscoped fallback. Monitoring presentation uploads require an explicit
collector identity and collection grant.

Separate child clients exercise authority reads/writes, collection isolation,
explicit Frame access, active revocation/reconnect, Member management denial and
concurrent last-Admin protection. These are authority adapter checks, not Frame or
browser qualification. `scripts/check.sh` supplies its freshly built authority to
`otrios-core/tests/spaces_process.rs`. Its test-only `tungstenite` dependency reuses
the pinned workspace WebSocket client; it adds no runtime dependency.

For real-browser qualification, build the authority, `bundle_counter` example and
Wasm package from the tested revision, then run
`python3 scripts/qualify-spaces-browser.py --wasm-pkg /ABS/BUILT/pkg`. The probe uses
disposable data and signing keys, normal sign-in and separate Chromium cookie
contexts. It checks shared edits, Space isolation, anonymous/non-Member denial,
active revocation and continued use by an unaffected Member. It also exercises
the host controls for Quit, retained-data inspection, removal, reattachment and
Space switching, verifying that another participant can edit after Quit and that
reattachment preserves the collection and value. Reports include the
source revision, dirty-tree status and artifact digests. The fixture has no native
artifact; it does not establish ARM execution or Frame deployment acceptance.

## Identity configuration

The mode-0600 authority auth file additionally accepts version 1 with up to 64
`users` records (`username`, `identity`, `password_hash`) and 64 `native` records
(`identity`, `token_sha256`). Passwords use the existing bounded Argon2id verifier;
native tokens are random 256-bit lowercase hex credentials whose SHA-256 digests
are provisioned in this file. Never put plaintext tokens in a bundle, registry,
release, command-line argument or evidence report.

Browser sessions retain their authenticated identity until expiry or logout.
Native requests use an Authorization Bearer header; native credentials remain
valid until removed from configuration and the authority restarted. Space
membership is checked independently and can be revoked without changing a
credential. Multiple valid credential sources in one request are rejected. Existing
single-login files remain readable, but authenticating a legacy login confers no
Space membership or Admin role.

The native host accepts `OTRIOS_SPACE` and a mode-0600 credential file selected by
`OTRIOS_IDENTITY_FILE`. It sends the token in an Authorization header, never in a
URL or state message. A Space query in the configured session URL is also accepted;
specify the Space exactly once. The browser forwards the selected `space` query
to its shared session owner. Neither host opens an extra state connection.

## Offline bootstrap and migration

Stop the existing authority before provisioning; never run two authorities over
the same state. Use the matching verified release, publisher public key, durable
app directory and a private, explicit version-1 provisioning manifest:

```json
{
  "version": 1,
  "spaces": [
    {
      "name": "Household",
      "admins": ["named-admin"],
      "members": ["frame-household"],
      "apps": [{"id": "home", "legacy_state_ref": "app:home/state"}]
    },
    {
      "name": "Operations",
      "admins": ["named-admin"],
      "members": [],
      "apps": [{"id": "monitoring", "legacy_state_ref": "app:monitoring/state"}]
    }
  ],
  "services": [{
    "identity": "monitoring-collector",
    "space": "Operations",
    "app": "monitoring",
    "output": "/ABS/PRIVATE/collector-binding.json"
  }]
}
```

Declare every desired root deliberately. The example does not grant the Frame
Operations access or infer membership from the old shared login. Provision named
human credentials and separate Frame/collector tokens in the auth file.

Run the authority with its normal `--releases`, `--bundle-key`, `--app-state` and
`--presentation-file` arguments, plus `--spaces-registry /ABS/PRIVATE/spaces.flex`
and `--provision-spaces /ABS/PRIVATE/provision.json`. This one-shot command binds
no listeners. It verifies the installed app catalog, creates explicitly named
Spaces/roles/attachments, imports each chosen legacy root after journal recovery,
and syncs the complete snapshot including confirmed writer receipts. An existing
different target root is rejected instead of overwritten. Monitoring's current
complete presentation is copied into its admitted collection where available.

A durable pending marker closes admission throughout provisioning. Failed or
interrupted provisioning must resume the same manifest. Completion records the
manifest digest, generated Space/collection IDs and source root digests before
clearing the marker. Rerunning a completed manifest returns that record without
reapplying initial memberships, so it cannot silently undo a later revocation.
Preserve the original roots and operational rollback snapshot through validation;
their later cleanup must follow the deletion/backup policy.

Configure the authority with `--collector-binding /ABS/PRIVATE/collector-binding.json`
and set the collector's private `authority_identity` token-file path. The service
grant admits only uploads to its configured Monitoring collection. It grants no
human membership, view admission or sibling collection access. Ordinary human
Admin credentials cannot upload the view. Membership changes do not invent or
revoke a service role; removal or collection replacement invalidates its grant.
Configured service identities cannot create Spaces or acquire human membership;
binding a service identity already present in a human roster is rejected.
Current installed schema/closed-only constraints also apply to cached bindings.

Monitoring views are stored as `collection-ID.presentation.flex` beside that
collection's checkpoint/journal. A mount with no usable presentation remains
unavailable. Presentation uploads never follow redirects with credentials. A
collector grant must be explicitly rebound after attachment or collection identity
changes; a stale producer cannot recreate deleted data.

## Management v1

The public types live in `otrios_interfaces::spaces`. A management request has
`version: 1`, `expected_generation`, and a tagged `change`. Changes are Create,
Membership, Audience, Attach, Remove, and DeleteData. Authentication supplies the
actor independently of the request. A verified host supplies the installed app
policy for Attach; the request cannot choose its schema or privileges.

An app collection may contain up to 16 distinct state roots. The collection
schema fingerprint hashes the canonical `otrios-space-app-schema-v1` map of
owned state references and complete encoded state schemas, bounded to 512 KiB.
Screen order and additional screens sharing the same root do not change it.
Changing any root reference/schema makes the retained collection incompatible;
it never admits an empty replacement. Pre-release registries using the earlier
single-document fingerprint remain unavailable until deliberately migrated.

Every admitted screen receives its owner's collection binding and opens only
its declared document inside it. The primary document retains the existing
`collection-ID.merkle` checkpoint/journal location. Other owned documents use
`collection-ID.data/doc-SHA256_OF_STATE_REF.merkle` and matching journals. Writer
receipts and subscriptions remain separate per document. Private data stays
outside installed code; document filenames confer no access. A retained journal
without its checkpoint is unavailable rather than overwritten with empty state.

Offline provisioning accepts `legacy_roots: [STATE_REF, ...]` for apps with
several owned documents, as an alternative to `legacy_state_ref`. Each source
must belong to that verified app; duplicates and mixed declarations are rejected.
Every declared confirmed root is recovered, copied and recorded under the same
destination collection before the provisioning marker permits admission. Include
every existing root that the rollout intends to retain; undeclared roots are not
silently inferred from legacy references.

Create makes a Closed Space with the authenticated actor as its first Admin.
Every other change requires current Admin membership. The host serializes requests
under one authority lock. An expected-generation mismatch rejects the entire
request. Membership replacement cannot remove the last Admin. An invalid or
denied candidate never changes the published registry.

Space and collection IDs are random 128-bit lowercase hex strings, generated by
the authority. App IDs use the existing admitted destination syntax. A schema
identity is the SHA-256 digest of the complete canonical collection schema.
Do not use a display name as a schema compatibility test.

Limits: 64 Spaces, 128 members and 128 app collections per Space, 96 bytes per
identity, 128 UTF-8 bytes per Space name, 4,096 deletion records, and 2 MiB of
encoded registry payload. Capacity exhaustion rejects the change. Deletion
records never expire in v1; exhaustion must not discard stale-reference protection.

## Bindings and lifetime

Admission resolves a binding containing version, Space ID, app ID, collection ID,
schema digest, Space admission epoch and attachment epoch. It is a scope reference,
not a bearer credential. The host revalidates the authenticated identity and the
binding before delivering payloads and before accepting operations. App execution
receives only its own admitted binding. Management and credential records must
not appear in app content or public discovery.

Membership/audience changes advance the Space admission epoch. Removal,
reattachment and collection replacement advance the attachment epoch. Old mounts,
queued packets and subscriptions must revalidate these epochs. Reattachment reuses
the compatible retained collection, while requiring a fresh binding. Quit and
device uninstall do not mutate this registry. Schema incompatibility rejects
reattachment without changing the retained collection.

Members retain the generation at which their continuous membership began. A
membership change revokes the removed participant while leaving uninterrupted
Members' mounts usable. Removing and readmitting someone starts a new interval;
their earlier mount cannot reactivate. Older registry members default to interval
zero on recovery; live bindings are RAM-only and always renewed after restart.

Read, Edit, Comment and Moderate are distinct runtime operations. Public read
does not imply editing. Visitor comments require a verified identity and public
comment policy; moderation requires membership. An app can disallow editing or
comments. An attached closed-only app prevents a public audience, and a public
Space rejects attachment of a closed-only app.

## Persistence

The native Store writes `OTSP2`, a 32-byte SHA-256 payload digest, and a FlexBuffers
Registry. Collections are encoded as bounded vectors, not unbounded map keys.
Recovery checks byte bounds, digest, structural nesting/visit bounds, version,
unique IDs, last-Admin invariants and epochs before admitting any data. The digest
detects corruption; it does not authenticate an operator or grant access.

Recovery also accepts the initial `OTSP1` prototype, defaulting older members'
continuous-membership generation to zero. The next commit writes `OTSP2`; older
prototype writers reject it. Rollback must use a reader/writer that supports this
framing and the current admission rules. Management request version 1 is unchanged.

A commit builds a private candidate, writes a mode-0600 temporary file, syncs it,
renames it over the registry and syncs the containing directory before publication.
An uncertain persistence result closes registry admission until restart. One
authority process owns the state directory; concurrent authority processes are
unsupported, as with the existing app journals.

DeleteData publishes the old collection's tombstone and a fresh replacement ID in
one registry commit. This is only the management decision: host producer shutdown,
payload reclamation, backup restore protection and client invalidation still need
integration before permanent deletion can be exposed as complete.

## Rollout requirements

Provision named human identities and the bootstrap Admin explicitly. A legacy
shared login, device administration or trusted listener cannot grant membership.
Provision the Frame as a separate authenticated identity with explicit Space
memberships. Bind Monitoring's collector to its intended Operations collection
as a service identity; human Monitoring views remain read-only.

Before live cutover, record each legacy root's destination Space and collection,
preserve its checkpoint/journal and confirmed receipts, and validate recovery in a
staging copy. Advance the state protocol when the wire format changes while
preserving release-control v1. Rollback must retain registry/deletion decisions
and may use only code that enforces them. Backup restoration must reconcile
durable deletion decisions before data admission; restoring an older registry
alone is not a supported restore procedure.
