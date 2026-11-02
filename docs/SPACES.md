<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces management contract

The deployed RFC 0028 implementation uses state protocol v20, Spaces envelope
v5 and registry framing OTSP9, including staged portable replacement. Earlier
v19/OTSP8 qualification remains historical evidence. See
[Transfers](TRANSFERS.md) for payload details and the
[combined qualification record](experiments/spaces-portable-transfers-2026-11-02.md)
for deployed migration/rollback and preservation results. Final Safari acceptance
is still pending.

The covered `otrios_core::spaces` library implements registry transitions and
admission. The authority additionally supports an explicit managed mode via
`--spaces-registry /ABS/PRIVATE/spaces.flex`. The live Frame was migrated on
2026-11-01; see the [Frame rollout record](experiments/spaces-frame-2026-11-01.md).
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

State protocol `otrios-canvas-sync-v20` includes a version-2 presentation binding
in each managed mount: collection identity and effective edit/comment/moderation
actions. These describe authority decisions and are never credentials. A client
resumes pending RAM intents only when collection identity, effective actions and
the opaque actor context match its previous mount. The context distinguishes the
authenticated person, continuous membership interval and attachment epoch; it
does not confer permission. A different identity or removal/readmission cannot
inherit the former participant's pending input. A replacement collection starts at receipt
sequence zero; identical app and state references cannot restore old edits into
it. Read admission does not allocate durable writer receipts even for editable
schemas. The authority still checks every write independently.

Aggregate and managed chat presentations accompany the initial mount, so a candidate is
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

Management/discovery uses its v5 binary envelope on channel 5 of the same
negotiated session. Both hosts use the covered RAM-only observer; it sends Watch after each reconnect. The
browser/native state protocol is v20; v10 introduced discovery, v11 added the
managed activation acknowledgement, v12 scoped auxiliary payloads by mount,
v13 added a qualified first chat transcript window, and v14 added collection-specific
delete confirmations and Admin cleanup metadata. V15 adds owned attachments,
reviewed publication and message moderation. V16 adds request-correlated replies
and the version-2 actor context. V17 adds bounded Space previews and presentation
management commands. V18 adds archived Spaces, exact retained datasets and reviewed recovery/deletion commands. V19 adds scoped moves and durable outcome queries. V20 adds staged portable
replacement, cancellation, read-only replacement outcome queries and explicit
host-authorized code publication. Launch references avoid duplicating installed
code facts in each app row; numeric
management request version 1 remains unchanged.
Managed hosts send Leave(epoch) after discarding revoked replicas; the authority holds cleanup pins until then.
Release discovery remains on its unchanged control v1 envelope.

Watch selects metadata for one Space and includes the previous authority/revision
cursor where available. A nonzero observation ID advances on each Watch and is
echoed by its snapshots. Revocation forgets the previous observation; delayed
snapshots cannot satisfy the new refresh or repaint an old private roster. Fresh
metadata can retry an available destination with a new mount epoch, while an
unavailable destination returns an authenticated participant to Home.
Every Watch explicitly replaces the snapshot through
resync. Observation is registered before initial capture. The
authority publishes catalog and management revisions under its admission lock,
then emits complete, filtered snapshots. A changed incarnation or skipped revision
explicitly marks resync. Mailbox overflow requires resync; no unbounded queue or
second WebSocket is introduced. Snapshot delivery rechecks current identity and
revision under the management lock, preventing an older queued roster from being
delivered after revocation acknowledges.

Snapshots contain up to 64 visible Space summaries, each with up to 16 attached
app previews (128 total; bundle ID, Space-local title and code compatibility).
Attached previews contain no collection IDs or usage totals. Admins additionally
receive bounded retained-dataset previews with exact collection IDs; these are
never granted to Members or visitors. Attached and retained previews share the
128-row budget. The map draws at most 16 tiles per Space; larger retained sets
open the paged Data list. Larger attached Spaces open a paged app list.
Detailed metadata is scoped to the selected Space: at most 128 app rows,
128 Admin-visible member records and 16 verified
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

The shared native/browser entrance is the [RFC 0028](rfcs/0028-spaces-map-sharing-and-portable-data.md)
Spaces map. It has no Spaces heading or role subtitles. Sign-in/profile stays at
upper-right; an Admin's Edit/Done action stays at lower-right. Empty Spaces are
hidden outside Edit. Selecting a single-app Space directly opens that app;
multiple-app Spaces offer their app list, and map app tiles launch directly.
Unavailable local code keeps its entry and explains the problem on selection.

Edit exposes Add Space, each Space's Add App, title dragging and corner resizing.
Arrow keys on a title, app tile or resize corner perform equivalent ordering/size
changes. App minus controls are separate from the tile's drag target. App reordering is
within a Space; a cross-Space drop opens the move review. Space on an app or ghost
picks it up for a keyboard destination chooser; it uses the same review. Layout
uses logical app-cell widths/heights (1–6), grows to fit content and reflows locally
without changing shared preferred sizes. The map pans vertically with shared
sample-time momentum physics on Web and Frame, retaining the mount throughout a
gesture. Web also supports wheel/trackpad and Arrow/Page/Home/End keys. Partly
visible content and a subtle lower-edge fade indicate more below; the fade
vanishes at the bottom. Up to four Spaces per page bound the local scene.
Edit previews also retain the mount so native touch capture survives every
frame. While dragging a title or app, hold near the map's upper or lower edge
to scroll toward an offscreen destination; move into the middle to stop.
Cancel and drops back onto the same item leave metadata unchanged.
Managed apps return through `< OTriOS • App name`; the breadcrumb uses the host's
reserved return action and includes the top-left padding in its touch target.

