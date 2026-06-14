# Feature 020 — Artifacts & Natar villages — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

The end-game's first phase: at a configured date, **Natar NPC villages** materialize holding **artifacts**;
a **winning attack from a Treasury village** claims/steals an artifact; artifact **effects fold into the
sim reads** by scope. Key architectural choice: **Natar villages are ordinary `villages` rows owned by a
synthetic NPC user** (`users.is_npc`), so they reuse the **entire combat engine** (target resolution,
garrison, battle resolution, reports) — the only new combat step is an **artifact transfer** on a won
attack. Natar/NPC rows are excluded from boards/stat pages/abandonment/conquest by cheap filters.

## Constitution check

- **P1 (lazy/event-driven):** artifact effects are **not stored mutations** — they are aggregated from the
  holdings and applied **on read** (oasis-bonus pattern); losing an artifact removes the effect next read.
  Release is a one-time **state-driven due check** (now ≥ `artifact_release_at`, idempotent), not a tick.
- **P2/P6 (reproducible):** Natar placement + garrisons are **seeded** (world seed, deterministic ring
  order); the Fool artifact's effect is a **seeded** pick fixed at release; capture/transfer is a function
  of persisted state in the battle transaction. Same release ⇒ same villages/garrisons/artifacts.
- **P3 (pure domain):** `domain/artifact.rs` holds the artifact model, effect aggregation, and capture
  eligibility — pure, unit-tested. `BuildingKind::Treasury` is domain.
- **P4 (server authority):** release, capture, theft, and effect application are System/server only; no
  client path. Capture requires a server-validated **winning attack** from a **qualifying Treasury**.
- **P7 (config):** release date (world), artifact catalogue (types/scopes/magnitudes + Treasury level
  requirements), and Natar garrison strength are balance/config.
- **P11:** capture adds at most a couple of indexed reads on the (already heavy) battle-resolution path;
  effects are a small aggregation folded into reads already being computed.

## Domain (`domain/artifact.rs` + `building.rs`, pure)

- `enum ArtifactKind { Speed, Storage, Sustenance, Trainer, Architect, Eyes, Confuser, Fool }`.
- `enum ArtifactScope { Small, Large, Unique }`.
- `struct ArtifactId(String)`; `struct ArtifactDef { id, kind, scope, magnitude: f64 }`.
- `struct ArtifactEffects { troop_speed: f64, storage: f64, upkeep: f64, training: f64, durability: f64,
  scout_power: f64, scout_defense: f64 }` — multiplicative factors (1.0 = no effect).
- `fn effect_kind(def) -> ArtifactKind` resolving **Fool** to its seeded concrete kind
  (`fool_resolved(id_seed) -> ArtifactKind`, deterministic).
- `fn aggregate_effects(village_smalls: &[ArtifactDef], account_wide: &[ArtifactDef]) -> ArtifactEffects` —
  fold each holding's `(kind, magnitude)` into the matching factor (small = holding village only; large +
  unique = account-wide); multiple stack multiplicatively.
- `fn required_treasury_level(scope, rules) -> u8`; `fn can_capture(treasury_level, required_level,
  attacker_already_holds: bool) -> bool` (Treasury ≥ required **and** the village holds none yet).
- `BuildingKind::Treasury` added (population/capacity tables, construction prereqs in balance).
- **Unit tests:** aggregation by scope (small vs account-wide; stacking); Fool determinism; capture
  eligibility truth table; required level per scope; effects default to identity with no holdings.

## Balance (`specs/balance/artifacts.toml` + `construction.toml` + loaders)

- `artifacts.toml` — the released set (`[[artifacts]]`: id, kind, scope, magnitude), the per-scope Treasury
  level requirements, and the Natar garrison spec (unit + base/per-artifact strength, seeded). Faithful-ish
  magnitudes (e.g. Speed large ×2.0 beyond a threshold, Storage ×1.5–3, Sustenance ×0.75…).
- `construction.toml` — Treasury cost/time/prereqs + population; `economy.toml` population table gains a
  Treasury row.
- Loaders: `artifact_catalogue() -> Vec<ArtifactDef>` (+ garrison + treasury-level rules), fail-fast.

## Persistence (`infrastructure` + migrations `0031`/`0032`)

- `users` += `is_npc boolean NOT NULL DEFAULT false` (the synthetic **Natar** account).
- `villages` += `is_natar boolean NOT NULL DEFAULT false`.
- `artifacts (id text PK, world_id, kind, scope, magnitude, holder_village uuid NULL REFERENCES villages
  ON DELETE SET NULL, origin_x int, origin_y int, released_at)` — one row per released artifact; the
  **holder** is the village currently holding it (a Natar village at release).
- `worlds` += `artifact_release_at timestamptz NULL`; `World` struct + `ensure_world` carry it (computed
  from config — an offset/date).
