# Fair play & anti-cheat tooling — the enforcement surface

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 022

## Context
Fairness rests on the constitution (P4 server authority, P2/P6 reproducibility); this slice adds the
**policy/enforcement surface** on top (GDD §12.5, built progressively — P10): a **Moderator** role, player
**reporting → review → sanction**, server-side **rate limiting**, and reproducible **detection signals**.

## Design
- **Pure fair-play model (`domain/fairplay.rs`, P3).** `SanctionKind` (Warn/Suspend/Ban), `ReportReason`,
  and the single block predicate `account_blocked(banned_at, suspended_until, now)` — computed on read (P1),
  so a suspension simply expires with no sweep. Detection predicates (`shared_ip_flagged`,
  `inhuman_action_rate`) turn a reproducible count into an advisory flag against a config threshold (P7).
- **Sanction enforcement reuses two existing chokepoints (no new path).** `authenticate` rejects a blocked
  account with `LoginError::Sanctioned` (mirroring the 019 abandoned block). The web `action_guard` (the 021
  round-freeze middleware, now unified) reads the session player from the encrypted cookie and rejects a
  blocked player's mutating `POST`s. Reads + authentication always pass.
- **Report → review → sanction is moderator-gated CRUD.** `file_report` rejects self-reports and collapses a
  duplicate **open** report via a partial-unique index. `review_queue`/`resolve_report`/`sanction_account`
  gate on the `is_moderator` capability (read from `UserRecord`). `resolve_report` marks the report resolved
  (guarded on `status='open'`, idempotent) and applies the optional sanction to the subject **in one
  transaction** (AC4).
- **Rate limiting is DB-backed (P5).** A `rate_limit_guard` middleware counts each mutating `POST` in a
  fixed window via `bump_rate` (an atomic upsert) and returns **429** over the configured limit — `/login`
  + `/register` keyed by IP (brute-force / signup-spam), other actions by the session player. Counters live
  in the DB so the web tier stays stateless + horizontally scalable. It **fails open** on a backend glitch.
- **Detection signals are reproducible + advisory.** `ip_association_count` counts accounts sharing a
  `registration_ip` (captured at register from `X-Forwarded-For` → the `ConnectInfo` peer);
  `peak_action_count` reads the max per-window tally from `rate_limits`. `account_signals` (moderator-gated)
  combines them with the rules into flags. Signals never auto-sanction — a human always decides (P10).
- **Moderator bootstrap.** The operator lists usernames in `MODERATORS`; at startup they are granted
  `is_moderator` idempotently. A full Administrator console is later work.

## Persistence (migration 0034)
- `users` += `is_moderator`, `suspended_until`, `banned_at`, `registration_ip` (read-folded into
  `UserRecord`).
- `reports (id, world_id, reporter_id, subject_id, reason, note, status, created_at, resolved_by,
  resolved_at, resolution)` — a partial-unique `(reporter_id, subject_id) WHERE status='open'` collapses
  duplicates; a partial index on `created_at WHERE status='open'` serves the oldest-first queue.
- `rate_limits (subject, action, window_start, count)` — fixed-window counters; the tallies double as the
  inhuman-action-rate signal's input.

## Balance (P7)
- `fairplay.toml` — the rate window/limits, the suspension default, and the two detection thresholds.

## Consequences
- Sanctions ride the existing login + action chokepoints, so enforcement is server-authoritative and there
  is no parallel "is this allowed" path to keep in sync.
- Detection reuses already-persisted data (`registration_ip`, `rate_limits`) — no heavy per-action event
  log — and stays advisory, so a false positive is harmless.
- Rate limiting adds one indexed upsert per mutating action; it is DB-backed to honour P5 rather than a
  process-local bucket.