The Space settings panel combines name and background (including None), then usage
and its compact access/app/dataset information rows. People & access and Delete Space
are its primary actions. No width/height fields or separate
appearance panel is added. In Edit, an app opens a name field over its immutable
bundle ID. Enter, blur or closing commits a changed label; there is no Rename,
Move or Delete App button in that panel. Share App, Export Data and Replace Data
use the shared reviewed transfer workflow and qualified host adapters. Unmeasured
active-user totals remain unavailable only where no admitted-mount inventory is
available. The authority supplies Admin-only counts from accepted, currently
admitted mounts, deduplicating authenticated people and counting anonymous
connections separately. Successful changes do not display a Saved
message. An unavailable codec or host capability remains explicit.

In Edit, the red minus opens Delete App with Remove Data unchecked. Cancel
changes nothing and resets that choice. Delete without Remove Data detaches the
app while preserving its preferred collection. Checked deletion tombstones that
exact collection and removes the attachment. Detached datasets and sealed backups
appear after loaded apps as ghosted `<name> (data)` tiles; outside Edit they are
hidden. A ghost's info and minus select its exact collection, never an arbitrary
backup. Destructive and replacement reviews pin the registry generation; changes
or loss of Admin access require a new review.

Ordinary restoration reattaches the preferred collection with a fresh attachment
epoch. Restoring a sealed backup keeps that source sealed, retains the outgoing
current dataset as another sealed backup, copies all managed checkpoint/journal/
asset files to a fresh collection, then publishes its active binding. The
OTSR2 scoped plan uses the existing durable `.restore` admission barrier; verified
files become durable before the registry decision, and startup completes that
same decision before any app admission. Interrupted pre-plan staging is removed.
No second active instance of a bundle is created. Registry limits bound sealed
backups to 512 per authority and 128 total datasets per Space; reaching the limit
rejects restoration without discarding any dataset. Usage includes preferred and
sealed collections once each. Unavailable storage or schema/code compatibility
keeps Restore unavailable. Export/replacement also require the installed compatible
portable codec; see [Transfers](TRANSFERS.md).

Cross-Space moves require current Admin authority in both Spaces. A review pins
both exact collection bindings, the registry generation and discovery cursor;
metadata/catalog changes require picking up the item again. Attached apps offer
App and data, App only and Data only. Ghosts offer Data only. The destination
name and audience are visible; data-bearing public moves use an explicit
**Move and publish data** confirmation. App-only moves keep destination content
unchanged. A data-bearing occupied destination retains its outgoing dataset as
a sealed BeforeReplacement backup. A destination without an attachment receives
a ghost for Data only. Source attachment/data retirement and fresh destination
binding publish as one decision; delayed old producers cannot recreate either
collection. Closed-only/schema-incompatible data, unhealthy storage and service
or collector-backed apps without a separately authorized rebinding plan reject
without changing either Space. Service credentials never move.

The existing `.restore` admission barrier journals a scoped OTSR3 move plan.
Complete confirmed files and owned assets are staged and synced first, then
promoted under fresh collection identities, followed by the current registry and a
durable outcome receipt. Startup finishes this exact decision before admission.
The source's tombstone/pins retain existing cleanup protections; copied assets
have independent ownership rather than mutable links to source storage.
The deployed writer uses OTSP9, retaining OTSP8 custom RGB backgrounds and OTSR3
move recovery. Older writers that cannot preserve current decisions reject this
framing; the historical retained-data baseline also cannot recover OTSR3 plans.
Use a current compatible reader/writer for code rollback; preserve the paired
pre-deployment backup for an explicitly authorized state reset. Never remove the plan or restore older safety metadata.

Receipts live privately in `REGISTRY.moves/OPERATION.flex`: OTSM1, SHA-256 and a
bounded FlexBuffers record bound to the authority root, actor, both Spaces,
request digest and committed generation. They grant no access. The request hash
uses domain `OTriOS\0space-move-request\0v1\0`, length-prefixed UTF-8
actor/operation/source/destination/app/collection, then request version,
expected generation, optional destination collection and mode, in the canonical
little-endian encoding in `spaces/move_receipts.rs`. At most 1024 receipts of
2048 payload bytes are retained; capacity rejects new moves without evicting old
outcomes. Operational rollback kits include these receipts and pending plans;
app payload backups exclude authority safety metadata.

After a lost reply, the same client reconnects and issues read-only MoveStatus
by operation ID. Current Admin access to both scopes is checked again. A known
identical request can return its prior decision without executing again; changed
content under that ID conflicts. A new explicit review generates a new ID.
Identity changes discard the RAM watch. Disconnected outcome uncertainty cannot
block release discovery; connected outcome checks still precede handoff.

