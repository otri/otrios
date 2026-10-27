# RFC 0022: A playful shared chat room

Status: proposed for design review, 2026-10-27. This document and the interactive
mockup are planning work only; implementation and deployment are deferred until
the design is agreed. Native and browser ship the same room and behavior.

## Experience

Add **Chat Room** to Home alongside Test Playground and Monitoring. The current
design direction is **The Brass Relay**: a Steampunk telegraph room with the topic
“Good company. Questionable engineering.” Keep Home's compact icon, title and
subheading cards. This is a shared app, rather than another Playground demo.

The user prefers the **Lounge** layout: a roomy message stream, an IRC-style
member rail, a small live-draft stage and a compact composer. On narrow displays,
Members opens a sheet; typing remains visible above the composer. Avoid a channel
sidebar while there is only one room. Day separators show only Today, Yesterday
or the date on day changes; omit the redundant channel-name breadcrumb.

### Theme direction: Steampunk Lounge, with Lunar preserved

The user has a Steampunk identity and requested that the Lunar theme remain
available for review before replacing anything. **Do not erase the Lunar design.**
The later feedback favors a stronger Steampunk treatment in Lounge mode; it
does not change the shared room mechanics. The original **Moon Lounge** Lunar
mockup remains available for comparison.

The Steampunk direction is **The Brass Relay**, `#brass-relay`: a shared telegraph
room with brass fittings, copper accents, dark enamel and warm paper. Keep the
chosen Lounge layout, using an engraved serif nameplate, restrained brass bevels,
clockwork emblems and a telegraph-style incoming draft strip. Keep ornament sparse,
touch targets large and timestamps readable; the genre must not make navigation
or moderation cryptic. A separate Terminal/Telegraph Office variant is no longer
needed for the Steampunk preview.

Possible elements to compare with Lunar:

| Element | Lunar | Steampunk |
| --- | --- | --- |
| Avatars | Space oddities, snacks, animals, tiny machines | Clockwork animals, airships, diving helmets, automatons, peculiar inventions |
| Live drafts | Incoming transmissions with small caret sparks | Telegraph transmissions with copper sparks and an optional brief steam wisp |
| Member list | On the moon; standard `@` Admin markers | Operators on the wire; the same `@` markers and explicit Admin labels |
| Ambient character | Low gravity, snack jokes, confetti | Engraved headings, restrained mechanical details, dispatch-style notices |
| Command flavor | `/boop`, `/confetti` | `/salute` and `/steam` as bounded cosmetic alternatives |

Avoid full-screen moving gears, permanent steam, physical dial controls for
ordinary text actions, or invented telemetry gauges. Both themes use the same
particle budget and reduced-motion behavior. A separate interactive Brass Relay
preview shows the current direction without overwriting the Lunar source.

### Shared behavior

- Join immediately as a funny name, such as Professor Piston, Lady Gearbox or
  Captain Sprocket. Change the name and avatar through your profile button.
- Offer 64 distinct, named, bundled vector avatar designs: clockwork animals,
  airships, diving helmets, automatons and peculiar inventions. Palette choices
  are additional, not counted
  as different icons. Cache their masks; require no downloaded images or emoji
  font support. The mockup previews a subset of this proposed collection.
- Show avatar, name, timestamp and wrapped body for each message. Use `@` plus
  an explicit Admin label for operators. Sort admins first, then normalized name;
  do not reorder the roster on every keystroke. Tapping a member opens actions.
- Clearly disclose **Live drafts on — everyone can see what you type** before
  the first keystroke. A composer toggle permits private composition. Shared
  drafts are temporary, visually distinct from sent messages, and never history.
- Keep the empty composer to one compact row: command key, expanding text field,
  Live/Private control and Send. Remove the redundant room label and permanent
  helper/status paragraphs. Show the disclosure once on first focus and retain
  an accessible description and privacy tooltip. Switching Live/Private updates
  the control without a separate confirmation notice. Other brief operation
  feedback overlays the draft area and expires; it does not enlarge the composer.
- Keep up to three active remote drafts in a separate stage, with an “N more
  typing” summary. Updating a draft must not move previously sent messages.
  Screen readers get a debounced typing summary, not every draft character.
- Emit small copper caret sparks while typing, a hat-tip effect for `/salute`,
  and a brief puff for `/steam`. No perpetual animation or sound. Respect
  reduced motion and offer Quiet / Playful / Off effects without hiding content.
- New messages follow the bottom only when already there. While reading older
  messages, preserve the reading anchor and show an “N new messages” button.
  Sending your own message returns to the bottom; this is an explicit action.
  Keyboard opening, width changes and prepending history preserve source anchors.
