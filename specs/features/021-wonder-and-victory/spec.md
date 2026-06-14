# Feature 021 — Wonder of the World & victory

**Status:** Verified
**Depends on:** 020 (artifacts/Natar release + Treasury-gated capture — the template plans reuse), 014 (conquest — how an alliance takes a Wonder site), 015 (alliances — the victor + the plan-holding group), 003 (construction — the level-by-level build), 001 (world clock + lifecycle state)
**Roadmap:** M7 · slice 021 · GDD §11.3 (steps 2–3) / §13.3 — the **launch capstone**: Wonder-of-the-World building plans appear, an alliance holds a plan and **conquers a Wonder site**, builds a **Wonder to level 100**, and the **first alliance to 100 wins the round** — which then **ends** (winner recorded, world frozen). After this slice a world has a beginning, an arc, and a winner.

## Goal

The end-game's conclusion. After artifacts (020), a later **Wonder-release date** opens the final phase:
**Wonder building plans** materialize in Natar villages (captured by force, like artifacts), and a set of
**Natar Wonder-construction sites** appear (conquerable, unlike artifact vaults). An alliance must
**conquer a Wonder site** and **hold a plan**, then **build the Wonder** level by level — escalating
cost — toward **100**. The **first alliance to complete a Wonder (level 100) wins the world**; the round
then **ends**: the winner is recorded and the world is **frozen** (no further play), with a victory
surface. This gives the game its **seasonal shape** (§13.3) — a finite, winnable round.

Everything is **server-authoritative** (P4) and **reproducible** from persisted state + the seeded world
(P2/P6); the client never releases, captures, builds, or wins.

## Concepts

- **Wonder release (world clock).** At a configured **Wonder-release date** (after the artifact date,
  GDD §13.2), a one-time release materializes: (a) **Wonder plans** in Natar **vaults** (capturable), and
  (b) **Wonder sites** — Natar villages flagged **conquerable** (the WW construction villages). Idempotent.

- **Wonder plans (capturable, plan-holding).** A plan is a capturable item held in a village, transferred
  by a **winning attack from a Treasury** — the **020 capture mechanic** (a plan is taken/stolen exactly
  like an artifact). An **alliance holds a plan** when any of its members' villages holds one. Holding a
  plan **gates Wonder construction** (you cannot build the Wonder without your alliance holding ≥ 1 plan).

- **Wonder sites (conquerable Natar villages).** Unlike artifact vaults (020, unownable), a **Wonder site**
  is a Natar village that **can be conquered** (the 020 unconquerable guard is lifted for sites). An
  alliance takes a site via the existing **conquest** path (014: administrators → loyalty 0 → ownership).
  The Wonder is built **in a Wonder-site village the alliance controls**.

- **Wonder construction (to 100).** The Wonder is a building raised **level by level** to **100** with a
  steeply **escalating cost/time** (config, P7), reusing the construction queue. Each order requires: the
  village is a **controlled Wonder site**, the owner's **alliance holds a plan**, and the Wonder is below
  100. Construction is server-validated (P4) and resolves via the scheduler like any build.

- **Victory (first to 100).** When a Wonder reaches **level 100**, its owner's **alliance wins**. A
  state-driven check (a scheduler tick) records the **winner alliance** + **won_at** on the world,
  **exactly once** (guarded by `won_at IS NULL`). Because the check runs every tick and the first tick to
  observe *any* complete Wonder records the winner, the **earliest completer wins** — a Wonder finished in
  a later tick can never overwrite it. The only residual ambiguity is two Wonders completing within the
  **same tick** with neither yet recorded; that is broken **deterministically by descending level then
  ascending alliance tag** (P6) — a stable, reproducible order, not the wall-clock completion instant
  (which is not separately persisted).

- **Round end / freeze (§13.3).** Once the world is **won**, the round is **over**: further game actions
  are **rejected** (the world is read-only / frozen) and a **victory** state is surfaced. Auto-archival and
  spawning a fresh world are **out of scope** (an ops/launch concern) — the world simply ends with a winner.

## User stories

- As an **alliance**, I want the **Wonder phase** to open so the round has a clear finish line.
- As an **alliance**, I want to **capture plans** and **conquer a Wonder site**, then **race to build the
  Wonder to 100** against rival alliances.
- As a **player**, I want to see **who is leading the Wonder race** and, when it's over, **who won**.
- As an **administrator**, I want the **Wonder-release date, plan/site counts, and the Wonder cost curve**
  to be **config** (P7).

## Acceptance criteria

> Release, capture, conquest, construction, and victory are **server-authoritative** (P4) and
> **reproducible** from persisted state + the seeded world (P2/P6). The schedule, counts, and cost curve
> are **config** (P7).

- **AC1 — Wonder release at the configured date.** Before the Wonder-release instant there are no Wonder
  plans or sites. At/after it, a one-time release materializes the configured **plans** (in Natar vaults)
  and **conquerable Wonder sites**, **exactly once** (re-running is a no-op).

- **AC2 — Plans are captured like artifacts.** A winning attack from a qualifying Treasury village against
  a village holding a Wonder plan transfers (captures/steals) the plan, in the battle transaction, exactly
  once — the same rule as artifact capture (020 AC4/AC5).

