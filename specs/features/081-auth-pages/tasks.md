# Tasks — 081 auth pages

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation only; branch `feature/081-auth-pages`.

- [x] **T1 — CSS.** Add `.auth`/`.auth__brand`/`.auth-card`(`--wide`)/`.auth-card__{title,sub,alt}` (reuse
  `.form`/`.field`/`.choice`/`.alert`/`.btn--wide`).
- [x] **T2 — login.html / register.html.** Centered branded auth card; all fields/forms/world+tribe/POSTs +
  autocomplete kept.
- [x] **T3 — Tests.** Assert `auth-card`/`auth__brand` on login + register, keeping the tribe/flow/autocomplete
  assertions.
- [ ] **T4 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