- Show connection state and pending/failed sends. Disconnect clears stale remote
  drafts; reconnect restores membership and authoritative history. No fake peers
  or automatic bot conversation in the implemented room.

The mockup simulates peers, role, sends and effects locally. Its design controls
may preview Admin versus Guest; the production app has no client-side role switch.

## Commands and moderation

Commands are optional shortcuts for visible controls. Open a small command menu
on `/`; Tab completes commands and unambiguous names. Stable participant IDs,
not display names supplied by the client, identify moderation targets.

| Command | Behavior |
| --- | --- |
| `/help` | Private command reference, including the live-draft notice |
| `/nick name` | Change your name; roster and subsequent messages update |
| `/avatar` | Open the avatar picker |
| `/who` | Open Members, with operator markers and connection status |
| `/me action` | Send a plain-text action, e.g. “Professor Piston oils the clockwork pigeon” |
| `/roll [NdM]` | Authority chooses and publishes a bounded dice result; default 1d20 |
| `/shrug [text]` | Append a bundled text shrug to an ordinary message |
| `/salute name` | Send one small, rate-limited hat-tip effect, never change input focus |
| `/steam` | One bounded steam puff; receiver effect preferences still apply |
| `/topic text` | Admin changes the topic; publish a system event |
| `/kick name [reason]` | Admin previews target and reason, then confirms removal |

Typing a command prefix never broadcasts its arguments as a live draft. Draft
sharing resumes only after the composer becomes ordinary text. Unknown commands
fail privately; `//` sends a literal leading slash. Plain text and typed action
messages ship first; HTML, uploads, direct messages, multiple rooms, arbitrary
Markdown and a general IRC server are outside this milestone.

Monitoring sign-in grants `can_kick` and `can_set_topic`, computed by the server
from the current Monitoring session. Validate at each action and before emitting
privileged data. Logout and expiry revoke rights on existing connections; a
cached badge cannot authorize a command. Never copy a Monitoring grant, cookie
or credential into a public profile, document or room event.

The native trusted listener currently admits private Monitoring without the
browser's login session. That is a separate trust mechanism. Proposed policy:
native room users start as guests and explicitly sign in through Monitoring to
obtain a connection-bound, expiring grant. Its opaque credential stays in native
private RAM; reconnect reauthenticates or resumes only within its validity. Do
not silently make every native/trusted client an operator. Implementing this
requires an explicit native auth flow, not just reusing the existing trust flag.

A kick atomically ends the target's room membership, clears its drafts/effects,
disconnects **all** of its room connections, and records a system event. It does
not terminate unrelated apps on the shared socket: revoke/unsubscribe the room
channel and show a kicked screen. Include the membership generation in the
confirmed action to avoid applying a stale user-menu action to a later join.
Require explicit Rejoin after a proposed 60-second participant cooldown; automatic
reconnect cannot rejoin early. Persist the cooldown across authority restarts.
A kick is not a ban: clearing guest identity storage can create another guest.
Long-term bans, IP reputation and stronger identity belong in a later decision.

## Participant identity

Separate three concepts: a persistent guest identity, each transport connection,
and an authenticated Monitoring session. A funny name is not a unique key.

The authority issues a random 128-bit public participant ID and a separate
256-bit secret resume credential; retain only its SHA-256 digest server-side.
Bind every connection to the verified identity. Do not accept another guest's
public ID as proof of ownership. Public events expose the ID, never the secret.

- Browser: a persistent, host-only `Secure; HttpOnly; SameSite=Strict; Path=/`
  guest cookie carries the resume credential. The returned public ID and harmless
  presentation preferences may be cached locally. Provision through an
  origin-checked same-origin join endpoint; bound admission and cookie input.
- Native: an atomic private file (mode 0600) stores identity/resume information
  outside executable and release caches. Updating or pruning a release must not
  change the participant. Native passes the credential only on the trusted
  transport; never put it in a URL or log.
- Persist the bounded profile/credential-digest store independently of chat
  history and releases. Initial proposed policy: expire after 30 days inactive,
  cap at 4,096 profiles, prune expired entries first, and return an explicit
  admission failure rather than evicting an active or cooldown identity.
- Without persistent storage, allow an explicitly temporary guest. Reloading or
  restarting can then change identity; do not promise stable identity in that mode.
- Multiple tabs share one member entry and multiple connection IDs. The most
  recently focused composer acquires a server-issued draft generation; stale
  writers cannot overwrite or resurrect a newer draft. Sent-message nonces stay
  unique across those connections. A moderator's role belongs to its authenticated
  connection, not every tab carrying the same guest cookie; the public Admin
  badge means that member has at least one currently authenticated connection.

