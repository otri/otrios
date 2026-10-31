<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0027: Spaces, membership and app data

Date: 2026-10-31. Status: draft for review; no runtime implementation or
deployment in this change. Records the agreed product model and proposes bounded
implementation milestones. Builds on [standalone bundles](0025-standalone-app-bundles.md),
[durable app state](0006-durable-canvas-merkle-adapter.md) and the
[shared session](0009-unified-server-session.md).

## Goal and boundaries

Make apps shared experiences by default within a Space, with an access model
people can explain without document ACLs. A Space owns its data; an app supplies
the schema, views and operations used to work with it. The authority stores and
synchronizes that data on the Space's behalf. A publisher supplies code, not
ownership of the data or permission to retrieve it.

Use two membership roles: Member and Admin. Support multiple Admins. Visitors
may view or comment when the Space explicitly allows it. Different audiences
require different Spaces or an explicit publication into another Space.

This draft replaces RFC 0025's proposed `/PrivateApplications` convention.
`/Applications` is the proposed installed-bundle namespace. Space membership and
audience policy govern access to app experiences and data, independently of
bundle placement. These are OTriOS concepts, not Linux user/group permissions.

## Membership and audience

| Participant | Allowed actions |
| --- | --- |
| Member | Use the app's collaborative actions throughout the Space, including ordinary content editing/deletion and comment moderation where the app supports them |
| Admin | All Member actions, plus membership, audience settings, app attachment/removal and permanent deletion of an app's data collection |
| Visitor | Only the viewing/commenting actions explicitly enabled for the Space |

Admin includes Member capabilities; there is no unique Owner role. Keep at least
one Admin while the Space exists. Reject concurrent changes that would remove
the last Admin. Membership and destructive management changes are serialized
by the authority, not merged as ordinary collaborative document edits.

Each Space has one audience setting:

- **Closed:** only Members and Admins have access.
- **Public view:** visitors may read the Space's app content.
- **Public view and comment:** visitors may also submit comments where supported.

New Spaces start Closed. Sharing by default means collaboration among their
Members, with public access enabled through an explicit Admin action.

Visitors are an audience, not another membership role. For the first delivery,
comment submission requires a signed-in identity; signing in does not grant
membership or editing. Members can moderate comments. Apps must declare separate
comment operations so enabling comments cannot admit document-edit operations.

Audience settings apply uniformly to the Space's app content. Apps may offer
fewer actions: Monitoring is read-only for human participants, whereas Notes
offers editing. An app requiring closed access, such as Monitoring, cannot be
attached to a public Space; making a Space public must reject incompatible apps.
Do not silently expose a closed app or invent a hidden subset inside a public
Space. Internal credentials and management records are never public app content.

There are no per-page permissions, private folders within a shared Space,
invitation-only Viewer memberships, or per-app audience exceptions. A device
administrator does not automatically become an Admin of hosted Spaces. Local
root remains a trusted infrastructure operator; this model does not promise
cryptographic isolation from the machine storing the data.

## Identities and data organization

Keep these concepts separate:

| Entity | Identity and lifetime |
| --- | --- |
| Installed app | Stable app ID and signed bundle revisions; reusable by multiple Spaces |
| Space | Stable Space ID, name, membership and audience policy |
| App attachment | Makes an installed app available in one Space and binds it to that Space's collection |
| App data collection | Unique collection ID, Space ID, app ID and schema identity; survives app removal |
| Local session | A running view of an attachment; quitting does not delete shared data |

Start with one retained data collection per `(Space ID, app ID)`. Multiple pages,
notebooks or drawings belong inside that collection. Multiple independently
configured instances of the same app in one Space are deferred.

```text
Household [Space]
  Notes [app attachment -> collection]
    Pages, attachments, comments
  Drawing [app attachment -> collection]
    Drawings, assets, comments

Operations [closed Space]
  Monitoring [app attachment -> collection]
    Configuration, collected observations
```

This is the Admin's logical view, not a required filesystem hierarchy. Store
mutable data outside immutable `.otapp` bundles and signed code releases. App
updates and renames preserve collection identity. Schema incompatibility must
report an unavailable collection until an explicit migration is supported;
it must not silently create an empty replacement or overwrite existing data.

The authority resolves an admitted Space/app/collection binding. A client-chosen
`state_ref`, app ID or content hash never grants access. Current bundle state
references cannot remain a global room key when the same app serves two Spaces.
Define that binding and its versioned encoding before implementing scoped mounts.

