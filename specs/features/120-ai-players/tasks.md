# Tasks — 120 AI players

**Status:** Draft. Ordered; each gated by `cargo fmt --all -- --check`, `clippy --all-targets -- -D warnings`,
`cargo test --workspace`, and the P11 budget. Branch `feature/120-ai-players`; commit per task.

- [ ] **T1 — Visibility plumbing.** Migration `0051_ai_visibility.sql` (worlds column, default
  labeled); admin form select → `create_world` param → INSERT; `WorldMeta` cache →
  `context_for` → `GameContext.ai_labeled` / `WorldScope.ai_labeled`. Infra test loads it. (AC7)
- [ ] **T2 — Sweep carve-out.** The one-predicate exclusion in the 019 victim-select; repo tests:
  enabled bot survives however stale, revoked bot swept. (AC6)
- [ ] **T3 — Signals carve-out + mod badge.** `account_signals` short-circuit for `is_ai`;
  `ip_association_count` excludes bots; `ModAccountTemplate.is_ai` badge; reports against bots
  still file (disguise-preserving — plan Decision #2). Tests per plan. (AC5, AC4-mod)
- [ ] **T4 — Labeled tags.** `LeaderboardRow.is_ai` (five board queries) + board badge;
  `PlayerStatsTemplate.is_ai`; `VillageMarker.is_ai` + map-label "(NPC)"; all gated on
  `ai_labeled`; disguised world renders identically to humans. Integration tests both modes. (AC3/AC4)
- [ ] **T5 — Bulk seeding + fleet management.** `POST /admin/agents` (count ≤ 50, name pool +
  discriminator, tribe_mix) with the one-time JSON key manifest; fleet list (enabled state) +
  per-bot/fleet revoke; non-admin fail-closed. Integration tests per plan. (AC1/AC2)
- [ ] **T6 — Technical docs.** Rustdoc; ADR 0036 slice table note; docs/agent-api.md pointer to the
  manifest as the runner key-file format. End-user docs: operator-facing — recorded internal.
- [ ] **T7 — Review & accept.** Gates green; `eperica-reviewer` → APPROVE; statuses flipped; PR #138
  ready to merge.

## Done when

Per the [definition-of-done checklist](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
all ACs pass with tests, every task checked, gates green, reviewer APPROVE, merged once Verified.