- **AC3 — Wonder sites are conquerable.** A Wonder-site Natar village **can be conquered** via the conquest
  path (014) — administrators drop its loyalty to zero and ownership transfers — unlike artifact-vault
  Natar villages, which remain unconquerable.

- **AC4 — Construction is gated.** A Wonder build order is accepted only when: the village is a Wonder site
  **controlled by the orderer**, the orderer's **alliance holds ≥ 1 plan**, and the Wonder is **below 100**.
  Otherwise it is rejected server-side. Accepted orders build the Wonder one level at a time at the
  config cost/time.

- **AC5 — Build to 100.** The Wonder advances level by level up to a maximum of **100**; an order at 100 is
  rejected. Construction resolves through the scheduler like any build (P1).

- **AC6 — Victory: first to 100 wins, once.** When a Wonder reaches level 100, the world records the
  **winner alliance** and **won_at**, **exactly once** — the first alliance to 100 (earliest completer via
  the per-tick guard; a same-tick tie broken deterministically by descending level then ascending tag, P6).
  A later completion does not overwrite the winner.

- **AC7 — Round freeze.** Once the world is won, mutating game actions are **rejected** (the world is
  frozen / read-only); the **victory** (winner + when) is surfaced. Reads still work.

- **AC8 — Reproducibility & config.** Plan/site placement (seeded Natar tiles, deterministic order),
  victory selection, and the freeze are deterministic from persisted state (P2/P6). The Wonder-release
  date, plan/site counts, and the Wonder cost/time curve are config.

- **AC9 — Interface.** A **Wonder-progress** surface shows the leading alliances' Wonder levels (the race);
  on victory a **winner banner** is shown. No client action releases, captures, builds, or declares
  victory (P4).

## Roles & permissions

Per [roles.md](../../roles.md). The Wonder race is **alliance** play; release + victory are **System**.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A — may see public Wonder progress / the winner. | Building/capturing/declaring victory. |
| **Player** | **Capture plans** + **conquer Wonder sites** (by force) and, with the **alliance right** to build, **order Wonder construction** in a controlled site while the alliance holds a plan; see the Wonder race + winner. | Building without a controlled site / held plan / build right; building past 100; declaring victory; acting after the world is won. |
| **Moderator** | N/A (considered) — no Wonder moderation surface this slice. | — |
| **Administrator** | Configure (P7) the Wonder-release date, plan/site counts, and the Wonder cost/time curve. | Granting wins or plans from the client. |
| **System** | *(system-initiated)* **Release** plans + sites at the date; **transfer** plans on a qualifying winning attack; **advance** Wonder levels at the scheduler; **declare victory** (first to 100, exactly once) and **freeze** the world. | — |

## Out of scope

- **Auto-archival + spawning a fresh world / multi-world rotation** (§13.3 tail) — the world ends with a
  recorded winner and is frozen; starting the next round is an ops/launch concern (Decided).
- **Per-level plan requirements / Great Warehouse–Granary plans** — a single **plan-held gate** enables
  Wonder construction (faithful-enough); finer plan economies are deferred.
- **New combat/conquest math** — plan capture rides the 020 capture path; site conquest rides the 014
  path (only the Natar unconquerable guard is lifted for sites).
- **A hard end-date fallback** (§13.3 "or a hard end date") — victory is by first-to-100 only this slice.

## Decisions

- **Plans reuse the 020 capture machinery.** A Wonder plan is a capturable item transferred by the same
  won-attack-from-a-Treasury path (a parallel transfer in the battle transaction); "alliance holds a plan"
  is derived from member villages' held plans.
- **Wonder sites are conquerable Natar villages.** A new `is_wonder_site` flag marks Natar villages that
  the 014 conquest path may take (the 020 `is_natar` unconquerable guard is lifted only for sites).
- **Wonder construction reuses the build queue with a dedicated rule.** A `WonderRules` 100-level cost/time
  curve (config) + a Wonder build path gated by site-control + plan-held + level < 100; resolves via the
  scheduler. (A dedicated path keeps normal buildings ≤ their small caps.)
- **Victory + freeze are world state.** `worlds` gains `wonder_release_at`, `won_at`, `winner_alliance_id`.
  A state-driven `process_due_wonder_victory` tick records the first-to-100 winner (idempotent via
  `won_at IS NULL`); a **freeze guard** rejects mutating use-cases once `won_at` is set.
- **Release is a one-time wall-clock due check** (mirrors 020), idempotent; placement on reserved Natar
  tiles in seeded ring order (P6).

## Open questions

- **Where the freeze is enforced.** Per-use-case guards vs a single check at the action boundary.
  **Proposed:** a cheap world-ended check at the authenticated **action handlers** (POST routes) + the
  scheduler ceasing to start new Wonder/build work; reads stay open. Centralizes the freeze without
  threading a flag through every domain use-case.
- **Plan count vs site count.** **Proposed:** a small configured number of each (e.g. a few plans, a few
  sites) so the race is contestable but bounded; tunable in balance.
- **Wonder cost curve shape.** **Proposed:** a steep geometric per-level cost/time (config), large enough
  that reaching 100 is a multi-alliance, end-of-round effort.