A collection is a management boundary, not necessarily one CRDT document. Reuse
the authority's durable app roots, incremental Merkle synchronization and bounded
collection windows. Scope subscriptions, writer receipts and mutations to the
admitted collection identity; do not download an entire Space to open one page.
Clients retain the current RAM-only working-state policy. This does not introduce
peer-to-peer replication, durable offline clients or another session connection.

Space membership authorizes the person; it does not let an app read every sibling
app's collection. Execution receives its own admitted data binding. Cross-app
work such as publication uses an explicit host-mediated operation.

App services such as Monitoring's collector operate through explicit authority
bindings to the intended Space/collection. Installing a bundle does not install
privileged service code or let the publisher inspect data. Service bindings are
implementation identities, not another human membership role.

## Admin experience and lifecycle

Provide an **Apps & Data** view with attached apps, retained collections from
removed apps, storage usage and compatibility/status information. Identify the
Space and app before any destructive action. Export is available where supported;
do not imply every app already has a portable export format.

| Action | Effect on app and data |
| --- | --- |
| Add app to Space | Attach the app; reuse its compatible retained collection, or create a fresh collection if none exists |
| Quit | Dispose this participant's local execution, subscriptions and presentation resources; retain shared data and other participants' sessions |
| Remove app from Space | Remove the launch entry and stop its Space-bound sessions/services; retain its collection under “Data from removed apps” |
| Delete app data permanently | Delete this app's collection in this Space; if attached, the app becomes empty through a new collection identity |
| Remove app and delete its data | Combine removal with permanent deletion in this Space |
| Uninstall app from device | Remove local installed code and stop affected local execution; retain all Space data and attachments, which show unavailable locally |

Other devices can continue using their installed code. Removing an attachment
is a shared Space management action; quitting and uninstalling local code are
not. Ordinary page/drawing deletion is an app action available to Members,
preferably with recoverable trash. Whole-collection deletion is Admin-only.

### Permanent deletion and synchronization

Permanent deletion covers the selected collection's documents, attachments,
comments, histories and derived indexes/caches. It does not delete another
Space's collection or independently published copies.

The authority must durably record deletion and close admission for the old
collection before acknowledging success. Stop its producers and revoke active
mounts/subscriptions. Order deletion against concurrent writes: a write either
commits before deletion or is rejected; a later write cannot recreate the
collection. Restart must recover the deletion decision before accepting clients.

Connected clients receive invalidation and discard their replicas. A returning
offline client must revalidate access and collection identity before syncing;
reject old edits and discard the old replica. CRDT merge cannot resurrect a
deleted collection. Keep a minimal durable deletion record for stale-reference
rejection, without retaining the deleted document payload. Specify deletion
record retention before allowing it to expire.

If the app remains attached, create an empty collection with a fresh identity.
Recovery may finish that replacement after a crash; it must never reactivate the
old collection. All clients must acquire the new binding before submitting edits.
These are scoped management transitions, not a general cross-app transaction API.

Physical reclamation can follow logical deletion. Remove references to immutable
blobs and reclaim them when no live collection references them. Shared storage
must not let one Space's deletion erase another's referenced bytes. Report cleanup
as pending until managed storage and reachable replicas have completed cleanup;
an offline device cannot promise an immediate wipe.

Backups need a declared retention period. Deletion immediately removes system
access, while backup bytes expire under that policy. A restore must apply deletion
records before serving data; restoring an older backup must not resurrect deleted
collections. Do not promise erasure of user exports or unmanaged copies. Export,
backup retention and physical cleanup mechanisms need implementation specifications
before shipping a “permanently deleted” success claim.

## Sharing and publishing scenarios

| Scenario | Space arrangement |
| --- | --- |
| Blog with private drafts | Closed editorial Space; explicit publication into a public blog Space |
| Collaborative team notebook | Closed Space; all Members can work on all pages |
| Open notebook or drawing gallery | Public Space; visitors view, optionally comment; Members edit |
| Publish selected private pages | Explicit copies into another Space with the desired audience |
| Family photo frame | Household Space; app presents the collection to authorized participants; public display is an explicit publication/exposure choice |
| Monitoring | Closed Operations Space; human participants read; a bound collector supplies observations |

Initially, publication creates an explicit copy of selected content in the
destination Space. It requires read access to the source and editing permission
in the destination. Copy required assets rather than leaving references that
grant visitors access to private source data. Later private edits do not appear
automatically; updating a publication is another deliberate action.

