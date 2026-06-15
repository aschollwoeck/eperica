# Feature 029 — Settings & preferences — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. Completes the 026-deferred per-kind notification preferences.

## Domain

- [x] **T1 — `NotificationKind::ALL` (`domain/notification.rs`; P3).** The full kind set for iteration.
  **Unit test:** `ALL` round-trips every variant via `as_str`/`parse` (AC1).

## Persistence & ports

- [x] **T2 — Mutes table + gating (migration `0040`).** `notification_mutes` (row = muted).
  `NotificationRepository`: `muted_kinds`, `set_mute`; gate `record` + the inline `apply_battle` /
  `send_dm` notification inserts on `NOT EXISTS (notification_mutes)` and fire the live nudge only for
  inserted rows. **DB tests:** `set_mute`/`muted_kinds` round-trip + idempotent; a muted recipient gets no
  row from each generation path; a non-muting player still does (AC3, AC4).

## Use-cases

- [x] **T3 — Settings use-cases (`application/src/settings.rs`).** `notification_settings`,
  `set_notification_pref`; `SettingsError`. **Tests (fakes):** settings report enabled/disabled;
  `set_notification_pref(enabled=false)` mutes (AC1, AC2, AC4).

## Web

- [x] **T4 — Settings page + nav link.** `GET /settings` (checkbox per kind), `POST /settings/notifications`
  (owner-scoped); a Settings link in `base.html`. **Integration tests:** disabling a kind suppresses its
  notification (bell stays 0) + re-enabling restores it; Visitor redirected; one player's mute doesn't
  affect another (AC1–AC5).

## Acceptance

- [x] **T5 — Docs + review.** rustdoc on new public items; `docs/architecture/0031-settings.md`;
  `docs/manual/` settings note; `CLAUDE.md` active slice → 029. Full gates + P11; `eperica-reviewer` until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC6** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
