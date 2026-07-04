# Tasks — 125 spectator mode

**Status:** Draft. Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [x] **T1 — Role, keys, migration.** `0052_spectator.sql` (is_spectator + spectator_keys);
  `spk_` token support; ports + repo (set/find/insert/revoke); admin console: role toggle +
  key mint/revoke; roles.md Spectator row (with the fog caveat). Tests: toggle round-trip,
  mint/verify/revoke, role-revoke dead-ends keys. (AC1, AC2)
- [ ] **T2 — Read aggregation.** World-scoped capped queries (movements, build orders, training,
  recent reports, player index) + `application/spectate.rs` (feed / players / village detail
  reusing the owner read-model). Repo tests: caps, ordering, world isolation. (AC3–AC5 backend)
- [ ] **T3 — Dashboard.** `/spectate` picker, feed page (four sections + countdowns +
  auto-refresh), players index, village drill-down; `require_spectator` 403 guard. Integration:
  AC1 web, AC3/AC4 web (incl. defender-view contrast), AC7. (AC1, AC3, AC4, AC6, AC7)
- [ ] **T4 — Spectator API.** `SpectatorAuth` extractor (role re-check, budget class), the four
  GET endpoints, JSON error contract; rate-guard namespacing. Integration: AC2 cross-key
  refusal, AC3/AC4 JSON, AC6 (POST ⇒ 404/405, no activity side effects), AC8 (429). (AC2–AC8)
- [ ] **T5 — Docs.** manual: `spectating.md` (player-facing: what a spectator is, how to ask for
  access) + index; operations/administration.md: granting the role, minting keys, the fog
  caveat; agent-api.md cross-note or `spectator-api.md` contract stub.
- [ ] **T6 — Review & accept.** Gates green; reviewer APPROVE (in-loop if agents unavailable);
  statuses flipped; PR merged when Verified.

## Done when

All ACs pass with tests, gates green, review APPROVE, docs updated, merged once Verified.