Space, app, dataset, access and recovery controls share one rounded modal
surface over the dimmed Spaces map. The underlying map has no input responders or
semantic controls while a panel is open. The panel owns its own bounded local
pan/wheel scroll; it preserves the mount and distinguishes a tap from a pan that
starts on a field or button. Web traps keyboard focus and Escape within the
panel and restores the initiating semantic control on close. Short viewports
scroll the panel rather than scaling it into an unreadable whole-screen form.
Primary confirmations use the shared green action treatment; destructive
confirmations stay red and routine successes are silent.

Profile editing saves a self-only display name; it never renames the authenticated
principal, changes membership or transfers writer context. A profile has its own
revision CAS and a private authority-root-bound OTSU1 record under
`REGISTRY.profiles/ACTOR_SHA256.flex`, limited to 256 profiles and 1024 payload
bytes per record. These files are separate from app payloads and included in
operational rollback kits. Discovery returns only the signed-in actor's profile.
An absent profile falls back to the authenticated identity; corruption cannot be
accepted as an empty/default profile and overwritten silently. Profile updates
leave registry generations and data bindings unchanged.

Space name edits commit implicitly on Enter, blur or closing, as app labels do.
Palette selections commit the currently edited name and chosen background as
one generation-checked ConfigureSpace. Swatches wrap to retain touch target size
on narrow screens. Dataset headings edit exact preferred/sealed dataset labels
through LabelDataset; labels never change bundle/collection identity. Archived
Spaces are recovered through Profile → Deleted Spaces, with current Admin checks.

People & access holds an unsent local draft of audience and membership changes.
Save sends one single-Space Access command with the desired audience and at most
16 explicit identity/role edits. Final membership must retain an Admin, with a
128-member bound. Existing members keep their continuous membership interval;
new members receive the committed generation. Audience and membership changes
share one admission-epoch publication. Public audiences still adopt/check the
current verified app policies; service principals cannot acquire human roles.
An access change opens its explicit review before this command is sent. Cancel
leaves all authority metadata unchanged. This bounded Space ACL publication is
not a general transaction API.

Discovery includes the installed catalog identity independently of its delivery
revision. Access/move reviews pin catalog identity, authority incarnation and
registry generation. Presence updates can advance delivery revisions without
invalidating an unchanged review or rebuilding a Home map during a gesture.
Admin detail includes accepted participant totals; staged mounts and retired
cleanup pins are excluded. Verified InstalledApp metadata also supplies logical
bundle bytes and revision. Unsigned development fixtures leave these code facts
unknown rather than inventing a version or byte total.

Deleting a Space without Remove Data archives it with its data and ACL intact.
It immediately denies app and service bindings, disappears for Members/visitors,
and remains recoverable for current Admins through Deleted Spaces. Restore Space preserves its
collections while issuing fresh admission/attachment epochs. Checked deletion
tombstones every current and sealed dataset before archiving the empty Space;
restoring it cannot restore those deleted bytes. Cleanup retains RFC 0027's
producer pins and managed-backup retention reporting.

Name, background, size and order requests use the reviewed registry generation
and require current Admin authority on the changed Space. Reordering cannot use
an inaccessible Space as an anchor. Metadata changes preserve collection IDs,
schemas, admission and attachment epochs; they do not revoke a valid app writer.
A stale form cannot overwrite a newer edit. Unknown replies require refresh and
review rather than automatic management replay. While connected, pending management
also prevents release handoff.

Switching Spaces retains the browser/native client and intended destination, but
replaces the one scoped transport using `/session?space=NEW_ID`. The new v1 hello
carries the current release hash; old-epoch messages cannot cross that handoff.
A URL/history update alone must never leave mounts on the old Space's socket.

Local forms use the shared keyboard; their drafts never enter app
state. These are trusted host controls with a private action table: a publisher
cannot invoke them by reusing labels or event numbers. A changed admission snapshot
redacts held management frames before renderer credit returns.

Returning to OTriOS waits for connected edits to be confirmed and gestures to finish, then leaves
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
Physical Frame qualification remains separate from these software checks.

## Device-local uninstall

The native host optionally takes `--local-apps /ABS/PRIVATE/local-apps.json`
or `OTRIOS_LOCAL_APPS` with that path. The device operator initializes and
changes this private installation metadata as the same OS user that runs the
host, using a directory that other users cannot write:

```sh
python3 scripts/device-apps.py --file /ABS/PRIVATE/local-apps.json init
python3 scripts/device-apps.py --file /ABS/PRIVATE/local-apps.json remove brass-relay
python3 scripts/device-apps.py --file /ABS/PRIVATE/local-apps.json restore brass-relay
```

`status` prints the namespace generation and removed roots. A configured host
binds an exclusively owned private Unix notification socket before capturing
the initial snapshot. Mutations serialize through a separate writer lock, replace
the mode-0600 file atomically, fsync it and its directory, then request resync.
An offline mutation is loaded on next startup; coalesced notifications always
read the latest snapshot. Missing, invalid or rolled-back input closes local app
launches. Equal-generation snapshots must have identical contents. The live
worker retains its last accepted generation even while input is invalid.

