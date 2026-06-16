# Feature 047 — Per-world end-game schedule — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0035](../../../docs/architecture/0035-per-world-configuration.md)

## Approach

A small, self-contained change: the offsets are already plumbed to `create_world`; expose them on the form,
validate them, and make the env values the single default source. Behaviour-preserving when the defaults are
untouched (the existing suite + a new validation/override test are the oracle). No domain change (P3), no
migration.

## Stages (each a commit; suite green before advancing)

1. **Use case takes validated offsets.** `application::create_world(.., artifact_offset_secs,
   wonder_offset_secs)` validates `0 < artifact < wonder` (`AdminError::InvalidWorld`); drop the
   `ARTIFACT/WONDER_RELEASE_DEFAULT_SECS` constants. Unit tests for the new validation (home parity for the
   default values). (AC2/AC3)
2. **Wire the env defaults + form.** `AppState` carries the env offsets; `CreateWorldForm` gains optional
   `artifact_days`/`wonder_days`; the handler resolves days→secs (default = env) and passes them; the
   `AdminTemplate` + `admin.html` form gain the two inputs prefilled with the default days. (AC1/AC3)
3. **Acceptance.** Integration: creating a world with a custom schedule sets its release dates; an invalid
   schedule (`wonder ≤ artifact`) is rejected with no world created; omitting the fields uses the env
   default. Full suite green; spec/plan/tasks. (AC1/AC2/AC4)

## Key decisions

- **Days on the form, seconds in the core.** Operators think in days; the domain/DB keep seconds. The
  handler is the only conversion point (`days * 86400`).
- **Optional fields, env default.** An omitted field falls back to the operator's env default, so the form
  is forgiving and the env stays the deployment-wide default — one source of truth (AC3).
- **Creation-time only.** No mid-round schedule edits (would fight the 021 end-game freeze invariants); a
  different schedule is a new world.

## Risk

- Tiny surface; the only correctness point is the `0 < artifact < wonder` validation, covered by a unit test
  (reject) + an integration test (accept/override/default). No performance or schema impact.
