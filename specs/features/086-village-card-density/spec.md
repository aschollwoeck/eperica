# Feature 086 — village header: links beside the name + tighter cards

## Why

Two readability asks on the village page:
1. The cards carried a lot of vertical whitespace — chiefly because the card/section **headings**
   (`<h3>`/`<h2>`) kept their default browser margins (~32px), nearly doubling each header's height.
2. The quick-links sat in a separate row below the name/sub; the user wants them **beside the village name**.

## Acceptance criteria

- **AC1 — Links beside the name.** The quick-links (Map/Rally/Market/Academy/Smithy/troops/Reports/Quests/
  Alliance) flow to the **right of the village name** on the same row (wrapping individually as needed), via a
  `.vcmd__namerow` flex wrapper with the `.vquick` box flattened (`display: contents`, scoped to the village —
  the map's `.vquick` is untouched).
- **AC2 — Tighter card headers.** The card/section headings reset their margin to 0, halving the header height
  (e.g. a rail card header ~66→~34px). Card body/section paddings and inter-card gaps are trimmed modestly.
- **AC3 — No regression.** No horizontal overflow at desktop or mobile; the map header (shared `.vquick`) and
  the building pages (shared `.bld-card`/`.bld-cols__head`) still render correctly.

## Constitution

- **P3** — pure presentation (one template restructure + CSS). **P11** — no query.

## Out of scope

- Any change to which links/sections appear, or to the loyalty dial / ribbon.
