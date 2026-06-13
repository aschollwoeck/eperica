# Settling — culture points, expansion slots, founding a village, the capital

**Status:** Current
**Date:** 2026-06-13 · **Slice:** 013

## Context
A player starts with **one** village and grows by **settling** (GDD §3.3, §3.4, §11.1). This slice makes
a player **multi-village**: villages produce **culture points (CP)** over time; CP plus a
**Residence/Palace** unlock **expansion slots**; the player trains **settlers** and sends them to a
**free valley** to found a new village that runs its own economy/queues/defence. Building a **Palace**
designates the player's **capital** — the one village that may raise its resource fields **past the
normal cap** and (from 014) cannot be conquered. Conquest itself is out of scope (014); 013 delivers the
**peaceful** expansion path and the capital rule.

## Design
- **CP is a per-player lazy accumulator, mirroring the 002 economy.** `player_culture(player_id, value,
  updated_at)` holds one row per player (seeded at registration); it is **computed on read** (P1) — the
  stored `value` settled forward at the **live rate**, never ticked. The rate is **not stored**: it is
  derived from the player's villages' **Town Hall** levels, `culture_rate(townHallLevels) = Σ (base +
  townHallCp(level))` (pure, P3). Because the read is only correct while the rate was constant over
  `[updated_at, now]`, the row is **re-anchored** (settled to `now` at the rate in effect *up to* that
  instant) at every rate change — **before** a Town Hall completes (`process_due_builds` calls
  `reanchor_culture`) and **when a village is founded** (folded into the founding transaction). CP is
  **never spent**; it is a rising **threshold gate** (`cp_thresholds[n]`), pooled across villages.
- **The slot gate is `min(cpAllows, buildingAllows)`, pure over numbers (P3).** `cp_allows(cp)` is the
  largest `n` with `cp_thresholds[n] ≤ cp`; `buildingAllows = 1 + Σ expansion_slots(level)` over the
  player's Residences/Palaces (the `+1` is the always-present home village). `allowed_villages =
  min(...)`. A founding needs `villageCount < allowedVillages`, **re-evaluated at arrival** (P4) — two
  in-flight settles can both pass dispatch; the second **bounces** when the slot was consumed. All
  constants (`base_cp_per_village`, `town_hall_cp_per_level`, `cp_thresholds`,
  `expansion_slots_per_level`, `settlers_per_village`, `settler_id`, `capital_field_max_level`) are
  balance (`culture.toml`, P7).
- **Settling extends the 007 movement engine with a `settle` kind** (the 012 nullable-`deliver_village`
  pattern: the movement targets a **tile**, carries the settler group). `order_settle` validates the
  source is the player's village, the player has a **free slot** + the **settler group** (the
  `Expansion`-role unit, a non-combatant like the Scout) + a Residence/Palace, and the target is a **free
  valley** on **another tile**; it debits + schedules. `process_due_settles` claims due settles and, in
  **one transaction** (`apply_settle`, exactly-once, orphan-requeue safe via the shared `troop_movements`
  requeue), either **founds** the village or **bounces** the settlers:
  - **Found** (tile still a free valley **and** a slot still free): insert the village at the seeded tile
    (006 field distribution) + the 006 starting buildings + starting resources, owner = the sender; the
    founding is **guarded on the tile still being free** (`ON CONFLICT (world_id, x, y) DO NOTHING` ⇒
    `Conflict` ⇒ a later tick re-validates and bounces); and **re-anchor the player's CP** at the
    founding instant at the **old** rate (the new village joins the live rate from here, P2/AC1).
  - **Bounce** (tile taken/non-valley, or no free slot at arrival): schedule a `Return` of the settlers
    to the source tile (the 012 oasis-bounce pattern). Nothing is founded.
- **The capital is a flag + a field-cap branch.** `villages.is_capital` (a boolean filled on every
  village read, like 012's `oasis_bonus`, so construction validation sees it with no extra query).
  Completing a **Palace** build calls `set_capital(player, village)` — **at most one per owner**,
  cleared-and-set in the apply (per-owner needs the owner join, so it is enforced there, not as a table
  constraint). Construction validation uses `field_max_level(is_capital)`: a capital's **resource
  fields** may reach `capital_field_max_level` (> the normal cap); center buildings are unaffected (AC10).
  The flag also records "unconquerable" for 014 (no behaviour here beyond the cap).
- **The web is now multi-village (AC11).** Every page that showed "the player's village" takes an
  optional **selected village** — `load_economy(selected)` and each action use-case gained a
  `selected: Option<VillageId>` resolved by `select_village` (the chosen owned village, else the
  **capital**, else the first), so single-village play is unchanged. The village page renders a
  **switcher** (`?village=<id>`; the id rides as a **string** because `serde_urlencoded` cannot decode
  `u128`), a **culture panel** (CP + rate + slots used/allowed + next threshold), the **capital** badge +
  its raised field cap, and the Rally Point gains a **Settle** order (offered only with a free slot). The
  map **distinguishes the capital**. The scheduler ticks `process_due_settles` each loop; founding
  re-syncs nothing on the source village garrison, so no extra starvation re-sync is needed.

## Consequences
- **Conquest, administrators, loyalty** (Senator/Chief, ownership transfer) are 014; 013 persists the
  capital's unconquerable flag but adds no conquest mechanics. **Celebrations** (one-off Town Hall CP
  parties) and **resource-boost buildings** (Sawmill/…/Bakery) are later — 013 produces CP only from
  building levels over time, and the capital's benefit here is the **field cap**, not extra multipliers.
- A founded village is **fully independent** (002–009 per-village engines keyed by its own id/tile): its
  own resources, build/unit queues, garrison, and economy. There is no global resource pool.
- The whole loop — CP, the slot gate, the founding-or-bounce decision, and the capital flag — is
  reproducible from persisted rows + the world seed + the 006 template (P2/P6); the founding (or bounce)
  + CP re-anchor apply in **one transaction**, exactly-once and crash-resume safe.

## Links
specs/constitution.md (P1–P4, P6, P7, P11); specs/features/013-settling/;
specs/balance/culture.toml, construction.toml ([buildings.town_hall|residence|palace]), economy.toml
(population), units.toml (settler);
crates/domain/src/culture.rs (CultureRules, culture_rate, settle_value, cp_allows, expansion_slots,
allowed_villages), construction.rs (field_max_level capital branch), building.rs (TownHall, Palace),
village.rs (Village.is_capital);
crates/application/src/culture.rs (load_culture, reanchor_culture), economy.rs (select_village,
pick_village, selectable load_economy), settling.rs (order_settle, process_due_settles), build.rs
(Palace→set_capital, Town Hall re-anchor), ports.rs (CultureRepository, SettleRepository + types);
crates/infrastructure/src/repo.rs (Culture/Settle repo impls, set_capital, apply_settle),
event_store.rs (scheduler settle tick); crates/web/src/handlers.rs (switcher, culture panel, settle,
capital marker), state.rs (culture_rules), templates;
migrations/0018_capital.sql, 0019_culture.sql, 0020_settle.sql.
