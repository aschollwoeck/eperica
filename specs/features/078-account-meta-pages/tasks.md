# Tasks — 078 account/meta pages

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation only; branch `feature/078-account-meta-pages`.

- [x] **T1 — CSS.** Add `.btn--small`/`.btn--danger`, `.checkbox`, `.inline`/`.inline-form` (reusing
  `.phead`/`.page`, `.bld-cols__head`, `.bld-card`, `.table`, `.conversations`, `.choice__option`, `.banner`).
- [x] **T2–T5 — Templates.** `.phead` + section heads + the components on worlds, settings, search, sitting;
  all routes/forms/links preserved.
- [x] **T6 — Tests.** Assert `phead` (+ `checkbox`) across worlds/settings/search, keeping the content/flow
  assertions.
- [ ] **T7 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright; reviewer APPROVE; PR.
