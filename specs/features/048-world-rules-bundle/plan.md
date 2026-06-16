# Feature 048 — `WorldRules` bundle — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md)

## Approach

A mechanical, compiler-driven consolidation. Define the bundle, then let the type system find every site
that read an individual rule `Arc`. Behaviour-preserving — the bundle holds today's `classic` rules, so the
existing suite is the oracle. No domain change (P3).

## Stages (each a commit; suite green before advancing)

1. **Define `WorldRules` + loader.** `infrastructure/src/world_rules.rs`: the struct + `load_world_rules()`
   (assembles from the existing balance loaders); re-export. No call-site change yet. (AC1)
2. **Registry on the bundle.** `WorldRegistry::new` takes `Arc<WorldRules>`; fields/`build_and_spawn` read
   `self.world_rules.<set>`; `main.rs` + harness pass the bundle. Suite green. (AC3/AC4)
3. **`AppState` on the bundle + handler sweep.** Replace the individual rule fields with
   `world_rules: Arc<WorldRules>`; mechanically re-point handler reads; `main.rs` + harness build the bundle.
   Suite green (home parity). Spec/plan/tasks. (AC2/AC4)

## Key decisions

- **Owned values behind one `Arc<WorldRules>`**, not a struct of `Arc`s — one allocation per preset; field
  reads (`&state.world_rules.economy`) deref through the outer `Arc`. Background tasks clone the single
  `Arc<WorldRules>`.
- **`fair_play` excluded** — rate limiting / detection are process-level, not world flavour (per ADR 0035).
- **Bundle lives in `infrastructure`** next to the balance loaders it assembles; used by `web`
  (`AppState`/registry) and, through the registry, the scheduler.

## Risk

- High edit surface (every `state.<rule>` read, the registry constructor) but zero logic change — `clippy`
  type/borrow errors + the full suite catch any mis-mapping. No performance/schema impact (one `Arc` vs
  many; the same data).
