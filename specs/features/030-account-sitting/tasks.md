# Feature 030 — Account sitting — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. The takeover is an effective-player resolution in the auth layer; gameplay handlers are unchanged.

## Domain

- [x] **T1 — Sitter cap rule (`domain/sitter.rs`; P3).** `MAX_SITTERS` + `can_grant_sitter(owner, target,
  current_count)` (≠ self, under cap). **Unit tests:** self rejected, cap boundary (AC1).

## Persistence & ports

- [x] **T2 — Sitter tables + repository (migration `0041`).** `account_sitters` + `sitter_actions`.
  `AccountRepository`: `grant_sitter`/`revoke_sitter`/`is_sitter`/`count_sitters`/`sitters_of`/
  `sitting_for`/`log_sitter_action`/`sitter_actions`; `SitterActionView`. **DB tests:** grant/revoke/
  is_sitter/count round-trip; action log ordered read (AC1, AC5, AC8).

## Use-cases

- [x] **T3 — Sitting use-cases (`application/src/sitting.rs`).** `grant_sitter`/`revoke_sitter`/
  `list_sitters`/`list_sitting_for`/`sitter_log`/`authorize_sit`/`record_sitter_action`; `SittingError`.
  `authorize_sit` = `is_sitter` ∧ owner not `account_blocked`. **Tests (fakes):** grant rejects self/
  over-cap/unknown; `authorize_sit` false for non-sitter + blocked owner; revoke de-authorises (AC1, AC2, AC6).

## Web — identity + guards

- [x] **T4 — Effective-player identity + guards (`auth.rs`, `lib.rs`).** `SIT_COOKIE`; `effective_identity`;
  `AuthUser` → effective; new `RealUser` → human; `action_guard` rejects a blocked real **or** effective
  player; `sitting_guard` (refuse restricted set, else audit) ; `presence_touch` touches the effective
  player. **Tests:** covered by T5 integration.

## Web — pages

- [x] **T5 — Sitting page + controls + banner.** `GET /sitting` (RealUser), `POST /sitting/{grant,revoke,
  start,stop}`, `GET /sitting/status`; a `base.html` banner script + nav link. **Integration tests:**
  authorised sitter sees the owner's village while sitting; stop reverts; non-authorised cannot start;
  revoke mid-sit reverts next request; settings/profile/grant refused while sitting; a normal action runs as
  the owner + is audited; the owner sees the log; banned owner not operable; banned sitter can't sit
  (AC2–AC7).

## Acceptance

- [ ] **T6 — Docs + review.** rustdoc on new public items; `docs/architecture/0032-account-sitting.md`;
  `docs/manual/` sitting note; `CLAUDE.md` active slice → 030. Full gates + P11; `eperica-reviewer` until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
