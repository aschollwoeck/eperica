# Tasks — 118 Agent API core

**Status:** Draft. Ordered; each gated by `cargo fmt --all -- --check`, `clippy --all-targets -- -D warnings`,
`cargo test --workspace`, and the P11 budget. Branch `feature/118-agent-api-core`; commit per task.

- [x] **T1 — Keys: schema + ports.** Migration `0050_agent_keys.sql` (`users.is_ai`, `agent_keys`);
  ports `create_agent_key` / `find_agent_key` / `revoke_agent_key` / `create_ai_account` + Pg impls;
  key format `epk_<id>_<secret>` with SHA-256-at-rest verify (constant-time), unit-tested. (AC1)
- [x] **T2 — Bearer auth + `/api` skeleton.** `crates/web/src/api.rs`: `AgentContext` extractor
  (bearer → account → shared world-resolution core with `GameContext`, JSON failures), the `/api`
  router (nested in `lib.rs`), `GET /api/me`, the JSON error shape + axum-rejection mapper. 401/403
  never redirect. (AC1/AC2)
- [ ] **T3 — Agent rate budget.** `agent_limit_per_window` in `specs/balance/fairplay.toml` +
  `FairPlayRules`; `agent_rate_guard` on the `/api` router (all methods, subject = bound account,
  action `"agent"`), 429 JSON with `retry_after_secs`. (AC5)
- [ ] **T4 — State digest + map window.** `GET /api/w/{world}/state` composing `load_economy`,
  `active_builds`, `active_training`, `load_culture`, `incoming_against` per the plan;
  `GET /api/w/{world}/map` via the existing map read models, `r` clamped. Digest DTOs carry absolute-ms
  deadlines; incoming = village + arrival **only**. (AC3)
- [ ] **T5 — Economy actions.** `POST …/build` and `POST …/train` as `axum::Json` adapters onto
  `order_build` / `order_train`; use-case error enums → (status, `error` code, player-visible `reason`).
  Success returns the queue entry + completes-at. (AC4)
- [ ] **T6 — Admin bootstrap.** `POST /admin/agent { username, world, tribe }` (admin-gated):
  `is_ai` account + world join + key issued, plaintext shown once in the console. Denied to
  non-admins. (AC1, roles)
- [ ] **T7 — Integration tests.** The AC suite from plan §Test strategy, including the **AC6 opening
  loop** end-to-end over HTTP (read `/api/me` → digest → build → lane-denied → train → queues progress
  in later digests) and the frozen-world/blocked-account parity check (AC2). (AC1–AC6)
- [ ] **T8 — Technical docs.** Rustdoc on the new public items (`api.rs`, ports); `CLAUDE.md`: mention
  the `/api` surface + ADR 0036; no architecture note needed beyond the ADR (already written).
- [ ] **T9 — End-user docs.** Internal/operator-facing slice — record as internal; add a short
  `docs/agent-api.md` (endpoints, auth, error shape, rate budget) as the contract reference for 119+
  and the runner. No player-manual change.
- [ ] **T10 — Review & accept.** Full gates green; `eperica-reviewer` on the slice diff → fix findings
  until **APPROVE**; PR #136 updated; spec/plan/tasks statuses flipped to Built.

## Done when

Per the [definition-of-done checklist](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
all ACs pass with tests at the right layers, every task above is checked, gates green, reviewer verdict
APPROVE, and the slice is merged to `main` once Verified.
