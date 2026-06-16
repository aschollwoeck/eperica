# Per-world configuration — operator-tunable worlds & per-world rule presets

**Status:** Proposed (program design) · **Date:** 2026-06-16 · **Slices:** 047 (end-game schedule) +
048–05x (per-world rule presets)
**Depends on:** ADR 0034 (multi-world & administration; the `worlds` row, the registry, `GameContext`/
`WorldScope`, the per-world scheduler).

## Context

After M9 (036–046) the server runs many worlds concurrently, but a world is shaped by only **two**
operator-set knobs at creation — **speed** and **map radius** (`/admin` → `POST /admin/world`). Everything
else that shapes a world is fixed:

- **End-game schedule.** The artifact/Wonder release dates are stored per-world (`worlds.artifact_release_at`
  / `wonder_release_at`) but are **not on the form**. Worse, there are *two* sources of truth: the **boot**
  world uses the env offsets (`ARTIFACT/WONDER_RELEASE_DELAY_SECS`), while **admin-created** worlds use
  **hardcoded constants** in `admin.rs` (`90d`/`120d`) and ignore env. They coincide today, so it's latent.
- **The rule/balance set.** Every world shares **one** global balance — the `specs/balance/*.toml` files are
  loaded once at boot, `Arc`-wrapped, and handed to both `AppState` and the registry. So a "3× speed" world
  is *only* a linear time-scale (P7 `GameSpeed`); it cannot differ in **beginner-protection duration, troop
  speeds, merchant speed, build/research curves, culture thresholds, combat point values**, etc. Faithful
  Travian runs distinct **server types** ("classic 1×", "speed 3×", "fire-and-sand") that differ in exactly
  these rules — not just a global multiplier.