Deleting a private draft does not delete its published copy, and vice versa.
Comments belong to the destination publication. A link between Spaces never
confers membership or access. Automatic cross-Space synchronization, live mirrors
and permission inheritance are deferred.

## Discovery, launch and admission

Home observes the host's admitted `/Applications` view and the selected Space's
attachments. Browser Home receives the corresponding authority-provided view;
it does not require direct access to Frame's filesystem. Capture an initial
snapshot and register observation without a gap, with explicit resync on overflow.
Discovery reads launch metadata without starting the app.

Keep two readiness boundaries: the installer atomically publishes a complete,
verified bundle; a launch becomes ready only after compatible code, current
authorization, the collection binding and its first usable view are admitted.
Partial copies and failed candidates are never launchable. Removing an attachment
or deleting a collection during launch invalidates the candidate.

An installed app is not automatically attached to every Space. Home's lock
indicator describes current access to the selected experience; it is local
presentation, never an authorization grant. Sign-in may be necessary but is not
sufficient without membership for a closed Space. Discovery must not expose
closed-Space content or membership details to unauthorized visitors.

The authority checks access before delivering protected code/UI/state and before
accepting operations, including comments. Membership removal, audience restriction
and attachment removal invalidate affected active sessions; reconnect must check
current policy. Browser and native hosts use the same decisions. An unattended
Frame needs an explicit Space-bound identity, not unrestricted access to all
closed Spaces through trusted loopback.

## Current implementation and transition

Today bundles live under a signed release's `authority/bundles`, installation
generates Home/catalog snapshots, and data is persisted through app-specific
state references. Monitoring uses a cookie session in browsers and a trusted
native listener. There is no Space registry, membership system, runtime directory
watcher or collection-deletion protocol. Existing `Info.flex.private` and
`SessionAccess` support that bounded implementation; they do not implement this RFC.

Introduce explicit Space records and bindings before moving existing data. Map
current public demo roots deliberately, and put Monitoring in a closed Operations
Space. Do not infer Admin membership from a legacy shared login or silently grant
the native host access to every Space. Preserve confirmed data during the cutover.
The bootstrap administrator and unattended Frame identity provisioning require
an explicit rollout plan.

Keep one shared session and release-control v1 independent of Space admission or
data recovery. Version any new management contracts; bump the state protocol if
its wire format changes and keep browser/native codecs consistent. Code rollback
must preserve confirmed edits and deletion records. Do not roll back to a writer
that can ignore the new access or deletion boundaries.

## Proposed milestones and acceptance

These are planning gates, not authorization to implement all stages in this
documentation change. Scope the next implementation to one milestone.

| Milestone | Required evidence |
| --- | --- |
| S1: Spaces and admission | Two Spaces using the same app with isolated data; multiple Admins; concurrent last-Admin protection; Member management denial; unauthorized reads/writes denied in separate client processes; revocation on active sessions and reconnect; explicit Frame identity |
| S2: Apps & Data and launch | Initial snapshot plus observation without lost changes; incomplete bundles never discovered as ready; failed/racing launches expose no partial view; Quit affects one participant; removal retains visible data; reattachment reuses it; local uninstall preserves Space data |
| S3: Permanent collection deletion | Admin-only scope; write/delete race; crash/restart recovery at each management boundary; active invalidation; stale offline edits rejected; fresh replacement ID; shared-blob isolation; backup restore cannot resurrect deletion; truthful cleanup/retention reporting |
| S4: Public audiences and publishing | Closed/public-view/public-comment enforcement; comments cannot edit documents; source drafts/assets remain inaccessible; publication updates explicit; source/destination deletions independent; closed-only apps block public audience changes |

Run required repository checks for each implementation. Deployed behavior needs
real browser and Frame acceptance through the existing signed-release and rollback
workflow; scaffold tests alone are not acceptance. Runtime enforcement and tests
belong in LGPL `otrios-core`; contracts, schemas, docs and thin tooling remain Apache.

## Deferred decisions

- Concrete management encoding, bounds and identity provisioning for S1.
- Whether returning Home keeps a local app running, pauses it or quits it;
  explicit Quit always has the local-only meaning above.
- App-specific migration/export formats and ordinary-content trash retention.
- Backup retention, deletion-record retention and physical reclamation policy.
- Invitation-only read access, per-document ACLs, multiple instances per app,
  federation and automatic cross-Space synchronization.

None of these deferrals changes the central rule: collaborate within a Space;
use another Space when the audience changes.
