<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0038: Locally owned Monitoring data and authorized replication

Status: implemented; live cutover acceptance pending. This is a bounded Monitoring milestone,
not general multi-authority writes or a new OS object store.

Monitoring owns typed current values, hourly samples (including gaps), UTC
window metadata and collection time. Frame's collector atomically commits those
values using the existing canonical Merkle Snapshot implementation. Root metadata
binds format, stable collection/feed identity, configured producer and monotonic
revision. Eleven fixed metric children share unchanged canonical nodes. The
latest complete local snapshot survives collector/display restarts. A clock
regression or invalid/partial response retains the prior complete revision.

The renderer derives the chart presentation from this data on both native and
Web hosts. Rendered charts are not the replicated state. Existing normalized
Canvas control fields and their writer receipts are unchanged; this typed data
is the Monitoring collection's owned app data, like the existing app-owned chat
collection, rather than a new global feed namespace.

A producer process initiates an outbound /session connection and uses the stable
v1 hello plus Monitoring publication channel 6. Offer announces the latest root.
After current service-grant authorization, the server replies with its base root
or acknowledges an identical root. Frame sends an existing canonical Merkle delta
when that base is retained, otherwise a bounded complete snapshot. The server
rechecks identity, installed app and collection/attachment binding for every
request, rejects stale/equivocal revisions, validates the complete candidate,
flushes it durably and only then acknowledges/publishes. Hashes confer no access.
Reconnect offers the newest local revision; intermediate dashboard refreshes need
not be queued. Servers preserve the producer's root and revisions.

Managed viewers receive data only on their admitted Monitoring lease through
channel 1; each host computes the same chart projection. State protocol changes
to v42, while release-control v1 remains compatible. Collector publication and
viewing credentials remain separate. The collector cannot administer Spaces or
use its service grant to read unrelated app data. The old collector View upload
path is retired for the newly declared Monitoring data presentation.

An explicitly configured Frame owner can persist its installed Monitoring UI
and app data for local use. This is distinct from remote viewers' RAM replicas:
server-side membership revocation cannot recall locally owned data. Local
installation still verifies UI bytes against the signed catalog and respects
device-local uninstall. No generic offline authorization is granted to browsers.
Collection/display must not block on network publication. When OrbStack is down,
Frame still collects directly from Prometheus and renders local committed data.
When Prometheus/network is down, the saved collection time remains visible and
is not advanced. Other clients need a reachable authorized server for new data.

Acceptance: focused single-producer integrity/replay tests, denied producer and
anonymous/private viewer checks, native and Wasm builds, then actual Frame local
collection/display while the OrbStack authority is stopped, local restart during
the outage, and successful latest-root publication after server recovery.

The ARM collector/publisher is an optional named artifact in the existing signed
release closure. Frame's updater verifies and caches it with executable mode;
the service launcher checks that same publisher signature and artifact hash.
A release-pointer change restarts the producer onto the current verified tool.
No additional publisher trust root is introduced.
