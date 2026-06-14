# Notifications & alerts

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 026

## Context
Players need to know, promptly and durably, when something demands their attention: an **incoming attack**,
a new **battle report**, a new **direct message**. The signal must be **server-authoritative** (P4),
**persisted** (a feed they can revisit — P2/P6), and delivered **live** without a reload — on the existing
stateless web + DB-as-truth architecture (P5).

## Design
- **A notification is a recorded outcome, not a recomputed view.** It is written at the exact point its
  triggering event commits — never invented or recomputed on read (P1: discrete outcomes, recorded once).
  Each row is `(player_id, kind, ref_kind, ref_id, body, created_at, read_at)`, strictly private to its
  recipient.
- **Kind vocabulary in the pure crate.** `domain::NotificationKind` (IncomingAttack / BattleReport /
  NewMessage) owns the stable DB codec (`as_str`/`parse`) + a label (P3). *Who* gets *which* is decided at
  the commit sites; the domain owns only what a kind means.
- **Three generation hooks, each at a commit boundary.**
  - **Incoming attack** — `order_attack` (application) records an `IncomingAttack` for the defender (≠ the
    attacker) right after the attack movement commits. Best-effort: a notification failure is logged and
    never fails the attack.
  - **Battle report** — `apply_battle` (infrastructure) inserts one notification per distinct participant
    (attacker + each defender) **inside the report transaction**, so a report and its alerts commit together
    (never orphaned).
  - **New message** — `send_dm` (infrastructure) inserts a `NewMessage` for the recipient alongside the DM,
    in one statement.
- **Live delivery reuses the 024 bus.** Persist-then-notify: the same statement that inserts the row(s)
  emits `pg_notify('notifications', {key, kind})`. A second per-process `PgListener`
  (`run_notification_listener`) republishes to an in-process `NotificationHub` broadcast; the SSE endpoint
  `GET /notifications/stream` forwards only the events whose `key` equals the **logged-in player's** key.
- **Per-player routing key `notif:<uuid>`.** Each player has exactly one private key, derived by
  `application::notif_key` (matched by the SQL the hooks emit). A player can only ever compute — and thus
  subscribe to — their own, so the stream is private by construction (P4) — no client-side filtering of
  someone else's data (contrast 024's pair-canonical DM key, which had to defeat wiretapping).
- **Bell + feed.** A nav **bell** polls `/notifications/unread` (mirroring the 024 message badge) and an
  `EventSource` on the stream refreshes it promptly on a live nudge. `GET /notifications` renders the feed
  (most-recent first, bounded by `FEED_LIMIT`) and marks it read on view; rows deep-link via `ref_kind`/
  `ref_id` to the report (`/reports/{id}`), conversation (`/messages/c/dm:<uuid>`), or map (`/map?x&y`).
  The poll + stream are excluded from the 025 presence-touch (background traffic).

## Persistence (migration 0037)
- `notifications (id, world_id, player_id, kind, ref_kind, ref_id, body, created_at, read_at)`. Indexes:
  `(player_id, created_at DESC)` for the feed; a partial index `(player_id) WHERE read_at IS NULL` for the
  unread count. The `body` (a short pre-rendered detail) is denormalised so the feed read is a single
  table scan with no joins (P11).

## Reuse / decisions
- **DB as the bus** — identical to 024; correct across multiple stateless web instances (P5), durable (a
  dropped live nudge loses nothing — the row is there; the next poll/load shows it).
- **In-transaction generation where possible** (battle report, DM) — exactly-once with the triggering
  write; the incoming-attack hook is best-effort right after the movement commit so it can never fail an
  attack.
- **A second listener, not a shared channel** — keeps the notification envelope separate from chat lines;
  one extra long-lived connection per process, which is acceptable.
- **Bell only carries `kind`, never private payload** — the live event just says "something arrived"; the
  authoritative count + content come from the player's own authenticated reads.

## Consequences
- A durable, private, live notification surface with no new infrastructure dependency and no new
  enforcement path (`/notifications/read` is an ordinary guarded POST).
- **Out of scope (deferred):** completion alerts (build / training / movement-arrival / trade) are
  higher-volume per-tick events; scout-report, alliance, and diplomacy events; per-kind preferences/muting
  (the later Settings slice); grouping/threading and web-push. This slice covers the attention-critical
  trio.