Normalize names and check collisions atomically. Start with 24 grapheme clusters
and 96 UTF-8 bytes, reject controls and misleading operator prefixes, and append
a short stable discriminator to colliding display names. Render all names as
text. Message records snapshot the sender's display name/avatar at send time;
renaming does not rewrite history. The shared text path must preserve clusters
and provide explicit missing-glyph fallback; native Unicode shaping coverage
must be tested rather than inferred from a browser mockup.

## State, delivery and bounds

Reuse one `/session` WebSocket and its stable v1 opening hello/upgrade envelope
([RFC 0009](0009-unified-server-session.md)). Add a separately framed chat message
family after compatible negotiation; do not overload ephemeral ink channel 2.
Allocate a currently unused channel during implementation. Bump the declared
state protocol from sync v5 in browser, native and server together. Release
discovery must continue to work with obsolete chat/state clients.

Keep three bounded lanes with independent budgets:

| Lane | Contents and delivery |
| --- | --- |
| Durable room | Authority-ordered messages, action messages, topic and moderation events; paged by room sequence |
| Profile / membership | Persistent guest profile, current connections, permissions and membership generation; bounded snapshots/deltas |
| Ephemeral | Latest live drafts, typing status and effect triggers; coalesce superseded revisions and expire stale data |

Reuse existing app mount and receipt infrastructure where it fits, but do not
put every keystroke or the whole growing transcript into a repeatedly replicated
app document. A small room state references bounded immutable history pages.
The authority serializes appends; this does not require implementing general
transactions or a new CRDT engine. Runtime logic/tests belong in LGPL core;
public packet contracts remain Apache interfaces. Exact packet schemas and
storage adapter changes are part of the approved implementation, not this mockup.

Each sent message includes a random client nonce. Within an identity, retries of
that nonce return the original receipt/sequence; a changed payload conflicts.
Publish a durable ACK only after the append is durable. Recover index and dedupe
receipts together after a crash. Keep dedupe receipts at least for the admitted
retry window; after that window reject stale retries rather than appending them
anew. Bind each nonce to a server-issued acceptance epoch with a bounded expiry;
retain its receipts until that epoch's retry window closes. Random nonce bytes
alone do not establish when an unseen retry originated. Client pending sends are
RAM-only and follow current release-upgrade policy.

Proposed initial ceilings, to tune with measurements:

| Resource | Initial bound |
| --- | --- |
| Message | 2,048 UTF-8 bytes, 1,024 grapheme clusters, 16 explicit lines |
| Transcript retention | 50,000 events or 32 MiB stored records, whichever comes first; trim whole old pages and report the retained boundary |
| History response | At most 64 events / 128 KiB; request by sequence range |
| Draft | At most 256 grapheme clusters / 1,024 UTF-8 bytes; throttle to 10 updates/s; local composer remains immediate |
| Draft lifetime | Three seconds without activity; active writers refresh once/s; clear immediately on send, cancel, leave or kick |
| Public effects | Two triggers/s per participant, with room-wide and receiver caps |
| Message sends | Burst of five, sustained one/s per participant; explicit retry feedback |
| Participants | Reuse current connection admission; qualify 16 clients / three simultaneous typists first, without promising the admission ceiling as performance capacity |

Use monotonic draft revisions, membership/writer generations and ordered clear
events. A delayed update cannot restore a sent or kicked user's draft. Server
time determines expiry; old packets do not acquire a fresh lifetime on receipt.
Slow readers lose obsolete drafts/effects first. Bound history/control queues
separately, paginate on resync and disconnect a persistently stalled room
subscription without blocking release control. Rate limits aggregate by verified
identity across tabs; global connection/provisioning limits bound fresh guests.

## Retained message rendering

Build on the [chunked document reader](0020-chunked-text-documents.md): stable
message blocks, cached measured runs, a height index, retained glyph chunks,
viewport offsets, ancestor clips and whole-block/line/glyph culling. Scrolling
inside the resident window must generate no new glyph geometry. Append changes
only the new tail, affected visible layout and the height index.

The reader currently ships the plain-text first delivery; its structured block
adapter is still deferred. Implement the narrow message-block adapter needed
here, not an assumption that rich message rows already exist. Keep message ID,
body spans, header, avatar and paint dependencies distinct. Plain text plus an
action kind is enough initially; reserve span/run references for later links,
mentions, emphasis and other rich text. Do not add thousands of ordinary scene
nodes or rebuild the transcript when a profile or draft changes.

