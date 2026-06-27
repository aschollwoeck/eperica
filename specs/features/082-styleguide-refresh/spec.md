# Feature 082 — refresh the component gallery (/styleguide)

## Why

`/styleguide` is the living component reference, but it predated the whole redesign: it loaded the **old
`theme-ash`** stylesheet (the app ships `theme-steel`) and showed only a handful of legacy components, none of
the design-system pieces built across 066–081. This refreshes it into an accurate reference + visual smoke test.

Presentation only — a standalone template; no domain/route/handler change (P3).

## Acceptance criteria

- **AC1 — Correct theme.** The gallery loads `theme-steel.css` (the shipping theme), not `theme-ash`.
- **AC2 — Design-system coverage.** It showcases the shared components: the `.phead` header, palette tokens,
  buttons/badges (incl. small/danger/disabled), `.bld-card` + `.statline`, the resource ribbon (`.gauge`), the
  unified roster `.unit` row, `.tabs`, `.repcard`, `.conversations` + chat `.messages` bubbles, alerts, form +
  `.checkbox` + `.table`, and the `.wonder-board`.
- **AC3 — Preserved.** Standalone page (own `<head>`), the favicon link kept; reachable at `/styleguide`.

## Constitution

- **P3** — pure presentation; one template. **P11** — static page, no query.

## Out of scope

- Removing now-unused legacy CSS (`.swatch`, `.resource-list`, `.hero`) — a separate cleanup if desired.