The env audit (which settings are world-scoped) lands cleanly: **per-world** = speed, radius, the two
release offsets, and the rule set; **process-global** = `DATABASE_URL`, `SESSION_SECRET`, `BIND_ADDR`,
`RUST_LOG`; **account-global** = `ADMINS`, `MODERATORS` (a human's role is not per-world). Only the
world-scoped ones belong on the world; the rest stay in env.

## Decision

### A. End-game schedule is operator-set (slice 047, standalone)

Add **artifact-release** and **Wonder-release** offsets (days) to the world-creation form, validated
`0 < artifact < wonder` (the Wonder phase must follow the artifact phase, GDD §13.2/§11.3). Unify the two
sources of truth: the **env** `ARTIFACT/WONDER_RELEASE_DELAY_SECS` become the **form defaults**, and the
admin path stops using its hardcoded constants — both the boot world and admin worlds flow through one
code path. This is already 90% plumbed (`AdminRepository::create_world(speed, radius, artifact_offset,
wonder_offset)` exists); only the form + use-case defaults change. Independently valuable; ships first.

### B. Per-world rule presets (the program, 048–05x)

A world carries a **named rule preset**; the registry serves each world its preset's rules.

1. **`WorldRules` bundle.** Group the ~19 **sim** rule sets (economy, build, units, combat, culture,
   loyalty, alliance, ranking, achievements, quests, lifecycle, merchant, wonder, map, scout, oasis,
   artifacts, medals, starting-village) into one `WorldRules` struct, `Arc`-shared. **`fair_play_rules`**
   (rate limits / detection — a process-level anti-cheat concern) and the hashers/hubs/proxy flag stay
   global on `AppState`. This bundle is the unit that becomes per-world.

2. **Preset model: named presets, not per-field override deltas.** A preset is a *complete* balance set
   under `specs/balance/presets/<name>/` (the existing `specs/balance/*.toml` becomes the **`classic`**
   preset). Chosen over sparse per-world override deltas because: a preset is a coherent, designer-authored,
   testable whole (P2 reproducibility); it is finite and cacheable (load once per distinct preset, not per
   world); and it matches how operators actually think ("run a speed server"), not "tweak field 7". Override
   deltas can layer on later if a need appears, but the base unit is a named preset.

3. **`worlds.rule_preset` column** (text, default `'classic'`). Set at creation (admin picks from the
   available presets); immutable for a running world (changing rules mid-round would break P2 within a
   round — a new preset is a new world).

4. **Registry serves per-world rules.** `WorldRegistry` caches `preset_name → Arc<WorldRules>` (load on
   first use, like the per-world meta cache). `context_for(world)` additionally returns the world's
   `Arc<WorldRules>`; the per-world **scheduler** (040) uses its world's bundle.

5. **Context carries the rules.** `GameContext` (043) and `WorldScope` (046) gain a `rules: Arc<WorldRules>`
   field; game handlers read `ctx.rules.*` instead of `state.*`. The boot/home path keeps `classic`, so
   home behaviour is byte-for-byte preserved.

## Slice decomposition (build order)

Sequenced low-risk-first; the heavy refactors (the bundle, the handler sweep) are isolated and
behaviour-preserving, gated per slice by the reviewer/PR.

| #   | Slice | Risk | Delivers |
|-----|-------|------|----------|
| 047 | **Per-world end-game schedule** | Low | Artifact/Wonder offsets on the create form (validated `0 < a < w`); env values as defaults; one code path for boot + admin worlds (fixes the env-vs-hardcoded split). Independently useful; no rule program needed. |
| 048 | **`WorldRules` bundle** | High | Consolidate the ~19 sim rule `Arc`s into one `WorldRules` struct loaded once (= the `classic` preset); thread it through `AppState`/registry/handlers in place of the individual fields. Pure refactor, no behaviour change. The keystone (mirrors 037). |
| 049 | **Preset loader + `worlds.rule_preset`** | Med | Load `WorldRules` by preset name from `specs/balance/presets/<name>/` (move current balance → `presets/classic/`); add the column (default `classic`); boot world uses `classic`. Single preset in practice ⇒ behaviour-preserving. |
| 050 | **Registry serves per-world rules** | High | `WorldRegistry` caches `preset → Arc<WorldRules>`; `context_for` returns it; the per-world scheduler uses its world's bundle. Still all-`classic` ⇒ preserving. |
| 051 | **Handlers read rules from context** | Med | `GameContext`/`WorldScope` carry `Arc<WorldRules>`; migrate game handlers `state.rules.* → ctx.rules.*` (the handler sweep, mirrors 044). |
| 052 | **Admin preset selection + a real 2nd preset** | Med | Preset dropdown on the create form (server-authoritative: only a known preset); ship a genuine **`speed`** preset (shorter protection, faster troops/merchants, steeper curves) so per-world rules are actually exercised end-to-end. |
| 053 | **Acceptance + docs** | Low | Two worlds on different presets prove divergent rules (e.g. protection duration); home parity; ADR → Accepted. |

047 stands alone. 048–051 are the heavy lift that must all land to reach per-world rules. 052–053 turn it
on for operators and prove it.

## Reuse / decisions

- **Named presets over override deltas** — a coherent, testable, cacheable whole (P2); finite set; matches
  operator mental model. Deltas remain a future option layered on top.
- **`WorldRules` bundle mirrors the 037 player keystone** — one invisible, high-surface refactor isolated
  from behaviour change, so the existing suite is the regression oracle.
- **Reuse the registry's per-preset cache** (like the per-world meta cache) rather than loading rules per
  world; reuse `GameContext`/`WorldScope` rather than a new extractor.
- **Rules immutable per running world** — preserves P2/P6 reproducibility within a round; a different
  ruleset is a different world, consistent with the round-based model (the registry-add, not hot-swap,
  principle from 0034).
- **`fair_play` stays global** — rate limiting and detection are process/account concerns, not world flavour.

## Consequences

- 048 touches a large surface (every `state.<rule>` read) but is user-invisible — the risk concentrate,
  deliberately isolated, like 037.
- Per-world rules multiply the **balance test matrix**: each preset is independently validated; the 023
  scale + P11 budgets apply per preset (the bundle is an `Arc`, shared across a preset's worlds — no
  per-world memory blow-up).
- P5 (stateless app tier, state in the DB) holds: the registry's preset cache is a DB-/file-derived cache
  rebuilt on boot; `worlds.rule_preset` is the source of truth.
- Designers gain a real authoring surface (`presets/<name>/`); operators gain Travian-style server types.
  Visual/theming per server type is **out of scope** here (a separate UI concern).