The device's installed namespace is the verified release catalog minus removed
roots and their child destinations. Removed code cannot qualify a mount, run
from cached code, or reappear through a fresh authority discovery snapshot.
Signed catalog bytes and release hashes stay intact; immutable distribution and
rollback code replicas may remain cached. `restore` reinstalls the root only if
compatible verified code is available. It does not fetch unsigned code.

Uninstall cancels staged launches, blocks new interactions, waits for outstanding
edits and captured gestures, then disposes local execution and invalidates held
presentation frames independently of renderer credit. Managed clients return to
Home, where attachments show unavailable locally. Uninstall sends no Space
management operation: attachments, collection identities, retained histories and
other devices are unchanged. Reinstall can reopen the same collection. This
PoC discards unconfirmed RAM edits if that participant loses its transport during
uninstall. A reconnect cannot qualify the removed root or resume those discarded
intents. Confirmed authority data is preserved; uninstalling a root does not
introduce durable client storage. Device installation
administration is an OS-operator action, independent of Space Admin membership.
When `OTRIOS_LOCAL_APPS` is configured, the Frame supervisor also passes the
required `--local-apps` option to the verified executable. Older binaries reject
that option; a rollback must use an executable that enforces the namespace.

Managed mode admits Create, Membership, Attach, Remove, confirmed whole-collection
deletion and Admin audience changes. Global chat/collection fallbacks and legacy
presentation reads are disabled; they cannot be used
as an unscoped fallback. Monitoring presentation uploads require an explicit
collector identity and collection grant.

Managed ink uses channel 2 on the shared session and a little-endian nonzero
32-bit accepted-mount epoch before each auxiliary payload. The socket binds it to
the accepted app's own document within its collection; staged candidates cannot
subscribe or publish. A typed internal lease notification releases old resources
on Quit, activation changes or revocation, independently of state output credits.
Both reads (including welcome/history) and incoming points recheck current
admission under the management lock through delivery/mutation. Stale epochs are
ignored before decoding their payloads, including after the host returns Home.
Read-only clients do not publish points. The unchanged v1 Ink control is a local
subscription preference and cannot choose a Space, app or collection.

Trails remain ephemeral: each room retains at most 1024 recent points for five
seconds, and at most 128 rooms are cached. Idle rooms may be evicted; active
subscriptions pin their rooms. Removing an attachment discards its relay caches;
reattaching retains durable app data and starts fresh trails.

Managed chat and its collection windows use channels 3 and 4 with the same
accepted-mount epoch framing. Each owning document has its own paired room and
collection channel; the fixed transcript collection number cannot select a
sibling app, document or Space. Eight room pairs may be cached; staged and active
leases and workers pin their pairs. Only healthy idle pairs may be evicted.
An uncertain journal remains poisoned until recovery at restart.

The first transcript is one immutable, indexed window of at most 64 messages and
256 KiB. Capturing it creates no profile or presence and writes no journal. A
missing, malformed or unreadable first view rejects launch while preserving the
previous activation. The owner builds and lays out the first transcript in bounded
ticks while the old view stays visible, and sends Accept only when its visible
document frame is ready. Candidate layout failure cancels that candidate;
renderer credit does not block this preparation. After Activated, the normal collection subscription selects
the latest coherent window, covering changes made during launch qualification.

The covered room authority binds an app-local profile to an admitted human
identity. This is a trusted host API requiring collaborative edit admission;
client commands cannot choose the authority identity. It preserves the profile
through restart without issuing a guest resume credential or exposing the
authority identity in room events. Legacy guest credentials remain separate and
retain their existing expiry. Revocation has a journal-free presence cleanup
operation, so disposing execution cannot recreate a deleted journal path. Browser
managed sessions do not provision a guest cookie; native guest credentials are
ignored. Room moderation is an ordinary Member action and the UI labels it
Moderator; it does not grant Space administration. Authentication changes use
the host controls, rather than the legacy room's Monitoring sign-in form.

All room operations and collection output recheck the active binding under the
management lock. Durable room writes finish within that lock's lifetime;
cancelling a socket task cannot leave a detached write behind it. Quit and
revocation release presence independently of renderer credit, and output rechecks
admission through delivery. Removing and reattaching an app preserves its chat
journal and profiles. Pre-Space chat history requires the explicit offline import
below; attaching the app alone does not infer a legacy source.

Separate child clients exercise authority reads/writes, collection isolation,
explicit Frame access, active revocation/reconnect, Member management denial and
concurrent last-Admin protection. These are authority adapter checks, not Frame or
browser qualification. `scripts/check.sh` supplies its freshly built authority to
`otrios-core/tests/spaces_process.rs`. Its test-only `tungstenite` dependency reuses
the pinned workspace WebSocket client; it adds no runtime dependency.

For real-browser qualification, build the authority, `bundle_counter`,
`space_ink_probe`, `space_chat_probe` examples and Wasm package from the tested
revision. Use `python3 scripts/qualify-spaces-browser.py --wasm-pkg /ABS/BUILT/pkg
--serve --output REPORT_DIRECTORY` to serve a disposable fixture in Safari.
The fixture uses synthetic data, test identities and a temporary signing key;
`REPORT_DIRECTORY/fixture.json` records its URL and runtime hashes. Stop the
fixture process when acceptance is complete. The focused map driver remains an
optional automated Chromium probe. The superseded management-page browser
script has been removed; real authority/process tests retain data isolation,
revocation, concurrent-write and ownership coverage.

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

