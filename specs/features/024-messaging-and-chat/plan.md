# Feature 024 — Communication: conversations — Plan

**Spec:** ./spec.md · **Status:** Verified

A WhatsApp-style conversation model (DMs + group channels), persisted (P2/P6) and delivered live (SSE +
`LISTEN/NOTIFY`, P5). Reuses the 021/022 send-time guards + the 022 rate limiter — no new enforcement path.

## Domain (pure, P3) — `crates/domain/src/comms.rs`

- `enum ChatChannel { Global, Alliance(AllianceId) }` (key round-trips `global` / `alliance:<id>`) +
  `can_access_channel(channel, membership)`.
- `fn valid_body(body) -> bool` (non-empty after trim, ≤ `MAX_MESSAGE`). No subjects.
- (DM threads are keyed viewer-relative as `dm:<other>`; that key is built in the repo, not the domain —
  the domain owns channel access + body validation.)

## Persistence (migration `0035`)

- `direct_messages (id uuid pk, world_id, sender_id → users, recipient_id → users, body, created_at)` —
  a DM thread between A and B is every row with `{sender,recipient} = {A,B}`. Indexes for the two access
  directions: `(recipient_id, sender_id, created_at DESC)`, `(sender_id, recipient_id, created_at DESC)`.
- `chat_messages (id uuid pk, world_id, channel text, sender_id → users, body, created_at)` —
  index `(channel, created_at DESC)`.
- `conversation_reads (player_id → users, conversation text, last_read_at, pk (player_id, conversation))` —
  `conversation` is the viewer-relative key (`dm:<other>` / `global` / `alliance:<id>`).

## Application (ports + use-cases) — `crates/application/src/comms.rs`

- **`CommsRepository`** (default no-ops):
  - DM: `send_dm(sender, recipient, body, now) -> id`, `dm_history(viewer, other, limit) -> Vec<MessageView>`,
    `dm_threads(viewer) -> Vec<ConversationSummary>` (other party, last body+time, unread).
  - Channel: `post_chat(channel_key, sender, body, now) -> id`, `chat_history(channel_key, limit)`.
  - Reads: `mark_read(player, conversation_key, now)`, `unread_after(player, conversation_key) -> i64`,
    `total_unread(player, alliance) -> i64` (DM threads + global + the alliance channel).
- **Use-cases** with validation + access gates + `CommsError` (Invalid, SelfSend, RecipientUnavailable,
  Forbidden, Backend):
  - `send_dm` (validate body, reject self, recipient exists + not abandoned), `send_chat` (validate body,
    `can_access_channel` via `AllianceRepository::alliance_of`), `conversation_list`, `open_dm`/`open_chat`
    (history + `mark_read`), `unread_badge`.
- `MessageView` + `ConversationSummary` view structs in `ports`.

## Realtime (infrastructure) — SSE + `LISTEN/NOTIFY`

- A **`ChatHub`** (`tokio::sync::broadcast::Sender<LiveMessage>`) in `AppState`. One per-process task runs a
  `sqlx::postgres::PgListener` on channel `comms`; each `NOTIFY` (payload = conversation key + message) is
  republished to the broadcast. The SSE handler subscribes, filters to the **one conversation** it streams
  (already access-checked), and emits events.
- `send_dm`/`post_chat` insert the row **and** `pg_notify('comms', json)`. A DM notifies the
  **pair-canonical** key `dmp:<lo>:<hi>` (sorted uuids) — both parties derive it, only they can, so a third
  party can't subscribe to the thread. A channel notifies the channel key.
- Listener task started in `main.rs` + the test harness.

## Web (`crates/web`)

- `GET /messages` — the conversations list (DM threads + global + alliance), recency-sorted, previews +
  unread. Nav badge = `total_unread`.
- `GET /messages/c/{key}` — a conversation (history + live region + send box); marks read. `key` is a DM
  (`dm:<id>`) or a channel (`global`/`alliance:<id>`), access-checked.
- `POST /messages/send` — `{conversation, body}` → `send_dm`/`send_chat`.
- `GET /messages/stream/{key}` — **SSE**: backfill recent history then stream live events for that key
  (access-checked).
- `GET /messages/with/{player}` — open/create the DM with a player (used by the profile “Message” link),
  redirecting to its conversation.
- All `POST`s flow through the existing `action_guard` (sanction/freeze) + `rate_limit_guard` (022).
- `AppState` gains `Arc<ChatHub>`.

## Reuse / decisions

- **One conversation model** unifies DMs + channels in the UI/use-cases; storage keeps DMs (sender+recipient)
  and channel lines in separate tables, joined only at the read-model level (the conversations list).
- **Two DM key forms:** the **viewer-relative** `dm:<other>` keys URLs + the per-player read watermark;
  the **pair-canonical** `dmp:<lo>:<hi>` keys live broadcast/notify routing (pair-unique, so it can't be
  used to subscribe to a third party's thread). Neither needs a canonical-pair table.
- **SSE over WS / persist-then-notify / anti-abuse for free** — as in the original plan: durable first,
  live on top; the send `POST` inherits 021/022 enforcement + rate limiting.

## Risks / testing

- **Realtime test:** an SSE client (streaming `reqwest` GET) on a conversation receives a posted message
  live + the row persists. History-backfill, channel access-denial, DM party-guard, unread/mark-read, and
  the conversations list are DB/handler tested.
- **Conversations list cost (P11):** `dm_threads` is a grouped query over the viewer's DM rows
  (index-backed by the two directional indexes); channels are two key lookups. Bounded + indexed.
- **One `PgListener` per process** (not per stream); SSE clients are cheap broadcast receivers.
