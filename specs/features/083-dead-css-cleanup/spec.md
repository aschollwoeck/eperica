# Feature 083 — remove CSS orphaned by the redesign

## Why

The 066–082 redesign replaced many components, leaving their old CSS (and an alternate theme file) referenced
by nothing — flagged by the 082 review. This removes the verified-dead rules so `base.css` documents only what
ships.

Pure cleanup — **no behaviour change** (P3); every removed selector was confirmed to have **zero** references
across all templates, the handler-generated class strings, and the inline JS.

## Acceptance criteria

- **AC1 — Dead rules removed.** `.has-building-bg` (building pages use `.bld-page`/`.bld-hero` now),
  `.unit-cell`/`.unit-thumb` (063 → `.unit__thumb`), `.resource-list`/`.res`/`.res--*`/`.resbar*` (032 bars →
  the `.gauge` ribbon), `.hero`/`.gallery`/`.swatch*`/`.actions`/`.theme-switch` (old styleguide), and the
  `.res` in the `.res, .num` selector — all removed.
- **AC2 — Orphaned theme removed.** `static/theme-ash.css` (referenced by nothing after 082 switched the
  styleguide to `theme-steel`) is deleted.
- **AC3 — No regression.** Every live component (`.badge`, `.table`/`.num`, `.gauge`, `.unit__thumb`,
  `.map-grid__cell*`, `.statcard`, `.choice__option`, `.alert--*`, `.banner`) keeps its styling; the suite
  stays green.

## Constitution

- **P3** — pure presentation; CSS + one file deletion. **P11** — no query.

## Out of scope

- Any further consolidation of inline `style=` attributes into utility classes.
