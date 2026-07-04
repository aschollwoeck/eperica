# Tasks — 122 the LLM strategist

**Status:** Draft. Ordered; each gated by `cargo fmt --all -- --check`, `clippy --all-targets -- -D warnings`,
`cargo test --workspace`. Branch `feature/122-llm-strategist`; commit per task.

- [x] **T1 — Strategy overlay in the pure doctrine.** `strategy.rs` (Strategy/Focus, strict
  `parse_reply`, bounded `build_prompt`); `plan_tick(…, &Strategy, …)` with the five bias rules;
  `Strategy::default()` proven a no-op. Unit tests per AC1/AC2/AC3/AC4. (AC1–AC4)
- [x] **T2 — Backend seam.** `StrategistBackend` trait; `AnthropicBackend` (Messages API via
  reqwest, env model/key); `ScriptedBackend`. No test touches the network. (AC3 plumbing)
- [x] **T3 — Runner integration.** Per-bot strategist cadence (4h ± jitter, in-window) + fleet
  `LlmBudget` (rolling hour, pure + tested); apply-or-reject; the optional DM (one per cycle);
  `--no-llm` / key detection / `EPB_LLM_MODEL` / `--llm-budget`; dry-run coverage. (AC1/AC5/AC6)
- [x] **T4 — E2E with ScriptedBackend.** Military-strategy-vs-baseline intents; invalid reply →
  baseline; scripted DM lands once in the recipient's list. (AC6/AC7)
- [x] **T5 — Docs.** README strategist section (env vars, budget, cost note); agent-api.md
  cross-note; ADR 0036 slice table → program complete. End-user docs: internal.
- [ ] **T6 — Review & accept.** Gates green; `eperica-reviewer` → APPROVE (incl. the AC8 diff-stat
  check); statuses flipped; PR ready to merge.

## Done when

Per the [definition-of-done checklist](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
all ACs pass with tests, every task checked, gates green, reviewer APPROVE, merged once Verified.
