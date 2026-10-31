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

This increment admits Create, Membership, Attach and Closed audience changes.
Removal, deletion and public audience changes remain unavailable through HTTP
until their lifecycle integrations are complete. Global chat/collection/ink lanes
and legacy presentation reads are disabled in managed mode; they cannot be used
as an unscoped fallback. Monitoring presentation uploads require an explicit
collector identity and collection grant.

Separate child clients exercise authority reads/writes, collection isolation,
explicit Frame access, active revocation/reconnect, Member management denial and
concurrent last-Admin protection. These are authority adapter checks, not Frame or
browser qualification. `scripts/check.sh` supplies its freshly built authority to
`otrios-core/tests/spaces_process.rs`. Its test-only `tungstenite` dependency reuses
the pinned workspace WebSocket client; it adds no runtime dependency.

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

Create makes a Closed Space with the authenticated actor as its first Admin.
Every other change requires current Admin membership. The host serializes requests
under one authority lock. An expected-generation mismatch rejects the entire
request. Membership replacement cannot remove the last Admin. An invalid or
denied candidate never changes the published registry.

Space and collection IDs are random 128-bit lowercase hex strings, generated by
the authority. App IDs use the existing admitted destination syntax. A schema
identity is the SHA-256 digest of the complete canonical encoded state schema.
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

Read, Edit, Comment and Moderate are distinct runtime operations. Public read
does not imply editing. Visitor comments require a verified identity and public
comment policy; moderation requires membership. An app can disallow editing or
comments. An attached closed-only app prevents a public audience, and a public
Space rejects attachment of a closed-only app.

## Persistence

The native Store writes `OTSP1`, a 32-byte SHA-256 payload digest, and a FlexBuffers
Registry. Collections are encoded as bounded vectors, not unbounded map keys.
Recovery checks byte bounds, digest, structural nesting/visit bounds, version,
unique IDs, last-Admin invariants and epochs before admitting any data. The digest
detects corruption; it does not authenticate an operator or grant access.

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
