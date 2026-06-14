# Feature 026 — Notifications & alerts — Plan

**Spec:** ./spec.md · **Status:** Verified

A communication slice: a persisted, per-player notification feed for the attention-critical trio (incoming
attack, battle report, new message), with a nav bell + read/unread, delivered live by reusing the 024
SSE + `LISTEN/NOTIFY` bus. Server-authoritative (P4), persist-then-notify (P5), reproducible (P1/P2).

## Domain (pure, P3) — `crates/domain/src/notification.rs`

- `enum NotificationKind { IncomingAttack, BattleReport, NewMessage }` with `as_str`/`parse` (stable codec
  for the DB), and a pure presentation helper (a short label per kind). No I/O, no timestamps invented.
- The *decision* of who-gets-what is trivial and lives at the call sites; the domain owns only the kind
  vocabulary + labelling so the rule of "what a kind means" is in the pure crate.

## Persistence (migration `0037`)

- `notifications (id uuid pk, world_id uuid, player_id uuid, kind text, ref_kind text null, ref_id text null,
  body text not null default '', created_at timestamptz, read_at timestamptz null)`.
  - `ref_kind`/`ref_id` point the UI at the target (e.g. `report:<id>`, `dm:<other-uuid>`, `village:(x|y)`).
  - Indexes: `(player_id, created_at desc)` for the feed; partial `(player_id) where read_at is null` for the
    unread count. `world_id` for scoping.

## Application (ports + use-cases)

- New port `NotificationRepository` (default no-op methods so non-relevant fakes are untouched):
  - `record(notes: &[NewNotification]) -> ()` — bulk insert + `pg_notify('notifications', …)` per recipient
    in one statement (persist-then-notify). `NewNotification { player, kind, ref_kind, ref_id, body }`.
  - `list(player, limit) -> Vec<NotificationView>` (most-recent first).
  - `unread_count(player) -> i64`.
  - `mark_read(player, now) -> ()` (all of the player's unread → read; owner-scoped by the `player` arg).
- `crates/application/src/notification.rs`:
  - Generation helpers used by the hook sites: `notify_incoming_attack(notifs, defender, target, arrive)`,
    `notify_battle_report(notifs, recipients, report_id)`, `notify_new_message(notifs, recipient, other)`.
    Each builds `NewNotification`s and calls `record` (no-op-safe). Skips self-notification.
  - Read use-cases: `list_notifications`, `notification_unread`, `mark_notifications_read`.
  - `NotificationError` (Backend).

## Hook points (where notifications are generated)

- **Incoming attack** — in `order_attack` (`application/combat.rs`), after the attack movement is persisted,
  if the target owner ≠ attacker, record an `IncomingAttack` for the defender (ref = target village, body =
  arrival time / coords). Best-effort: a notify failure must not fail the attack (log, continue).
- **Battle report** — in the repo `apply_battle` (`infrastructure`), in the **same transaction** as the
  report + defender rows, insert notifications for the attacker + each distinct defender (so they can never
  be orphaned), then `pg_notify` each. (Done in SQL alongside the existing inserts.)
- **New message** — in the repo `send_dm`, alongside the existing DM insert + comms notify, insert a
  `NewMessage` notification for the recipient + `pg_notify('notifications', …)`.

## Live delivery (reuse 024) — `crates/infrastructure/src/comms_live.rs` (or a sibling)

- `NotificationHub` (a tokio broadcast of `LiveNotification { key: notif:<uuid>, kind, body }`), and
  `run_notification_listener(pool, hub)` — a `PgListener` on the `notifications` channel that republishes to
  the hub. Spawned in `main.rs` next to `run_chat_listener`.
- Web: `GET /notifications/stream` — SSE for the **logged-in player only**, subscribed on `notif:<player>`;
  emits a tiny event the client uses to bump the bell. (Per-player key ⇒ no cross-player leak, P4.)

## Web (`crates/web`)

- `GET /notifications` — the feed (most-recent first, bounded); rendering marks them read (calls
  `mark_notifications_read`), and each row links to its `ref` (report/conversation/map).
- `GET /notifications/unread` — plain-text count for the nav **bell** badge (mirrors `/messages/unread`).
- `POST /notifications/read` — explicit mark-all-read (owner-scoped).
- `base.html`: a bell next to the Messages link, polling `/notifications/unread` (reuse the existing badge
  JS pattern) + an EventSource on `/notifications/stream` to refresh live.
- Routing/middleware: `/notifications/unread` + `/notifications/stream` join the `presence_touch` and
  `rate_limit` **exclusion** lists (background pollers, like the 024 endpoints). `POST /notifications/read`
  is a normal guarded mutating action.

## Reuse / decisions

- **Persist-then-notify, DB as bus** — identical to 024; correct across multiple stateless web instances
  (P5), durable (a dropped nudge loses nothing).
- **Per-player routing key (`notif:<uuid>`)** — a notification stream is private by construction; no
  filtering of someone else's data client-side (contrast the 024 DM pair-key care — here each player has a
  single private key).
- **Notifications created in the triggering transaction where possible** (battle report, DM) — never
  orphaned; the incoming-attack hook is in the use-case right after the movement commit (best-effort, must
  not break the attack).
- **Bell mirrors the message badge** — same poll + SSE pattern, minimal new UX surface.

## Risks / testing

- **Domain tests:** `NotificationKind` codec round-trip; label per kind.
- **DB tests:** `record` inserts + `list`/`unread_count` reflect it; `mark_read` clears only the player's
  own; `apply_battle` writes report + a notification per participant; `send_dm` writes a recipient
  notification.
- **Application tests:** self-notification skipped (attacking own village; DM to self already rejected);
  generation helpers no-op-safe.
- **Web tests:** an attacked player sees an unread bell + the feed entry; opening the feed clears the count;
  a player cannot read/stream another's notifications (the stream + reads key off the session player);
  the new-message notification appears for the recipient and the report notification for both parties.
- **Performance (P11):** feed read is a single bounded, index-backed query; unread count is a partial-index
  count; live path is a broadcast receiver. No N+1.