Chat import uses provisioning manifest version 2, so older v1 provisioners reject
the plan instead of ignoring its import declaration. Existing v1 plans without a
chat import remain supported. Each app entry can declare one `legacy_chat_root`
that names an installed Chat presentation owned by that app:

```json
{
  "version": 2,
  "spaces": [{
    "name": "Household",
    "admins": ["alice"],
    "members": ["frame"],
    "apps": [{
      "id": "brass-relay",
      "legacy_chat_root": "app:brass-relay/state"
    }]
  }]
}
```

Stop the old authority before provisioning. The importer takes the legacy
`state.apps/brass-relay.journal` ownership lock, validates every complete record,
recovers only an incomplete final write, and copies the complete retained journal
into the selected owning document. It does not copy just the initial 64-message
window, choose a destination automatically, or import arbitrary filesystem paths.
The source must be a private regular file, at most 64 MiB. A missing/corrupt source,
live owner, or different confirmed destination rejects the import and leaves
provisioning unavailable rather than starting with empty data.

The durable private `legacy-chat-import.json` receipt records collection, schema,
owning root, source digest and byte count. A completed import validates its current
destination on later provisioning; it never replays the old journal over newer
confirmed messages and can resume without the old source file. A missing or corrupt
destination cannot be recreated through that receipt. The original legacy journal
is retained after the first import; later deletion/backup policy must account for
that copy. Historical sender snapshots survive; admitted identities receive their
own app-local profiles and cannot claim a legacy guest profile using its old secret.

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

## Public audiences and comments

An Admin chooses Closed, Public view, or Public view & comment in Apps & Data /
Sharing. Public choices require an explicit confirmation naming the Space and
explaining that every attached app's content is included. Current closed-only
code (including Monitoring) blocks public settings even if retained policy metadata
predates that code. An explicit audience change adopts compatible installed app
capabilities while preserving collection identity and data. Schema changes never
silently replace a collection.

The fixed Chat presentation declares separate comment operations. Read-only viewers
receive the first transcript and bounded live windows without creating a profile,
presence or writable document session. Signed-in visitors may join/post and manage
their own app-local nickname/avatar/draft when public comments are enabled. These
commands cannot edit the app document or grant moderation. Members moderate where
supported; guest credentials and legacy app authentication cannot elevate managed
participants. Configured service tokens cannot act as human commenters.

Managed activations also bind the authenticated principal and effective action
mask. Changes revoke active/staged views even when public read remains available,
such as signing out of a public-comment Space. Queued state and auxiliary payloads
recheck the same grant through send; the old writable document session and chat
identity cannot survive an anonymous read grant. Role/audience restrictions apply
to subsequent operations and reconnects.

## Selected publication and owned attachments

The fixed Chat adapter copies one reviewed confirmed dispatch into the same app in
another Space. The authority requires current source Read, destination Edit and
the reviewed management generation under one lock. Public commenting alone cannot
publish. The source row revision prevents copying a removed or changed selection.
Other presentations have no publication adapter; this is not a general cross-app
transaction API.

In Chat, open Commands / Dispatches, choose a dispatch, then Publish to
another Space. The trusted chooser lists other Spaces where the current person
is a Member or Admin and requires an explicit destination confirmation. The same
app must be attached and compatible there. The source mount remains active during
review; role loss, deletion or removal clears the private preview before its
disposal acknowledgement. Changed destination metadata cancels the confirmation.
The fixed host controls cannot be remapped by publisher descriptions; compatible
older Chat descriptions can use newer host actions without being rewritten.

Browser Commands / Send file opens a bounded file picker and sends the selected
file with the current composer text. Both hosts list attachment names and can
publish their owned bytes; browser attachment controls download an explicit
separate copy. Native Frame file picking/downloading is not offered. File callbacks
carry the originating mount epoch and cannot target a replacement collection.
Unsolicited, stale or mismatched download replies cannot initiate a download.
Connected navigation and upgrades wait for send, publication and moderation
receipts. Unconfirmed file/publication/moderation operations and previews are
discarded on disconnect; confirmed authority data remains authoritative.

The destination gets its own sequence, timestamp, nonce and app-local sender ID.
Only the selected display name, icon, message content and owned attachment bytes
are copied. No source binding, original profile ID, authority identity, guest secret,
live draft or private source URL appears in destination messages. Required assets
are copied and synced into the destination collection before its one journal
record is committed. Later private changes do not update the copy. A subsequent
publication is another explicit copy; a durable receipt deduplicates retries of
the same nonce and selection. Source and destination deletion are independent.
Destination comments remain in the destination's transcript.

Each attachment is at most 256 KiB. A message has at most four attachment
references; the first upload command supplies one file. A collection owns at most
32 immutable blobs (8 MiB), stored as flat `asset-SHA256.blob` files in its owned
`.data` namespace. The digest identifies bytes and is never a capability.
Scoped channel-3 ReadAsset requires a current admitted message and exact row
revision. Anonymous readers can download public attachments; known hashes and old
row references cannot read another Space's data or unreferenced bytes. Large
binary replies use a bounded direct peer queue, not broadcast history. Input keeps
eight control-message slots and holds byte credits through processing, bounded
by 384 KiB plus eight collection-command allowances per peer. Identity,
policy and mount epochs are rechecked through shared-session delivery.

