# Tasks — 119 Agent API actions complete

**Status:** Draft. Ordered; each gated by `cargo fmt --all -- --check`, `clippy --all-targets -- -D warnings`,
`cargo test --workspace`, and the P11 budget. Branch `feature/119-agent-api-actions`; commit per task.

- [x] **T1 — Military sends.** `attack`/`scout`/`reinforce`/`return` adapters (units-map bodies,
  plan Decisions #1–#3) + Combat/Scout/Movement error maps; success returns the movement via
  `active_movements`. Integration: success + empty-composition + not-all-scouts + protection +
  recall-by-host + foreign-group 404. (AC1/AC2)
- [x] **T2 — Trade & settle.** `trade`/`settle` adapters + Trade/Settle error maps; success returns
  shipment/settling arrival. Integration: success + no-marketplace + not-settler-group. (AC3)
- [x] **T3 — Research & smithy + digest research block.** `research`/`smithy` adapters + error maps;
  digest gains per-village `research` (researched/levels/active orders). Integration: success +
  already-researched + digest reflects. (AC4)
- [x] **T4 — Digest closure + report reads.** Digest `movements`/`reinforcements_here`/`_abroad` +
  `scout_reports` heads + `kind` on report heads; `GET report/{id}` + `GET scout-report/{id}`
  party-scoped. Integration: digest equality (M4 pattern) + non-party 404. (AC5/AC7)
- [x] **T5 — Messages.** `POST message` (username→account, `send_dm`), `GET messages`
  (`conversation_list`), `GET messages/{account}` (`open_dm`); Comms error map; account-id vs
  player-id called out in comments. Integration: exchange + self-send + unknown recipient. (AC6)
- [x] **T6 — AC8 loop test.** The two-agent raid→report→reinforce→recall script over pure JSON,
  driving `process_due_combat`/`process_due_movements` for determinism. (AC8, AC9 spot-checks)
- [x] **T7 — Docs.** docs/agent-api.md v0.2: all new endpoints/bodies/codes + digest additions;
  rustdoc on new public items.
- [ ] **T8 — Review & accept.** Gates green; `eperica-reviewer` → APPROVE; statuses flipped; PR #137
  ready to merge.

## Done when

Per the [definition-of-done checklist](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
all ACs pass with tests, every task checked, gates green, reviewer APPROVE, merged once Verified.
