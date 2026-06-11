# Feature 008 — Marketplace & trade — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Trade domain.** `BuildingKind::Marketplace`; `trade.rs`: `TradeKind`,
  `MerchantProfile`/`MerchantRules` (`profile`, `merchants_total`), `merchants_required` (ceil over
  capacity), bundle `total`/empty helpers. Unit tests: round-up + capacity scaling; per-level total
  (**AC3**).

## Balance & persistence

- [x] **T2 — Balance.** `[buildings.marketplace]` in `construction.toml` (prereq Main Building 1);
  `marketplace` population row in `economy.toml`; new `trade.toml` (`merchants.per_level`, per-tribe
  `capacity`/`speed`) + `merchant_rules()` loader; `parse_building` + every `BuildingKind` mapping
  (balance/repo/web) gains the `marketplace` arm.
- [x] **T3 — Migration + trade repository.** `0011_trade.sql` (`trade_movements` + due/home indexes);
  `TradeRepository` (optimistic-spend `start_trade`, `committed_merchants`, `claim_due_trades`,
  `deliver_and_schedule_return` (guarded credit + return leg, single tx), `complete_trade`,
  `active_trades`, orphan requeue). DB tests: send debits + commits merchants; deliver credits capped
  exactly once + schedules return; crash-resume; return frees merchants (**AC1**, **AC4**, **AC5**).

## Application

- [x] **T4 — Trade use-cases.** `order_trade` (validate Marketplace/bundle/merchants/target → travel
  → spend), `process_due_trades` (claim → capped credit + return, free merchants on return). Fake
  tests: send success + every rejection; merchant commit math (**AC1**, **AC2**, **AC3**).
- [x] **T5 — Scheduler.** Tick `process_due_trades`; startup orphan requeue for trades. DB test via
  the processor (**AC4**/**AC5** restart path).

## Web

- [x] **T6 — Marketplace page + village panel.** `GET /village/market` (capacity, free/total
  merchants, send form); `POST /village/market/send` (PRG); `/village` **Shipments in transit**
  (direction + contents + countdown); Marketplace link. Integration tests (**AC6**).

## Documentation & acceptance

- [x] **T7 — Technical docs.** rustdoc; `docs/architecture/0010-marketplace-trade.md`; `CLAUDE.md`
  active slice.
- [x] **T8 — End-user docs.** `docs/manual/` trade guide; link from index.
- [ ] **T9 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC6** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