Files are private, integrity checked and synced before a confirmed message can
reference them. Interrupted `.asset-RANDOM.tmp` stages are unreachable and removed
on the next new-blob write or collection deletion. Completed unreferenced blobs
remain within the quota until collection deletion; no early cleanup success is
claimed. Remove, Quit and uninstall retain these bytes. Paired snapshots include
them; permanent deletion reclaims only their owning collection. An admitted
in-flight download or user export is outside a universal erasure claim.

Members/Admins with app moderation can remove a reviewed dispatch; visitors cannot.
Removal publishes a new coherent collection root after its journal record is
synced, preserving committed sequence identities and send/publication receipts.
Old pins remain bounded until replaced/released; retries do not re-add removed
rows. Ordinary message moderation can still have retained journal/backups. The
non-expiring deletion barrier and retention/cleanup reporting apply to permanent
collection deletion, not an unqualified per-message wipe claim.

The message collection schema is v2. Empty attachment lists retain the legacy
serialized message shape; old confirmed records are read without replacing app
collections or their schema fingerprints. New chat journals use `OTCHAT02`.
An `OTCHAT01` journal is recovered completely, then atomically compacted into
`OTCHAT02` before its next durable write. Older writers reject the new framing
without truncating it. Legacy journal-only manifests reject owned attachment
references rather than silently losing their namespace; use paired collection
backup/restore for owned data. Registry framing is `OTSP9`; release-control v1 remains
independent. State protocol v20 retains these commands and correlates moderation/download replies
with request nonces.


## Persistence

The native Store writes `OTSP9`, a 32-byte SHA-256 payload digest, and a bounded
FlexBuffers record containing the registry, a stable authority-root identity and
its paired app-storage path and migration-original retention decisions. The
registry remains bounded at 2 MiB; attribution metadata adds at most 2 MiB and
the private record has an additional 8 KiB framing/path budget. Collections are
bounded vectors, not unbounded map keys.
Recovery checks byte bounds, digest, structural nesting/visit bounds, version,
unique IDs, last-Admin invariants and epochs before admitting any data. The digest
detects corruption; it does not authenticate an operator or grant access.

Recovery upgrades `OTSP1`, `OTSP2`, `OTSP3`, `OTSP4`, `OTSP5`, `OTSP6`, `OTSP7` and `OTSP8` registries while preserving their decisions;
the initial prototype defaults older members' continuous-membership generation
to zero. The upgrade writes the latest decision ledger before replacing the
primary. Older writers reject `OTSP9`, including writers that cannot honor
an incomplete payload restore or migration-original retention protections. Rollback must use a reader/writer that
supports this framing and the current admission rules. OTSP9 adds imported inactive dataset provenance and fences older portable-replacement writers. OTSP8 added custom RGB background colors; OTSP7 added archived Space and sealed-dataset decisions; OTSP6 added Space layout and
Space-local app labels; omitted legacy fields default without rebinding collections.
Legacy Spaces start at one to three cells wide according to attached-app count;
existing OTSP6 layout edits are preserved on every later recovery.
Never downgrade current safety metadata to make an older writer accept it.
Management request version 1 and release-control v1 remain stable. Current state protocol v20 retains the
collection-specific deletion confirmation and Admin-only cleanup metadata and adds
reviewed publication, attachment blobs and message moderation.

A stable private `REGISTRY.owner.lock` inode carries an exclusive OS lock and
the root identity. The registry's containing directory must not be writable by
other users. Do not remove or replace this lock file. A second process cannot
acquire the registry, including across atomic state-file replacements.

Each commit writes the complete latest decision record to `REGISTRY.latest`,
then the primary registry. Each publication uses a mode-0600 temporary file,
file sync, atomic rename and parent-directory sync. Only then does the in-memory
registry advance or the management request acknowledge. Any uncertain result
closes admission until restart. Recovery applies the ledger before serving an
older primary, including an older primary restored from backup; a missing primary
can be rebuilt from the ledger. Missing, corrupt, foreign or inconsistent safety
metadata fails closed instead of falling back to an older registry. A torn initial
owner identity can finish only against the matching durable ledger.

The authority pairs app storage once using its private `spaces.root` marker and
records the canonical path in the decision record. Removing that initialized
storage or substituting a different root leaves state unavailable, even if the
host creates an empty directory during startup. Restoring the original paired
storage recovers its confirmed data. Release-control hello remains available
while state recovery is unavailable.

The primary, `.latest` and `.owner.lock` files are control metadata outside
rollbackable app payloads. A supported payload restore must retain the latest
ledger and root identity; copying an old registry alone cannot regrant revoked
membership or remove deletion decisions. Rolling all control metadata back to an
old snapshot is unsupported and cannot recover later decisions. The managed
payload backup/retention and reclamation workflows below preserve these decisions.

Admin-only DeleteData names the exact reviewed collection as well as Space/app
and the expected registry generation. A stale confirmation cannot delete a newer
replacement. The trusted host identifies Space and app, explains retained copies,
and requires a separate confirmation. Delete and Remove & delete are distinct.
One durable publication tombstones the old collection and creates its fresh ID
before success is acknowledged. All writes serialize against that decision.

