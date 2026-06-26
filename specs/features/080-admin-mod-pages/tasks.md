# Tasks — 080 admin & moderation pages

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation only; branch `feature/080-admin-mod-pages`.

- [x] **T1 — admin.html.** `.phead` + a `.statgrid` server summary + section heads over the server detail /
  worlds + create-world / accounts + role table; all forms/gates/timestamp-JS kept.
- [x] **T2 — mod_queue.html.** `.phead` + the reports table + resolve form.
- [x] **T3 — mod_account.html.** `.phead` + Status + Detection-signals cards + the sanction form.
- [x] **T4 — Tests.** Assert phead/statgrid on the admin console; migrate the count assertions from `<td>` to
  the stat cards; keep the role-management/world-creation/sanction flow assertions.
- [ ] **T5 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
