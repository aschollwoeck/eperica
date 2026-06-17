# Feature 060 — messages aggregated across all the account's worlds

**Status:** Verified
**Amends:** 024 (communication) under the 043–046 multi-world model + 056 routing. Sibling of 059
(notifications aggregation).

**Note:** The account-level Messages inbox/badge read only the **home** world's `CommsRepository` (the
repo is world-scoped), so a player active in a non-home world never sees that world's conversations, and
opening a conversation always lands in the home world. Aggregate the inbox **across all the account's
worlds** — one inbox on every page — with each conversation deep-linking into **its own** world
(`/w/{world}/messages/c/{key}`). This is the second half of the 059 decision; it also fixes the 059 NIT
(the `new_message` notification deep-link is now world-qualified). No domain change (P3).

## Problem

`messages` / `messages_unread` use `state.accounts` (the home-world repo), and the conversation
view/send/stream (`conversation` / `messages_send` / `messages_stream` / `messages_with`) are
account-level routes that operate through the home-world repo. So:

1. The inbox lists only home-world DM threads + the home world's global/alliance channels.
2. The nav badge counts only home-world unread.
3. Opening `/messages/c/{key}` reads/sends in the home world regardless of where the conversation lives.
4. `conversation_reads` has **no `world_id`** (PK `(player_id, conversation)`), so a read watermark for
   `global` / `dm:<peer>` / `alliance:<id>` is shared across worlds — reading global in world A would clear
   global's unread in world B, and per-world unread counts are wrong once an account plays >1 world.

## Goal

- **AC1 — Aggregated inbox.** The Messages page lists the account's conversations from **all** worlds it
  plays — DM threads + each world's global + alliance channels — newest-activity first, each row carrying
  **its** world so its link is `/w/{that-world}/messages/c/{key}`.
- **AC2 — Aggregated unread badge.** The nav Messages badge sums unread across **all** the account's worlds.
- **AC3 — World-scoped conversation.** Opening a conversation operates in **its** world: it reads that
  world's DM/channel history, sends into that world, streams that world's live key, and marks **that
  world's** watermark read. A conversation route is `/w/{world}/messages/...`.
- **AC4 — Per-world read isolation.** A read watermark is per `(account, world, conversation)`: reading a
  conversation in one world never clears its unread in another. (`conversation_reads` gains `world_id`.)
- **AC5 — Server-authoritative (P4).** Aggregation keys on the **account** (`players.user_id` / the DM
  `users` id the handler holds); a player only ever sees/sends their own conversations in worlds they have
  joined. A conversation route to an unjoined/unknown world bounces to the lobby (the existing `GameContext`
  guard).

## Design

The account's `user_id` equals its home player id (the reuse-UUID invariant) and is the `PlayerId` the
account-level handlers hold (`AuthUser`). DMs are keyed by `users` id (world-agnostic identity, world-tagged
storage); channels (`global`, `alliance:<id>`) are per-world. `worlds_of_user(account)` lists the account's
joined worlds; `WorldRegistry::context_for(world)` yields that world's `CommsRepository`/`AllianceRepository`.

- **Migration** `00NN_conversation_reads_world.sql`: `ALTER TABLE conversation_reads ADD COLUMN world_id
  uuid REFERENCES worlds(id)`, backfill existing rows to the home world (oldest `worlds` row), set
  `NOT NULL`, repoint the PK to `(player_id, world_id, conversation)`. (Touch `infrastructure/src/db.rs` so
  `sqlx::migrate!` re-embeds.)
- **`infrastructure/src/repo.rs`** (per-world `CommsRepository`, already world-scoped via `self.world_id`):
  the four `conversation_reads` queries (`mark_read` upsert + the three unread `last_read_at` sub-selects)
  add `world_id = self.world_id` so the watermark is per-world (AC4). No new account methods here — the
  aggregation reuses the per-world methods via iteration.
- **`application/src/comms.rs`**: `conversation_list` / `unread_badge` (per-world) + `open_chat` / `send_chat`
  split the conflated `viewer` into `account` (the `users` id — DMs, channels, the read key) and `player`
  (the per-world player id — alliance membership, repointed to `players(id)` in 0045). They coincide in the
  home world and differ elsewhere. The cross-world aggregation **is orchestrated in the web layer** (the
  `WorldRegistry` lives there, so application stays free of it, P3): the handler iterates the account's joined
  worlds and reuses these per-world use-cases.
- **`web/src/lib.rs`**: move `conversation` / `messages_send` / `messages_stream` / `messages_with` under
  `/w/{world}` (resolved by `GameContext` → the per-world repo + `account`/`player`); keep `/messages` (inbox)
  and `/messages/unread` (badge) account-level (one inbox/badge on every page). `presence_touch`'s
  background-poller allow-list matches the now world-scoped `…/messages/stream`.
- **`web/src/handlers.rs`**: `messages` / `messages_unread` iterate `worlds_of_user(account)` →
  `WorldRegistry::context_for(world)` → the per-world `conversation_list` / `unread_badge`, tagging each row
  with its world (inbox) / summing (badge). The moved handlers take `GameContext` and use its per-world repo +
  `account`/`player` + `world_id`. `notification_href`'s `dm` arm becomes `/w/{world}/messages/c/dm:{other}`
  (closes the 059 NIT — the notification already carries its world).
- **Templates**: `ConversationRow` (the web inbox-row view) gains `world` (link `/w/{world}/messages/c/{key}`);
  `ConversationTemplate` gains `world` (send → `/w/{world}/messages/send`, stream →
  `/w/{world}/messages/stream/{key}`, back → `/messages`). `base.html` Messages link stays `/messages`.

## Out of scope

- A **cross-world compose** surface (starting a new DM picks a world first) — sending stays inside an opened,
  world-scoped conversation. New-DM entry continues via a player's profile/`messages/with` in a world.
- The DM/chat **SSE live streams** stay per-world (subscribed under `/w/{world}/messages/stream/{key}`); the
  account badge picks up other worlds via its existing poll (now aggregated).
