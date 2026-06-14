# Feature 022 — Fair play & anti-cheat tooling — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Reuses 019 (login block + activity),
021 (freeze guard), 016 (account surface). New surface: Moderator role, report→review→sanction, rate
limiting, detection signals.

## Domain & balance

- [ ] **T1 — Fair-play rules + sanction/detection model (`domain/fairplay.rs`; P3/P7).** `SanctionKind`,
  `ReportReason` (+ string round-trips), `account_blocked(banned_at, suspended_until, now)`,
  `shared_ip_flagged`, `inhuman_action_rate`, `FairPlayRules`. `fairplay.toml` + `fair_play_rules()` loader.
  **Unit tests:** block transitions (banned/suspended/expired/none); detection at the threshold; round-trips;
  the rules load (AC5, AC7, AC8).

## Persistence & enforcement

- [ ] **T2 — Schema + account sanction/role state (migration `0034`).** `users` += `is_moderator`,
  `suspended_until`, `banned_at`, `registration_ip`; `reports` + `rate_limits` tables. `UserRecord` +
  `find_user_by_*` carry the sanction fields + `is_moderator` (read-folded). **DB test:** the new fields
  round-trip; a sanctioned user reads back blocked (AC1, AC5, AC8).

- [ ] **T3 — Sanction enforcement (auth + the action guard).** `authenticate` returns
  `LoginError::Sanctioned` when `account_blocked` (after the abandoned check); extend the web freeze-guard so
  a sanctioned logged-in player's mutating `POST`s are rejected. **Tests:** a banned/suspended account
  cannot log in; an expired suspension can; (web) a sanctioned player's action is rejected, reads pass (AC5).

## Reporting, review & sanction

- [ ] **T4 — `ModerationRepository` + report/review/resolve use-cases (`application/fairplay.rs`).**
  `file_report` (reject self-report; collapse duplicate open), `review_queue` (moderator-gated, oldest
  first), `resolve_report` (moderator-gated; resolve + optional sanction in one tx, idempotent),
  `set_moderator`/`is_moderator`, `apply_sanction`. `ModerationError`. **DB tests:** report persists; self
  + duplicate rejected; non-moderator denied; resolve applies a sanction once (AC1–AC5).

## Rate limiting

- [ ] **T5 — DB-backed rate limiting (`bump_rate` + `check_rate_limit` + web middleware).** Fixed-window
  upsert/read; `check_rate_limit` returns `Err(RateLimited)` over the config limit; a web middleware returns
  **429** for over-limit mutating `POST`s (player-keyed) and login attempts (IP-keyed). **Tests:** within
  limit passes, over limit is `RateLimited` (DB); (web) the Nth+1 request gets 429 (AC6, AC8).

## Detection signals

- [ ] **T6 — Detection signals (`ip_association_count`, `peak_action_count`, `account_signals`).**
  Moderator-gated aggregation: shared registration-IP count + the inhuman-action-rate flag (from the
  `rate_limits` tallies), via the rules. **DB tests:** association counts accounts sharing an IP; the
  inhuman flag trips at the threshold; deterministic (AC7, AC8).

## Interface

- [ ] **T7 — Web: moderator pages + player report + bootstrap + IP capture.** `/mod` review queue;
  `/mod/account/{id}` inspect (sanctions + signals + resolve/sanction forms), moderator-gated; a **report**
  action on the player-stats page; `register_submit` captures `registration_ip` (`X-Forwarded-For` →
  `ConnectInfo`); `MODERATORS` env bootstrap at startup; `AppState` carries the rules; sanctioned-login + 429
  surfaced. **Integration tests:** a player reports → it appears in the moderator queue; a non-moderator is
  denied `/mod`; a sanction blocks the subject's action (AC1–AC3, AC9).

## Docs & acceptance

- [ ] **T8 — Technical/end-user docs + review.** rustdoc on new public items;
  `docs/architecture/0024-fair-play-and-anti-cheat.md`; `docs/manual/` fair-play & moderation guide;
  `CLAUDE.md` active slice → 022. Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (022 ✅).
