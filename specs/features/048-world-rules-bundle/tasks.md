# Feature 048 — `WorldRules` bundle — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving —
the bundle holds today's `classic` rules; the existing suite is the regression oracle. No pure-domain task.

- [x] **T1 — Define `WorldRules` + `load_world_rules()`.** `infrastructure/src/world_rules.rs`: the struct
  (owned sim rule sets) + the loader assembling from the existing balance loaders; re-export from the crate.
  No call-site change. (AC1)

- [x] **T2 — Registry on the bundle.** `WorldRegistry::new` takes one `Arc<WorldRules>` (drop the ~15 rule
  args); fields + `build_and_spawn` read `self.world_rules.<set>`; `main.rs` + the test harness pass the
  bundle. Suite green. (AC3/AC4)

- [x] **T3 — `AppState` on the bundle + handler sweep.** Replace `AppState`'s individual rule fields with
  `world_rules: Arc<WorldRules>`; re-point handler reads (`state.<rule>` → `state.world_rules.<set>`);
  `main.rs` + harness build the bundle. Full suite green (home parity). Spec/plan/tasks. (AC2/AC4)