Current policy fences old documents, chat workers, windows, ephemeral ink and
staged mounts before further delivery or mutation. Hosts discard revoked replicas
and then acknowledge disposal using Leave(epoch). The authority retains minimal
lease pins until that acknowledgement or disconnection; sync snapshots are dropped
immediately. At most two outstanding revoked mounts are accepted per session,
with connection closure on further undischarged revocations. Old epoch packets
cannot decode against or mutate the replacement. Returning clients only resume
RAM edits when collection identity and effective actions still match.

The covered reclamation backend requires a durable tombstone and trusted
producer-quiescence facts from the authority under its admission lock. Until
producers stop it reports pending without changing payloads. Reclamation covers
the exact collection's checkpoint, journal, presentation, child documents, chat
history, migration receipts, derived files and interrupted-write temporary files.
It never selects data by content hash alone. Unlinking a shared hard link preserves
another collection's reference, and symlinks cannot redirect directory cleanup.
Reclamation syncs directory removals, resumes idempotently after interruption,
and processes at most 128 extra root files per turn. Tree depth/entry limits or
filesystem failures keep cleanup pending. A reclaimed authority namespace says
nothing about backup expiry, disconnected replicas or unmanaged exports.
Startup and the authority maintenance timer reclaim tombstoned namespaces only
after all local producers and connected disposal pins are gone. At most eight
collections receive filesystem reclamation per turn; blocked/failed collections
rotate so unrelated work continues. Pending work retries once per second.

Apps & Data exposes an Admin-only Cleanup view. Its summaries distinguish pending
collection/session cleanup, retained backup deadlines, migration originals awaiting
qualification, original-retention deadlines and unknown storage inventory. Missing
or corrupt retention metadata cannot produce a completed-inventory claim. Cleanup
changes advance discovery without changing the durable registry generation.
Disconnected replicas and user exports remain explicitly unverified; collection
file reclamation is not a promise of erasing every copy.

## Managed snapshots and offline restore

Run storage maintenance as the authority's OS user after stopping the authority.
Its exclusive registry lock rejects concurrent offline maintenance. These commands
require an existing paired safety root and never initialize missing storage:

```sh
dashboard-server --spaces-registry /ABS/PRIVATE/spaces.flex --app-state /ABS/PRIVATE/state.apps --spaces-storage backup
dashboard-server --spaces-registry /ABS/PRIVATE/spaces.flex --app-state /ABS/PRIVATE/state.apps --spaces-storage list
dashboard-server --spaces-registry /ABS/PRIVATE/spaces.flex --app-state /ABS/PRIVATE/state.apps --spaces-storage restore:BACKUP_ID
dashboard-server --spaces-registry /ABS/PRIVATE/spaces.flex --app-state /ABS/PRIVATE/state.apps --spaces-storage prune
dashboard-server --spaces-registry /ABS/PRIVATE/spaces.flex --app-state /ABS/PRIVATE/state.apps --spaces-storage finalize-legacy
```

These actions finish without opening listeners or loading app execution. Pending
provisioning must be resumed first. Maintenance is separate from online Space
membership; restoring payloads does not change who may access them.

Snapshots live under `REGISTRY.backups`, paired by the private
`REGISTRY.backups.root` marker. Each immutable snapshot ID contains its creation
time and a random nonce. A checksummed manifest records collection identities,
schemas, file lengths and SHA-256 digests. Only current attached/retained collection
payloads are copied, including histories and assets; tombstoned collections,
authority membership/auth configuration, safety decisions and owner locks are
excluded. Files are private and streamed with bounded memory. Atomic directory
publication follows file and directory sync, so incomplete snapshots stay
unavailable and can be reclaimed after a crash.

The PoC keeps up to 16 snapshots with a 2 GiB payload budget. One snapshot is at
most 512 MiB, 4096 files and 64 MiB per file, with a manifest at most 2 MiB. Capacity
failure leaves the source unchanged. Snapshots expire for restore access seven
days after creation. Pruning removes expired bytes and incomplete snapshot copies;
corrupt manifests cannot extend retention because expiry comes from the owned
snapshot name. Pruning runs before a snapshot, through explicit maintenance,
on authority startup and at most once per minute while serving. Expiry does not
promise removal at the exact millisecond: pending/failed physical cleanup remains
visible until verified.

A restore verifies all selected payloads into private staging and first creates
a safety snapshot of current data. It replaces only snapshot collections whose
identities and schemas still match the latest registry. Collections created
after the snapshot remain intact; files added later inside a restored collection
are removed to reproduce that collection's snapshot. Latest membership, audience,
attachments and deletion decisions are retained. A deleted collection is skipped
even if its old backup files still exist or have already been reclaimed.

Before replacing payloads, the authority durably publishes `REGISTRY.restore`.
Registry admission then stays unavailable until every replacement/removal and
directory sync completes. Startup validates the paired storage, resumes that
plan and verifies staged or already-promoted bytes before admitting apps. A
failed or corrupt restore stays closed. Abandoned staging before publication is
reclaimed; an interrupted published plan resumes idempotently. `--spaces-storage
resume` also completes a pending plan. Code rollback keeps payloads and current
control metadata; payload restore is a separate explicit maintenance action.

