# Tasks — 085 village density

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation; branch `feature/085-village-density`.

- [x] **T1 — Full width.** Add `{% block body_class %}bld-page{% endblock %}` to village.html.
- [x] **T2 — Denser fields.** `.vfields__grid` minmax 150→136 + gap 12→8; `.vfield` padding + `min-width: 0`;
  `.vfield__n`/`__r` `display: block` + nowrap + ellipsis (single-line truncation); plan canvas 520→460.
- [x] **T3 — Verify.** Live measurement — desktop 2688→1680px, fields 1068→~320px, no overflow; mobile no overflow.
- [x] **T4 — Test.** Assert the village page contains `bld-page`.
- [ ] **T5 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; reviewer APPROVE; PR opened.
