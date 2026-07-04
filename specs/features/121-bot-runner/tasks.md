# Tasks — 121 the bot runner

**Status:** Draft. Ordered; each gated by `cargo fmt --all -- --check`, `clippy --all-targets -- -D warnings`,
`cargo test --workspace`, and the P11 budget (client-side: the one-digest-per-tick + cap rules).
Branch `feature/121-bot-runner`; commit per task.

- [x] **T1 — Crate + client + DTOs.** `crates/bots` (lib+bin skeleton) in the workspace; defensive
  digest/map DTOs (fixture-parse test); `ApiClient` (me/state/map + all action calls) with the
  `ApiFailure` classification; manifest loading + startup key validation (AC1 shape). (AC1)
- [ ] **T2 — Persona + policy.** Inline-FNV personas (deterministic — AC4) + the full reflex
  doctrine from plan §doctrine as pure `plan_tick`; unit tests for every rule + negative gates. (AC2/AC4)
- [ ] **T3 — Executor.** Intents → API calls; pure `classify(status, body)` (Ok/RuleDenied/
  Backoff/RetireBot/Transient) unit-tested; per-tick behaviour per AC3 (no same-tick retries,
  429 honours retry_after_secs, 401 retires). (AC3)
- [ ] **T4 — Fleet loop + main.** Single scheduler + semaphore cap + jittered per-bot next_tick;
  activity windows; ctrl-c drain; `main` flags/env + tracing; `--dry-run` (log intents, no POSTs);
  map-window TTL cache. (AC4/AC5/AC7-ops)
- [ ] **T5 — E2E.** The forced-tick test against the spawned server (orders appear in the next
  digest), dry-run zero-writes, dead-key retirement. (AC1/AC6)
- [ ] **T6 — Docs.** `crates/bots/README.md` (run book: seed fleet → download manifest → run);
  CLAUDE.md commands section (`cargo run -p eperica-bots -- …`); docs/agent-api.md cross-link.
  End-user docs: operator-facing — recorded internal.
- [ ] **T7 — Review & accept.** Gates green; `eperica-reviewer` → APPROVE; statuses flipped; PR
  ready to merge.

## Done when

Per the [definition-of-done checklist](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
all ACs pass with tests, every task checked, gates green, reviewer APPROVE, merged once Verified.
