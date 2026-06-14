# Communication — conversations (DMs & chat channels)

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 024

## Context
Players need to talk: a durable, WhatsApp-style **conversation** surface — direct-message threads + group
chat channels (global + alliance) — that is **server-authoritative** (P4), **persisted** (P2/P6), and
delivered **live**, on the existing stateless/DB-as-truth architecture (P5).

## Design
- **One conversation model.** A conversation is a **DM** (1:1, keyed viewer-relatively as `dm:<other>`) or a
  **channel** (`global` / `alliance:<id>`). The UI + use-cases treat them uniformly (a conversations list +
  a thread view); storage keeps DM lines (`direct_messages`, with explicit sender+recipient) separate from
  channel lines (`chat_messages`), joined only at the read-model level (the conversations list).
- **Pure rules (`domain/comms.rs`, P3).** `ChatChannel` + `can_access_channel(channel, membership)` (global
  open; alliance gated by membership) and `valid_body` (non-empty, ≤ `MAX_MESSAGE`; no subjects). Access +
  validation are the server's (P4).
- **Read state, per viewer.** `conversation_reads(player, conversation, last_read_at)`; a conversation's
  **unread** is its messages after `last_read_at` not sent by the viewer. Opening advances the watermark
  (the WhatsApp badge behaviour). The nav badge polls `/messages/unread` (total).
- **DM keying.** The conversation key crosses the pure↔DB boundary, so the application encodes a DM key as
  `dm:<uuid>` (the player's id in the DB's uuid form) — `dm_key`/`parse_dm_key` own the codec; channel keys
  are opaque text passed through verbatim. This keeps the SQL read-watermark join and the stream key
  consistent without any uuid↔u128 conversion in SQL.
- **Live delivery (SSE + `LISTEN/NOTIFY`, P5).** A send is an ordinary `POST` that **persists then
  `pg_notify('comms', …)` in one statement** (the row is the source of truth; the notify is the live nudge).
  One **`PgListener` per web instance** (`run_chat_listener`) receives notifications and republishes them to
  an in-process **`ChatHub`** broadcast; each SSE handler (`GET /messages/stream/{key}`) subscribes and
  forwards the lines whose `keys` include the conversation it streams. A DM notify carries **both** parties'
  viewer-relative keys so each side's stream matches. The DB is the bus → correct across **multiple web
  instances** with no Redis and no sticky sessions; a dropped notification never loses a message (it's in
  the table; the next page load shows it).
- **Anti-abuse for free.** Sends are mutating `POST`s, so the 021 round-freeze + 022 sanction `action_guard`
  and the 022 rate limiter already apply — a frozen/banned player can't send and floods are throttled. SSE
  streams + reads are `GET`s and stay open.

## Persistence (migration 0035)
- `direct_messages (id, world_id, sender_id, recipient_id, body, created_at)` — a DM thread is every row
  with `{sender,recipient} = {A,B}`; two directional indexes serve history + the conversations list.
- `chat_messages (id, world_id, channel, sender_id, body, created_at)` — `(channel, created_at)` index.
- `conversation_reads (player_id, conversation, last_read_at)` — per-viewer read watermark.

## Reuse / decisions
- **SSE over WebSockets:** receive is one-way; send is a normal `POST` (so auth/sanction/rate-limit
  middleware apply unchanged). Simpler, testable over plain HTTP, P5-correct via `LISTEN/NOTIFY`. WS remains
  a future option.
- **Persist-then-notify:** durable first, live on top — history + moderation trail are never at the mercy of
  delivery.
- **History at page load, deltas over SSE:** the conversation page renders recent history server-side; the
  SSE stream carries only new lines (the client appends them).

## Consequences
- A new communication surface with no new enforcement path and no new infra dependency (Postgres is the
  message bus).
- The conversations list is a grouped read over the viewer's DM rows + two channel lookups — bounded +
  index-backed (P11). SSE clients are cheap broadcast receivers; only the single per-process listener holds
  a dedicated connection.
- Moderation of content reuses 022 (report a player → sanction); deletion/edit/reactions/presence are future
  work.
