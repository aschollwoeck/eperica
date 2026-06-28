# Feature 103 — stabilize two flaky infrastructure tests

## Why

Two `repo.rs` tests failed CI intermittently (a manual re-run always passed), forcing a re-run on most merges:
- `occupied_oasis_bonus_stacks_into_village_read` — its strict "production rises" assertion summed only
  wood+clay+iron, but the seeded oases may both grant a **crop-only** bonus (a valid balance entry), which
  lifts crop alone → the sum was unchanged → assertion failed. A seed lottery, not load.
- `scheduler_throughput_drains_backlog` — wall-clock bounds (`<20s`, `>100 events/s`) that are dominated by
  contention when the whole `cargo test` suite shares one Postgres, not by the drain itself.

## Acceptance criteria

- **AC1 — Oasis test deterministic.** The strict-increase assertion includes **crop**, so any oasis bonus
  (including crop-only) lifts the total — passes for every seed.
- **AC2 — Scheduler test load-tolerant.** The timing bounds are loosened (`<120s`, `>20 events/s`) so CI
  contention can't trip them while still flagging an order-of-magnitude regression; the exactly-once +
  idempotent-drain correctness assertions are unchanged.

## Out of scope
- The perf tooling (023); any production code (tests only).
