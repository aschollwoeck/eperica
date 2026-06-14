# Feature 024 — Communication: messaging & chat — Plan

**Spec:** ./spec.md · **Status:** Reviewed

How the spec maps onto the layered architecture (P3). Mail is ordinary persisted CRUD; chat adds a thin
realtime layer (SSE + `LISTEN/NOTIFY`) on top of a persisted record. Reuses the 021/022 action guards for
send-time enforcement and the 022 rate limiter for anti-spam — no new enforcement path.

## Domain (pure, P3) — `crates/domain/src/comms.rs`

- `enum ChatChannel { Global, Alliance(AllianceId) }` with string round-trips (`global`, `alliance:<id>`)
  for persistence/URLs, and `fn can_access_channel(channel, membership: Option<AllianceId>) -> bool`
  (global ⇒ always; alliance ⇒ member of that alliance).
- Validation: `fn valid_message(subject, body)` / `fn valid_chat(body)` (non-empty after trim, length caps
  `MAX_SUBJECT`/`MAX_BODY`/`MAX_CHAT`). Pure; unit-tested.

## Persistence (migration `0035`)

- `messages (id uuid pk, world_id, sender_id → users, recipient_id → users, subject text, body text,
  created_at, read_at timestamptz, sender_deleted bool, recipient_deleted bool)`. Indexes:
  `(recipient_id, created_at DESC) WHERE NOT recipient_deleted`, `(sender_id, created_at DESC) WHERE NOT
  sender_deleted`.
- `chat_messages (id uuid pk, world_id, channel text, sender_id → users, body text, created_at)` —
  index `(channel, created_at DESC)` for history backfill.

## Application (ports + use-cases)

- **`MessageRepository`** (mail): `send_message(sender, recipient, subject, body, now) -> Result<…>`
  (validates recipient exists + not abandoned + not self), `inbox(player, limit)`, `sent(player, limit)`,
  `message_by_id(id) -> Option<MessageView>` (+ guard caller is a party), `mark_read(id, recipient, now)`,
  `delete_for(id, player)` (per-side), `unread_count(player)`. Default no-ops so non-mail fakes are
  untouched. `MailError` (NotFound, SelfSend, RecipientUnavailable, NotAuthorized, Backend).
- **`ChatRepository`** (chat): `post_chat(channel, sender, body, now) -> Result<ChatMessageView>`,
  `chat_history(channel, limit) -> Vec<ChatMessageView>`. The use-case `post_chat` gates access via the
  pure `can_access_channel` (reading the sender's alliance via `AllianceRepository::alliance_of`).
- **`crates/application/src/comms.rs`** use-cases wrapping the above with validation + access gates +
  `CommsError`.

## Realtime (infrastructure) — SSE + `LISTEN/NOTIFY`

- A **`ChatHub`** (`tokio::sync::broadcast::Sender<ChatEvent>`) held in `AppState`. One background task per
  process runs a `sqlx::postgres::PgListener` on channel `chat`; on each `NOTIFY` it parses the payload
  (channel + message) and publishes to the broadcast. The web SSE handler subscribes to the broadcast,
  filters to the channels the player may access, and streams events.
- **`post_chat`** (repo) inserts the row **and** `pg_notify('chat', payload_json)` in one statement/tx, so
  persistence is the source of truth and the notify is the live nudge (DB-as-truth + cross-instance, P5).
- Listener task started in `main.rs` (and the test harness) next to the scheduler.

## Web (`crates/web`)

- Mail: `GET /messages` (inbox), `/messages/sent`, `GET/POST /messages/compose`, `GET /messages/{id}`
  (marks read), `POST /messages/{id}/delete`. Unread badge in the nav (a small count read per page render).
  A “message this player” link on the player-stats page (016) prefilling the recipient.
- Chat: `GET /chat` + `GET /chat?channel=alliance` (page, server-rendered history + the live region),
  `POST /chat/send` (persist + notify), `GET /chat/stream?channel=…` (**SSE**, `axum::response::Sse`:
  emits backfilled history then live events). Channel access checked server-side on both send + stream.
- All send `POST`s flow through the existing `action_guard` (sanction/freeze) + `rate_limit_guard` (022).
- `AppState` gains `Arc<ChatHub>`.

## Reuse / decisions

- **SSE over WebSockets:** one-way server→client fits chat-receive; send is a normal `POST` (so auth +
  sanction + rate-limit middleware all apply unchanged). Simpler, testable over plain HTTP, and P5-correct
  via `LISTEN/NOTIFY`. WS remains a future option (spec §Out of scope).
- **Persist-then-notify:** chat is durable first (history, moderation, P2/P6); live delivery is best-effort
  on top — a dropped NOTIFY never loses a message (it's in the table; the next history read shows it).
- **Anti-abuse for free:** sends are guarded + rate-limited by 021/022; no new code.

## Risks / testing

- **Realtime test:** an integration test connects an SSE client (a streaming `reqwest` GET), posts a chat
  message via `POST`, and asserts the event arrives on the stream + the row persisted — exercising the
  hub/listener/notify path end-to-end. History-backfill + access-denial are DB/handler tested.
- **Per-instance listener:** one `PgListener` per process (not per connection) — SSE clients are cheap
  broadcast receivers; only the single listener holds a dedicated connection.
- **Ordering/My-own-mail guards:** repo reads filter by the acting player; `message_by_id` + delete verify
  the caller is sender or recipient (P4).