Imported migration originals are attributed inside the private latest decision ledger; completed import receipts can repair this attribution
without replaying data or granting admission. Originals stay protected until the
operator records migration acceptance and runs `finalize-legacy` offline. That
sets a fixed seven-day retention deadline and closes further legacy imports.
Repeating finalization cannot extend it. `prune` removes attributed originals
after expiry only after checking every retained original against its recorded
length/hash and acquiring the legacy chat owner's lock. A changed source leaves
all remaining originals intact. Collection payloads and stable owner locks are
preserved. Missing originals are treated idempotently during pruning. A source
associated with a tombstoned collection cannot be imported into a fresh collection;
unregistered originals are also refused after any deletion decision.

Inventory independent legacy roots before finalizing migration; unattributed
files are not silently classified or erased. Known interrupted checkpoint and chat-compaction temporaries expire with their
originals in bounded cleanup turns. The authority timer also prunes expired
originals and reports remaining retained copies. Browser lifecycle and publication
acceptance are recorded in the qualification report. Physical Frame acceptance
remains required before claiming the rollout complete.

## Rollout requirements

Provision named human identities and the bootstrap Admin explicitly. A legacy
shared login, device administration or trusted listener cannot grant membership.
Provision the Frame as a separate authenticated identity with explicit Space
memberships. Bind Monitoring's collector to its intended Operations collection
as a service identity; human Monitoring views remain read-only.

The installed authority unit must explicitly pass
`--spaces-registry /var/lib/otrios-authority/spaces.flex`, matching the checked-in
unit, and `--collector-binding /var/lib/otrios-authority/collector-binding.json`.
Provision that registry and migrate the declared legacy roots while the
authority is stopped, before activating the unit. The explicit argument also
prevents pre-Spaces binaries from silently reopening legacy admission during a
rollback; use only a rollback binary qualified for the current registry and chat
journal formats. Never replace the registry with an empty bootstrap.

Before live cutover, record each legacy root's destination Space and collection,
preserve its checkpoint/journal and confirmed receipts, and validate recovery in a
staging copy. Advance the state protocol when the wire format changes while
preserving release-control v1. Rollback must retain registry/deletion decisions
and may use only code that enforces them. Backup restoration must reconcile
durable deletion decisions before data admission; restoring an older registry
alone is not a supported restore procedure.

## Home input deployment checks

For a validated input/presentation update, use the existing signed staging and
activation workflow. `scripts/backup-frame-release.py --config PRIVATE_CONFIG
--tag YYYY-MM-DD-description` preserves a quiescent rollback kit on the Frame,
restores the active services, and verifies a private copy under the Mac runtime.
Inventories and payloads stay outside the repository. Code rollback must continue
using the current safety roots; the backup is not permission to restore them.

After activation, `scripts/check-frame-release.py --config PRIVATE_CONFIG --output
REPORT` checks the signed pointer, native readiness and services in one SSH call.
`scripts/test-frame-home.py --config PRIVATE_CONFIG --output REPORT` exercises
three scroll round trips, immediate reversal at each bound, stable mount epochs,
and a public app's return target at logical `(5,5)` through the calibrated evdev
reader. Its explicit `--app-point` defaults to the existing Frame's public Brass
Relay Space at `(120,132)`; adjust that point when the layout changes. It writes
no app data and exports neither pixels nor private state. Synthetic device-input
acceptance does not establish human touch quality or a measured frame rate.
The optional `--reorder-spaces` probe additionally drags the existing Brass Relay
before Household, verifies the authoritative order and retains that arrangement.
It changes presentation metadata only and requires their titles to be visible.
Edit probes require `--runtime-probe` naming a private on-Frame build of the
`space_map_probe` example. That read-only wrapper derives title coordinates through
the same runtime from local metadata; it does not export them. `--edit-capture
--skip-navigation` checks a same-title drop without a management write. Run that
mode from the ordinary map. Reordering requires Admin in both named Spaces.
The explicitly invoked `grant-frame-admin.py` operator tool uses the existing
offline provisioner for authorized membership grants; it is not a routine
deployment step. Its private plan and pre-change metadata remain on Frame.

The populated browser probe uses four synthetic Spaces and 14 app previews:
`python3 scripts/qualify-spaces-browser.py --wasm-pkg BUILT_PKG --populated
--driver scripts/test-spaces-map-browser.mjs --output REPORT_DIRECTORY`.
It signs in once from the public common entry, resizes the viewport while the
form is open, scrolls repeatedly, and checks fixed header/footer bounds and
actual fade/footer pixels. It never loads production state. Scrolling binds the
explicit map node from the host renderer; the outer page is also a clipped
canvas and must never be selected as the scroll viewport. Sign-in intent survives
presentation/metadata rebuilds until authenticated or cancelled; background
session refreshes defer to a pending cookie mutation.

The shared settings panel composes a separate, inert map scene beneath its
scrim. This keeps a populated map and the native software keyboard inside their
individual 128 KiB layout budgets. Pages also bound visible app previews rather
than counting only Spaces. Custom background colors use an explicit six-digit
hex field on both hosts; dark colors receive light labels. No native file or OS
color-picker capability is implied.
