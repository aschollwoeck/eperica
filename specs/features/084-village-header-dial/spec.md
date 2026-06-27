# Feature 084 — village header: pin the loyalty dial to the name row

## Why

On the village command header (069), the loyalty dial wrapped onto a **second row** below the name/quick-links
block (the header was `flex-wrap: wrap` + `align-items: flex-end`, so the wide id block pushed the dial down),
adding a tall band of whitespace that pushed every card below it down. Pin the dial to the **top-right of the
name row** so the header is compact.

The `.vcmd` chrome is shared with the **map** header (whose right-hand child is the recenter `.map-nav`, not a
dial), so the fix is **scoped** to the village header via a `.vcmd--head` modifier — the map keeps its default
wrap so its wide nav can still drop full-width below on narrow screens.

Presentation only — one CSS modifier + one template class; no behaviour change (P3).

## Acceptance criteria

- **AC1 — Dial in the name row.** On the village header, the loyalty dial renders at the top-right, in the row
  with the village name (not wrapped onto a row below), via `.vcmd--head` (`flex-wrap: nowrap` + the id
  `flex: 1`). The header is shorter, so the cards below sit higher.
- **AC2 — Map unaffected.** The map header keeps the default `.vcmd` wrap: its recenter nav still drops
  full-width below the title on narrow screens (no taller-header regression).
- **AC3 — Responsive.** No horizontal overflow on either header at mobile width.

## Constitution

- **P3** — pure presentation (CSS + a template class). **P11** — no query.

## Out of scope

- Any change to the dial's value/meaning or the rest of the header content.
