# Feature 024 — Communication: conversations (DMs & chat channels)

**Status:** Verified
**Depends on:** 015 (alliances — the alliance channel + membership), 022 (the sanction/rate-limit guards that gate sending), 016 (player identities the UI links to), 001 (auth/sessions, the DB, the world)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Communication) — player-to-player communication as **WhatsApp-style conversations**: a unified, recency-sorted list of threads (direct messages + group channels) that update live.

## Goal

One coherent communication surface, **server-authoritative** (P4) on the existing stateless/DB-as-truth
architecture (P5) — not a classic inbox/sent split, but **conversations** like a modern chat app:

- A **conversations list**: every thread the player is part of — their **direct-message** threads with other
  players plus the **group channels** they can see (a **global** channel and their **alliance** channel) —
  newest-activity first, each with a **last-message preview**, time, and **per-conversation unread count**.
- A **conversation view**: the running message history, a send box, **mark-as-read** on open, and **live**
  updates (new messages appear within ~a second).

Every thread is **persisted** (durable history + a moderation trail, P2/P6) and delivered **live** on top.
Sending reuses the fair-play guards (022): a **sanctioned/frozen** sender can't send, and sends are
**rate-limited** — no new enforcement path.

## Concepts

- **Conversation.** A thread the player reads/writes, of two kinds:
  - **Direct (DM)** — a 1:1 thread between the viewer and **another player**. From the viewer's side it is
    keyed by the other player (`dm:<other>`); the messages are every line exchanged between the two. Any
    player may open a DM with any other (existing, non-abandoned) player.
  - **Channel** — a group thread: **`global`** (open to all) or **`alliance:<id>`** (members only, 015).
    Channel access is a pure rule of alliance membership (P4).

- **Message.** A line in a conversation: a sender, a body (no subject), a `created_at`. DMs persist with an
  explicit sender + recipient; channel lines persist with the channel key. Persisting first makes the
  thread reproducible (P2/P6) and gives history + a moderation trail.

- **Read state (per viewer, per conversation).** A `last_read_at` per `(player, conversation)`; a
  conversation's **unread** is its messages after the viewer's `last_read_at` not sent by the viewer.
  Opening a conversation advances `last_read_at` to now (the WhatsApp “ticks”/badge behaviour).

- **Conversations list.** For a player: their DM threads (one per other party they've exchanged with) +
  `global` + their alliance channel, each carrying the **latest message** + **unread count**, ordered by
  latest activity. The nav badge is the **total** unread across all of them.

- **Live delivery (SSE + `LISTEN/NOTIFY`, P5).** A send is an ordinary `POST` that **persists** then
  `pg_notify`s. Each web instance runs **one** Postgres listener fanning new messages to its locally
  subscribed conversation streams; clients receive via a one-way **Server-Sent Events** stream (a `GET`).
  The DB is the bus — correct across **multiple web instances** (no Redis, no sticky sessions) — and the
  send `POST` flows through the existing auth/sanction/rate-limit middleware unchanged.

## Acceptance criteria

> Sending, reading, and access are **server-authoritative** (P4) and reproducible from persisted state
> (P2/P6). Live delivery is best-effort on top of the durable record.

- **AC1 — Send into a conversation.** A player posts a (validated, non-empty, length-capped) body into a
  conversation they may access — a DM to **another existing, non-abandoned** player (no self-DM), the
  **global** channel, or their **alliance** channel; it is persisted. Access/validation failures are
  rejected server-side.

- **AC2 — Conversation history.** Opening a conversation shows its messages oldest→newest (recent window),
  each with sender + time. A DM shows the full two-party exchange; a channel shows the channel's lines.

- **AC3 — Conversations list.** A player sees their DM threads + the global + their alliance channel, each
  with the **latest message** preview/time and an **unread count**, ordered by latest activity.

- **AC4 — Read state & badge.** Opening a conversation advances the viewer's `last_read_at`, zeroing its
  unread; the nav badge shows the **total** unread (messages after `last_read_at`, not the viewer's own).

- **AC5 — Channel access.** A player may read/post `global`, and an `alliance:<id>` channel **only** if a
  member; a non-member's read or post is rejected server-side. DM access requires the viewer be one of the
  two parties.

- **AC6 — Live delivery & persistence.** A post persists and is delivered **live** to connected subscribers
  of that conversation (typically within a second); a subscriber **backfills** recent history on connect.

- **AC7 — Fair-play enforcement.** A **sanctioned/frozen** player cannot send (021/022 guards) and sends
  are **rate-limited**; reads/streams stay available.

- **AC8 — Reproducibility, scale & roles.** Threads are persisted (reproducible, P2/P6); live delivery uses
  `LISTEN/NOTIFY` so it is correct across **multiple web instances** (P5). A **Visitor** cannot read or
  send; only a logged-in **Player** can (roles.md).

- **AC9 — Interface.** A conversations-list page (recency-sorted, previews + unread) with a total-unread nav
  badge; a conversation page (history + live updates + send box) for DMs and channels; a **“Message”** entry
  on a player's profile/stats page that opens the DM with them. No client action bypasses access or a limit
  (P4).

## Roles & permissions

Per [roles.md](../../roles.md). Communication is **Player** play; channel access derives from alliance
membership (015); content moderation reuses 022.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — | All conversations (must be a logged-in player). |
| **Player** | DM any other player; read/post global + their alliance channel; read their own threads; mark read. | Read others' DMs; post/read an alliance channel they're not in; send while sanctioned/frozen; exceed the rate limit. |
| **Moderator** | (022) review reported content + sanction senders. | — |
| **Administrator** | (operator) config. | — |

## Out of scope

- **Alliance forum** (threaded persistent boards) — a separate feature; this is conversations.
- **Presence / typing indicators / online lists / delivery receipts beyond unread**, message edit/delete,
  reactions, attachments, **block lists**, group DMs (multi-party ad-hoc), and search — future work.
- **WebSockets** — SSE + `LISTEN/NOTIFY` is the chosen realtime transport (simpler, P5-correct, testable).
