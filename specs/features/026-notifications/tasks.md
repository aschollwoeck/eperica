# Feature 026 — Notifications & alerts — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. Live delivery + persistence reuse the 024 `LISTEN/NOTIFY` + SSE architecture.

## Domain

- [x] **T1 — Notification kind vocabulary (`domain/notification.rs`; P3).** `NotificationKind`
  (IncomingAttack / BattleReport / NewMessage) with `as_str` / `parse` (stable DB codec) and a per-kind
  label. **Unit tests:** codec round-trips for every variant; unknown string → None; labels (AC1–AC3).

## Persistence & ports

- [x] **T2 — `notifications` table + repository (migration `0037`).** `NotificationRepository` (default
  no-ops): `record(&[NewNotification])` (bulk insert + per-recipient `pg_notify('notifications', …)` in one
  statement), `list(player, limit)`, `unread_count(player)`, `mark_read(player, now)`. `NewNotification` +
  `NotificationView` in ports. **DB tests:** record→list/unread reflect it; `mark_read` clears only the
  caller's own; list is newest-first + bounded (AC4, AC5).

## Use-cases + generation

- [x] **T3 — Use-cases + generation helpers (`application/src/notification.rs`).** `list_notifications`,
  `notification_unread`, `mark_notifications_read`; `notify_incoming_attack` / `notify_battle_report` /
  `notify_new_message` (build `NewNotification`s, skip self, call `record`; no-op-safe). `NotificationError`.
  **Tests:** self-notification skipped; helpers tolerate a no-op repo (AC1–AC3, AC7).

## Hooks (generation at the event commit points)

- [x] **T4 — Wire the three hooks.** `order_attack` records an `IncomingAttack` for the defender (≠ attacker)
  after the movement commits (best-effort, never fails the attack). `apply_battle` inserts a notification per
  participant **in the report transaction** + notifies. `send_dm` inserts a `NewMessage` for the recipient +
  notifies. **DB/app tests:** attacking another's village notifies the defender, not the attacker; attacking
  your own village notifies no one; a resolved battle notifies attacker + each defender; a DM notifies the
  recipient only (AC1–AC3).

## Live delivery + web

- [x] **T5 — Live bus + web surfaces.** `NotificationHub` + `run_notification_listener` (PgListener on
  `notifications`), spawned in `main.rs`. Routes: `GET /notifications` (feed; marks read; rows link to ref),
  `GET /notifications/unread` (bell count), `POST /notifications/read` (mark all), `GET /notifications/stream`
  (SSE, per-player `notif:<uuid>` key). `base.html` bell (poll + EventSource), badge JS. Exclude the
  poll/stream from `presence_touch` + rate-limit (background). **Integration tests:** an attacked player has
  an unread bell + feed entry; opening the feed clears the count; a player cannot stream/read another's
  notifications; recipient sees a new-message notification (AC4–AC7).

## Acceptance

- [ ] **T6 — Docs + review.** rustdoc on new public items; `docs/architecture/0028-notifications.md`;
  `docs/manual/` notifications note; `CLAUDE.md` active slice → 026. Full gates + P11; `eperica-reviewer`
  until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
