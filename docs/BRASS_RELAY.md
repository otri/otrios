<!-- SPDX-License-Identifier: Apache-2.0 -->
# The Brass Relay

Open **The Brass Relay** on Home, or `/brass-relay`, to join the shared lounge.
Browser and native use the same Rust presentation, text reader and touch physics.
Home shares its neutral charcoal surfaces, OTriOS headings and restrained brass
and copper accents. The Lunar design preview remains preserved for comparison.
See the shared [visual language](VISUAL_DESIGN.md) and
[visual refresh qualification](experiments/visual-refresh-2026-10-27.md).
[Phone readability and retained profiles](experiments/phone-readability-and-profiles-2026-10-27.md)
records the subsequent density, author-resolution and last-seen changes.

You join with a random funny name. **You** opens your calling card: rename yourself,
pick from 64 distinct copper-line avatars and choose Off, Quiet or Playful effects. **Members**
opens the operator list; wide screens also have a member rail. `@` marks a member
with an authenticated administrator connection. Member rows have no subtitles.
Name and avatar changes refresh loaded message authors by stable participant ID.
History pages resolve the current profile, including after an authority restart;
the journal retains the original sender snapshot. If the retained guest identity
has expired, that snapshot is the fallback. Message text remains as written.

The composer stays compact. **Live** shares a temporary draft before sending;
the first focus discloses that behavior. Switch to **Private** to clear the shared
draft and compose privately. Sent messages are separate durable records. Remote
drafts expire after three seconds without renewal, and disappear on disconnect,
leaving the room or removal. Slash commands are never shared as live drafts.
Typing sparks and steam have a bounded pool; reduced-motion preferences disable
them. Privacy toggles add no confirmation paragraph.

Drag the transcript to read older dispatches, with momentum on release. New
messages preserve your reading position; the new-dispatch control returns to the
latest dispatches. The transcript selects a coherent collection window of at
most 64 messages and 256 KiB of canonical message bytes. Moving near a window
edge replaces its coverage instead of accumulating pages. Resizing reflows
the selected records locally. Day separators show Today, Yesterday or the
date; this first version uses UTC for dates and timestamps. The composer supports
wrapped text, caret following, hardware editing and multiline paste. It grows to
four lines. The native frame supports vertical drag/coast preview and retains
its primitive keyboard. Browsers use a real textarea with native scrolling,
selection and the system keyboard on iPhone; Shift+Enter inserts a newline and
Enter sends after composition finishes. Profile names use native input fields.
Phone browser widths below 600 CSS pixels use approximately 1.5× larger text and
controls, with the member/profile actions below the heading.
Intermediate IME text stays private until committed. Live drafts wrap too. See
[browser input](HOME_AND_INPUT.md) for the host/editor boundary. Typing sparks originate at the visible edit
position, including replacements inside a remote draft; lease renewals do not
emit sparks.

| Command | Behavior |
| --- | --- |
| `/help`, `/who`, `/avatar` | Open the handbook, members or your calling card |
| `/nick Name` | Change your name |
| `/me action` | Send an action |
| `/roll 2d6` | Roll up to ten dice with 2–100 sides |
| `/shrug` | Send a shrug |
| `/salute Full Name` | Salute a member |
| `/steam` | Vent a little steam |
| `/topic text` | Administrator room announcement |
| `/kick Full Name` | Select a member and confirm removal |
| `//text` | Send a literal leading slash |

Signing in to Monitoring grants administrator rights to that authenticated
browser connection. Native members can explicitly enter Monitoring credentials
from Members. Trusted native transport alone grants no chat moderation rights.
Select a member, choose Kick and confirm: all their room tabs leave, their drafts
clear, and a persisted 60-second cooldown applies. This is a guest-session kick,
not a durable ban against someone discarding their guest identity.

Browsers retain identity through a Secure, HttpOnly, SameSite=Strict guest cookie;
localStorage holds only the public profile. A browser that cannot retain the
cookie can join temporarily, marked in its calling card. Native stores a separate
mode-0600 `guest.identity` outside digest-named executable caches. The frame
supervisor supplies `/var/lib/otrios-dashboard/guest.identity`; other native hosts
can use `--chat-identity` or `OTRIOS_CHAT_IDENTITY`. Guest resume secrets never
grant administrator privileges or enter presentation snapshots.

The covered authority lives in `otrios-core::canvas::chat`; the dashboard host
only adapts cookies, authentication, bounded queues and the shared WebSocket.
Room commands, receipts and ephemeral events use channel 3; coherent message
windows use the reusable collection channel 4 under state protocol v8.
Release discovery keeps its stable
v1 opening envelope. A failed chat journal closes the room while release/control
and other apps remain available. Slow room queues explicitly resynchronize.

The legacy History command, welcome bodies and per-message broadcasts are
retired. The same authoritative collection feeds both hosts; pending sends stay
in their separate eight-intent queue until a durable receipt. Connected code
replacement and navigation wait for those receipts. A retention-expired anchor
moves to the first retained key at or after it, or the last retained key if it
is beyond the tail, with an explicit notice. Immutable message blocks keep each
sent label, body and avatar together. Overlapping windows reuse their layout
chunks; the avatar and text share placement and clipping on both hosts. The frame/pixel, large-thread, journal rollback/restart and signed deployment
gates of [RFC 0024](rfcs/0024-viewport-scoped-collections.md) are complete; see the
[qualification ledger](experiments/collection-windows-2026-10-28.md) for commands,
measurements and remaining renderer/human-interaction limits.

The mode-0600 `brass-relay.journal` lives beside the authority's app state. Records
have lengths and SHA-256 checksums; writes sync before receipt publication. A
retry with the same nonce returns its original receipt; conflicting text is
rejected. An incomplete final record is truncated on recovery; complete corrupt
records fail closed. Atomic compaction bounds the journal at 64 MiB. Retained
history is capped at 50,000 messages and 32 MiB; inactive identities expire after
30 days, with a 4,096-identity ceiling. Message, draft, command and connection
limits are enforced by the authority rather than trusted to the UI.
Leaving the room removes online membership while retaining the profile. The
private guest record's `last_seen_unix_ms` is authority-clock Unix milliseconds;
it keeps the existing journal key `seen`. Quiet connected readers checkpoint
presence at most once per minute, and departure records the final timestamp.
Joins, sends and profile changes also refresh it. Clock corrections cannot move
the stored value backwards. This wall-clock value supports retention and dates;
message ordering continues to use sequence numbers.

The first version retains the document reader's single-font LTR typography and
whole-grapheme fallback. It does not add general shaping, bidirectional layout,
Unicode normalization, rich text, attachments, multiple rooms or offline durable
outboxes. See [RFC 0022](rfcs/0022-playful-shared-chat-room.md) for the design and
[qualification](experiments/brass-relay-2026-10-27.md) for actual checks and limits.