- `ArtifactRepository` (port + impl):
  - `release_artifacts(now, seed, catalogue, garrison) -> usize` — **idempotent** one-shot: if unreleased
    and `now ≥ release_at`, ensure the NPC user, place N Natar villages on the first N reserved Natar tiles
    (deterministic ring order), seed each garrison, insert artifacts held by their Natar village. Guard by
    existing-artifacts.
  - `artifact_at_village(village) -> Option<ArtifactDef>`; `held_by_player(player) -> Vec<(ArtifactDef,
    VillageId)>`; `account_wide_effects(player)` / `village_effects(village)` inputs.
  - `capture_artifact(artifact_id, to_village)` — used inside the battle tx (see Application).
- Battle apply gains an optional **artifact transfer** (set `holder_village`), executed in the same tx.

## Application

- `combat::resolve_attack_one` — when `outcome.attacker_won`, decide an **artifact capture**: read the
  target's held artifact (if any) + the attacker's home Treasury level + whether it already holds one;
  `can_capture(...)` ⇒ set `BattleApply.artifact_capture = Some(ArtifactCapture { artifact_id, to_village
  })`. The repo's `apply_battle` performs the transfer in the battle transaction (P2/P4). Natar villages
  are normal targets, so no new attack path.
- `artifact::process_due_artifact_release(repo, world_start/release_at, now, seed, catalogue) ->
  Result<usize, RepoError>` — the state-driven release (mirrors the 017/019 due checks); idempotent.
- `artifact::village_effects` / `account_effects` helpers gather a player's/village's holdings and
  `aggregate_effects(...)`.
- **Conquest guard:** block conquering a Natar village (`is_natar` ⇒ never transfers, like a capital).

## Effects wiring (the read hooks)

Each artifact factor multiplies at exactly one existing hook; the holding scope decides village vs account:
- **Speed** → movement order travel time (`slowest_speed` ×, in `order_attack`/`order_reinforcement`).
- **Storage** → `capacities()` (economy read) × on warehouse/granary.
- **Sustenance** → garrison upkeep × in the net-crop read.
- **Trainer** → training time × (`process_due_training`/order).
- **Architect** → siege `razed_levels`/durability × in combat resolution (defender's village).
- **Eyes** → scouting power ×; **Confuser** → scouting defence × (`resolve_scouting`).
The effects are gathered where each path already loads the actor's village/account.

## Interface (`web`)

- Scheduler: a `process_due_artifact_release` tick; `AppState`/main wire the catalogue + release date.
- Map: Natar villages render as a distinct marker; held-artifact locations are visible (public, §7.3).
- Village view: a panel listing the player's **held artifacts** (type, scope, effect, holding village).
- **Exclusions:** boards/stat pages (`population_board`, `conflict_board`, `climber_board`, alliance
  boards, `player_stats`) filter `is_npc`/`is_natar`; the abandonment sweep skips the NPC user.

## Test strategy

| AC | Test |
|----|------|
| AC1 | infra (DB): before release nothing; `release_artifacts` at/after the date materializes the set once; re-run is a no-op. |
| AC2 | infra: Natar villages exist with a seeded garrison, `is_natar`, NPC owner; not on boards; not conquerable. |
| AC3 | domain: `required_treasury_level`/`can_capture` truth table; infra: holding requires a Treasury (one per village). |
| AC4 | infra (DB): a winning attack from a Treasury village vs a Natar artifact moves it to the attacker; no/low Treasury ⇒ no transfer. |
| AC5 | infra (DB): a winning attack vs a player holder steals the artifact under the same rules. |
| AC6 | domain: `aggregate_effects` by scope + stacking; infra/app: a held Speed/Storage artifact changes the relevant read; losing it reverts. |
| AC7 | domain/infra: seeded Natar garrison + Fool determinism; config drives the set. |
| AC8 | web: Natar on the map; the holdings panel; boards exclude NPC/Natar. |

## Notes / open risks

- **NPC reuse is the crux.** Treating Natar villages as `villages` owned by `is_npc` reuses combat but
  requires the exclusion filters (boards/stats/sweep/conquest). Each is a one-clause filter mirroring the
  019 `abandoned_at` filter; enumerated above so none is missed.
- **Effect breadth.** All 8 types are wired, but each at a single hook with a clear factor; the domain
  aggregation is exhaustively unit-tested, with representative infra/web coverage per hook rather than a
  full combinatorial matrix.
- **Release date for tests.** `artifact_release_at` is settable (a past instant in tests triggers immediate
  release); production computes it from config.
- **Phasing (T1–T7):** T1 domain (artifact model + effects + Treasury) + balance; T2 world release-date
  schedule (config + worlds column + World); T3 persistence + `release_artifacts` (Natar materialization);
  T4 capture/steal in the battle path; T5 effects wiring across the read hooks; T6 web (release tick,
  AppState, map, holdings panel, exclusions); T7 docs + reviewer + finalize.
