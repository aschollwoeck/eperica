# Feature 024 — Communication: private messaging & real-time chat

**Status:** Reviewed
**Depends on:** 015 (alliances — the alliance chat channel + membership), 022 (the sanction/rate-limit guards that gate sending), 016 (player identities the UI links to), 001 (auth/sessions, the DB, the world)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Communication) — **private messaging** + **in-game chat**, the player-to-player communication layer.

## Goal

Give players two complementary channels of communication, both **server-authoritative** (P4) and built on
the existing stateless/DB-as-truth architecture (P5):

- **Private messaging (in-game mail).** Persistent player-to-player mail: compose, an inbox + sent box,
  read/unread, and per-side delete. A durable record, read at leisure.
- **Real-time chat.** Live channels — a **global** channel and a per-**alliance** channel — where messages
  appear to connected players within a second. Persisted (so there is history + an audit trail, P2/P6) and
  delivered live.

Both reuse the fair-play guards (022): a **sanctioned or frozen** sender cannot send, and sends are
**rate-limited** (anti-spam) — no new enforcement path.

## Concepts

- **Message (mail).** A row with a sender, a recipient, a subject, a body, a `created_at`, a `read_at`
  (null until the recipient opens it), and **per-side soft-delete** (each party removes it from *their*
  view without affecting the other's). The inbox is the recipient's non-deleted messages, newest first;
  the sent box is the sender's. Server-validated: a real, non-abandoned recipient; no self-send.

- **Chat channel.** A named stream a player may read/post to. **`global`** is open to every player;
  **`alliance:<id>`** is restricted to that alliance's members (015). Channel access is a pure rule of the
  player's alliance membership (P4 — enforced server-side, never trusted from the client).

- **Chat message.** A persisted row (channel, sender, body, `created_at`). Persisting first makes chat
  **reproducible** (P2/P6) and gives **history** (a new joiner backfills the recent messages) and a
  moderation trail.

- **Live delivery (SSE + `LISTEN/NOTIFY`, P5).** A send is an ordinary `POST` that **persists** the message
  then `pg_notify`s. Each web instance runs **one** Postgres listener that fans new messages out to its
  locally-connected subscribers; clients receive via a one-way **Server-Sent Events** stream (a plain `GET`).
  This keeps the web tier stateless and correct across **multiple instances** (the DB is the bus — no Redis,
  no sticky sessions), and the existing auth/rate-limit/sanction middleware covers the send `POST` for free.

- **Reuse of the fair-play guards (022).** Sending mail or chat is a mutating `POST`, so the round-freeze +
  per-account sanction guard and the rate limiter already apply — a banned/suspended player cannot send,
  and floods are throttled. Reads (inbox, the SSE stream) are always available.

## Acceptance criteria

> Sending, reading, deletion, and channel access are **server-authoritative** (P4) and reproducible from
> persisted state (P2/P6). Live delivery is best-effort on top of the durable record.

- **AC1 — Send mail.** A player sends a message (subject + body) to **another existing, non-abandoned**
  player; it is persisted. A self-send and a send to an unknown/abandoned recipient are rejected
  server-side.

- **AC2 — Inbox & sent.** The recipient sees received messages (excluding ones they deleted), newest first,
  each flagged read/unread; the sender sees their sent messages (excluding ones they deleted).

- **AC3 — Read & unread count.** Opening a received message marks it read (once); the unread count reflects
  only the recipient's unread, non-deleted messages.

- **AC4 — Per-side delete.** Either party can delete a message from **their own** view; it remains in the
  other party's view. A message is only readable/deletable by its sender or recipient (server-enforced).

- **AC5 — Chat channel access.** A player may read/post the **global** channel; an **alliance** channel only
  if they are a member of that alliance. A non-member's read or post is rejected server-side.

- **AC6 — Chat send & persist & deliver.** Posting to a channel the player may access persists the message
  and delivers it **live** to connected subscribers of that channel (typically within a second); a new
  subscriber **backfills** the recent history on connect.

- **AC7 — Fair-play enforcement.** A **sanctioned** (banned/suspended) or **round-frozen** player cannot
  send mail or chat (the 022/021 guards), and sends are **rate-limited**. Reads/streams stay available.

- **AC8 — Reproducibility, scale & roles.** Mail + chat are persisted, so state is reproducible (P2/P6).
  Live delivery uses `LISTEN/NOTIFY` so it is correct across **multiple web instances** (P5). A **Visitor**
  cannot send or read; only a logged-in **Player** can (per roles.md).

- **AC9 — Interface.** Mail pages (inbox / sent / compose / view / delete) with an **unread badge**; a chat
  page per accessible channel that shows history and updates live; a compose/“message this player” entry
  from a player's profile/stats page. No client action bypasses access or a limit (P4).

## Roles & permissions

Per [roles.md](../../roles.md). Communication is **Player** play; channel access derives from alliance
membership (015). Moderation of content reuses 022 (reports/sanctions).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — | All messaging/chat (must be a logged-in player). |
| **Player** | Send mail to other players; read own inbox/sent; delete own copy; read/post the global channel + their alliance channel. | Read others' mailboxes; post/read an alliance channel they're not in; send while sanctioned/frozen; exceed the rate limit. |
| **Moderator** | (022) review reported content + sanction senders. | — |
| **Administrator** | (operator) config. | — |

## Out of scope

- **Alliance forum** (threaded persistent boards) — a separate feature; this slice is mail + live chat.
- **Presence / typing indicators / online lists** — a later enhancement; this slice delivers send/receive +
  history.
- **Block lists** (refusing mail from a specific player), read receipts beyond the recipient's own
  read/unread, attachments, and message search — future work.
- **WebSockets** — SSE + `LISTEN/NOTIFY` is the chosen realtime transport (simpler, P5-correct, testable);
  a WS upgrade can replace it later if bidirectional/binary needs arise.
