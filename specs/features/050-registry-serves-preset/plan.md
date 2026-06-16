# Plan — 050 registry serves each world's rule preset

## Approach

A behaviour-preserving plumbing refactor: route every per-world sim read through the world's resolved bundle.
The registry becomes the single resolver (preset name → `Arc<WorldRules>`), the context carries the resolved
bundle, and handlers read from the context. `classic` is the only preset, so behaviour is identical; the seam
is now per-world.

## Tasks (serial, gated per task)

- **T1 — Registry: per-preset bundle cache + serve through context/scheduler.**
  `registry.rs`: `presets: Mutex<HashMap<String, Arc<WorldRules>>>` (seeded from the boot bundle via the new
  `new(.., boot_preset, boot_rules)` signature); `WorldMeta` gains `rules: Arc<WorldRules>` (→ `Clone`);
  `rules_for(preset)` resolver (cache-or-`load_world_rules`, log+`None` on failure); `context_for` returns
  `(repo, map, speed, radius, rules)` built from the world's bundle; `build_and_spawn` resolves + uses the
  world's bundle. Update `main.rs` to the new `new` signature. Unit test: `rules_for` caches; an unknown
  preset yields `None`.

- **T2 — Context carries the world's rules.**
  `auth.rs`: add `pub rules: Arc<WorldRules>` to `GameContext` and `WorldScope`; populate from `context_for`.

- **T3 — Handlers read rules from the context.**
  `handlers.rs`: scoped replacement — `GameContext` handlers + `village_view_data` → `ctx.rules`; `WorldScope`
  handlers → `world.rules`; the 4 non-world-scoped handlers keep `AppState.world_rules`. `AppState` keeps its
  `world_rules` field (home bundle).

- **T4 — Acceptance test + slice verification.**
  Integration test: two worlds at different speeds; assert `GameContext.rules` is served per selected world
  (and that the context's rules object is the registry-resolved bundle, not the global). Full gate + reviewer.

## Gates (every task)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
`cargo test --workspace`; P11 latency budget (no new per-request DB work — the bundle is cached).

## Risks

- **Wrong-bucket replacement** in `handlers.rs` (a non-world-scoped handler getting `ctx.rules`, or vice
  versa) — mitigated by the function-boundary-scoped script + the compiler (a handler without `ctx`/`world`
  in scope fails to compile).
- **Per-request cost** — `context_for` must not reload balance per request; the preset cache + meta cache
  keep it allocation-light (one `Arc` clone). Covered by the existing scale guard.