Retain the reader's 16 MiB CPU / 8 MiB GPU working-set caps, counting chat layout,
headers and avatar resources explicitly; history source pages have their own
bounded residency. Keep roughly one viewport behind/two ahead, page older
messages before reaching the boundary, and cancel obsolete layout epochs. Use
message ID + source cluster + viewport-relative y as the reading anchor. Preserve
the reader's scheduling/upload budgets and f64 origins with local f32 geometry.
No fixed-point pilot or new SIMD work without a measured bottleneck.

Drafts and effects have independent retained surfaces. Pool at most 128 simple
particles per client and batch their quads with existing GLES2 facilities; do
not assume instancing support. Emit only for visible caret/draft positions and
clip to the room. Drop excess effects rather than queue them. Stop scheduling
animation once the pool is empty. Reduced motion disables particles while
retaining readable drafts and command results. Effects never force scrolling.

## Three meaningful deliveries after design approval

1. **A usable shared room:** browser/native app, stable guest identities and
   profiles, avatar collection, paged durable transcript, roster, composer and
   retained anchored scrolling. Include private `/help`, `/nick`, `/avatar`,
   `/who`, literal slash handling and send/reconnect feedback. Qualify append,
   paging and restart behavior before adding live drafts.
2. **Live personality and moderation:** bounded disclosed live drafts, private
   composition, pooled effects, the remaining commands, Monitoring-backed roles,
   explicit native sign-in and complete kick/rejoin/logout behavior. Server
   enforcement and tests ship with the UI, not in a later security stage.
3. **Device qualification and release:** replay load and touch trajectories,
   tune the budgets, compare pixels and frame times, then publish one verified
   signed release to existing native/web hosts with rollback evidence.

No progress PRs. Open a PR only when its scoped delivery and required validation
are complete, in accordance with AGENTS.md. This proposed plan does not authorize
production implementation before the user's review.

## Acceptance evidence

- Browser + native + another browser share one actual room across real process
  boundaries. Reload, reconnect, authority restart and release/cache updates
  preserve identity as specified. Two tabs show one participant. Private-mode
  temporary identity and expired/failed storage are visible, tested cases.
- Guests cannot claim another ID or self-grant Admin. Monitoring login/logout/
  expiry changes permissions on an existing room connection. Native trust alone
  does not grant moderation. Confirmed kick removes every target room connection,
  clears drafts and enforces persisted cooldown without breaking release control.
- Retry-after-lost-ACK, duplicate nonce/conflicting payload, crash recovery,
  concurrent append, stale generations, out-of-order drafts, snapshot isolation,
  slow readers and retention trimming produce bounded, correct state. No claim
  of Linux IPC acceptance from in-process tests.
- Long-history scrolling, appends while reading above the tail, prepending,
  reflow, keyboard opening, giant admitted messages, gesture reversal and repeated
  strokes retain anchors and clips. Static screenshot fixtures compare normal,
  narrow and large-text rendering; deterministic effects use a fixed seed/clock.
- Measure actual native presented frames, CPU layout, geometry upload, queue
  bytes and memory with 16 peers / three active typists, effects off and on.
  Target at least 30 fps on a screen full of text and report p50/p95/p99 frame
  times and misses, not just average fps. Compare the current reader baseline;
  this is a target, not a result established by the mockup.
- `bash scripts/check.sh`, protocol compatibility/release discovery tests and
  real browser/native interaction checks pass before signed deployment. Record
  source/release/binary hashes, health and rollback verification. Screen-reader,
  keyboard, touch, narrow widths and reduced motion are part of sign-off.

Grounding: [public access and Monitoring auth](../PUBLIC_ACCESS.md),
[ephemeral ink transport](../TOUCH_PAD_INK.md),
[touch history and momentum](0021-touch-history-and-momentum.md), and
[chunked document qualification](../experiments/chunked-documents-2026-10-27.md).

## Plan and mockup review checks

On 2026-10-27, `bash scripts/check.sh` passed with local process/socket access;
the sandboxed attempt had denied the existing process-boundary socket tests.
`git diff --check` and the RFC's relative-link check passed. The Lunar mockup
passed 16 real Chrome layout cases (Lounge/Terminal, light/dark, 1,024/736/360/320px)
and local interactions for escaped text, profiles/avatars, private composition,
dice, particles, kick cleanup and help. Desktop and narrow profile screenshots
were visually inspected. These validate the plan/preview, not implemented chat
transport, authentication, persistence or native performance.

The revised Steampunk preview keeps Lounge as the sole layout, with a compact
composer and date-only separators. Its browser checks cover light/dark appearance,
desktop/phone widths, profiles, private drafts, commands, effects and kick cleanup.
