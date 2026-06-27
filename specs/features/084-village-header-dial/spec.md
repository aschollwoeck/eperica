# Feature 084 — village header: pin the loyalty dial to the name row

## Why

On the village command header (069), the loyalty dial wrapped onto a **second row** below the name/quick-links
block (the header was `flex-wrap: wrap` + `align-items: flex-end`, so the wide id block pushed the dial down).
That added a tall empty band of whitespace and pushed every card below it further down.

This pins the dial to the **top-right of the name row** so the header is compact.

Presentation only — one CSS rule change; no template/markup/behaviour change (P3).

## Acceptance criteria

- **AC1 — Dial in the name row.** The loyalty dial renders at the top-right of the command header, in the row
  with the village name (not wrapped onto a row below it).
- **AC2 — Less whitespace.** The header is shorter (no empty wrapped-dial band), so the wonder-site notice,
  ribbon and cards sit higher.
- **AC3 — Responsive.** No horizontal overflow on mobile; the dial stays on the right of the name row.

## Constitution

- **P3** — pure presentation (CSS). **P11** — no query.

## Out of scope

- Any change to the dial's value/meaning or the rest of the header content.
