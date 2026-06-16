# Feature 047 — Per-world end-game schedule — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving at
the default schedule — the existing suite is the regression oracle.

- [x] **T1 — Use case takes validated offsets.** `application::create_world` gains `artifact_offset_secs` +
  `wonder_offset_secs` params, validates `0 < artifact < wonder` (`AdminError::InvalidWorld`), and drops the
  `ARTIFACT/WONDER_RELEASE_DEFAULT_SECS` constants. Unit tests: reject `wonder ≤ artifact` / non-positive;
  accept the defaults. (AC2/AC3)

- [x] **T2 — Env defaults + form inputs.** `AppState` carries the env offsets; `CreateWorldForm` gains
  optional `artifact_days`/`wonder_days`; the handler resolves days→secs (default = env) and passes them;
  `AdminTemplate` + `admin.html` gain the two day inputs prefilled with the defaults. (AC1/AC3)

- [x] **T3 — Acceptance + regression.** Integration: a custom schedule sets the world's release dates; an
  invalid one is rejected (no world created); omitting the fields uses the env default. Full suite green;
  spec/plan/tasks. (AC1/AC2/AC4)
