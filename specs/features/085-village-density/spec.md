# Feature 085 — village page: less scrolling (full-width + denser fields)

## Why

The village page required ~3.5 screens of scrolling (2688px @ 1366×768). Root cause: the page was missing
the `bld-page` body class, so it was capped at the **default 880px container** instead of the 1220px `.vlg`
width. The cramped ~460px main column forced the 18 resource fields into a **2-column × 9-row** grid (1068px
tall on its own). The relevant information (resources, the fortress plan, build actions) sat far below the fold.

This widens the page to its intended full width and tightens the fields/plan so the key information is
reachable with far less scrolling.

Presentation only — one template class + CSS; no behaviour change (P3).

## Acceptance criteria

- **AC1 — Full width.** The village page carries `bld-page`, so `.vlg` uses its 1220px width and the main
  column gets room (the resource fields render in ~5 columns, not 2).
- **AC2 — Denser fields.** Each field card is a single compact row (name + rate truncated with ellipsis to one
  line each); the grid is tighter (`minmax(136px)`, 8px gap). The 18-field grid drops from ~1068px to ~320px.
- **AC3 — Shorter page.** The fortress-plan canvas trims 520→460px; total page height drops ~37% (≈2688→1680px
  @ 1366×768), bringing the header, ribbon and plan into the first screen.
- **AC4 — No overflow.** No horizontal overflow at desktop or mobile (the field text truncates rather than
  forcing width).

## Constitution

- **P3** — pure presentation (a template class + CSS). **P11** — no query.

## Out of scope

- Reordering/removing any village section; the rail and plan content are unchanged.
