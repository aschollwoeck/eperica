# Feature 127 — the in-game player manual: informative, visual, always true

**Status:** Draft
**Depends on:** 047–053 (WorldRules/presets — the data source), 045 (world context), the
docs/manual content base (audited in PR #141).
**Origin:** operator review — the markdown manual is factually right but "far away" from a real
player manual: not informative enough, not visually appealing, and not reachable from the game.

## Goal

`/manual` is part of the game: a **public**, game-styled manual with guided sections, chapters
written in task-oriented player language, and **reference pages whose stat tables are generated
from the live balance rules** — the numbers can never go stale again, and they show the values of
**the world you're playing** when one is selected.

## Concepts

- **Hybrid content pipeline.** Prose chapters stay markdown in `docs/manual/` (single source for
  repo readers and the site), embedded at compile time and rendered server-side (pulldown-cmark)
  into the game chrome. **Data lives in generated reference pages** — native templates fed from
  the loaded `WorldRules` (units, buildings & prerequisites, costs, culture/settling thresholds,
  trade, walls/siege, protection/lifecycle). Prose explains *how*; tables render themselves.
  Prose chapters link into the reference ("full stats →") instead of embedding big number tables.
- **World-aware numbers.** Anonymous readers (or no world selected) see the `classic` preset with
  a banner. A logged-in reader with a selected world sees **that world's preset and speed**: a
  world banner ("Values for *Botland* — speed 100×, classic rules") and speed-adjusted durations
  where applicable (training/build times). Never trust the client for this (P4): the world comes
  from the session's world context.
- **A manual that looks like the game.** Manual layout inside the base chrome: a section/chapter
  sidebar, breadcrumbs, prev/next footer nav, styled tables, and **callouts** (Tip/Warning/
  Faithful-note) authored as a markdown convention (`> **Tip:** …`) and styled by CSS. Stylish and
  characterful per the design direction — no layout rework of the game itself.
- **Guided structure.** Six sections replacing the flat index: **Getting started** (worlds,
  first village, quests) · **Economy** (resources, buildings, trade) · **Military** (units,
  training, combat, scouting, siege, oases) · **Expansion** (culture, settling, conquest,
  protection & lifecycle) · **Society** (alliances, communication, rankings, medals, profiles,
  sitting, fair play, AI players, spectating) · **Reference** (the generated pages + end-game:
  artifacts, Wonder). The markdown files stay flat; a compile-time registry defines the grouping.
- **Task-oriented prose.** Chapters are rewritten to answer player questions ("How do I raid?",
  "Why is my crop negative?"), lead with what to click, use short sections, callouts and
  cross-links — the audited facts of PR #141 are the raw material, not the final voice.

## Acceptance criteria

- **AC1 — Public routes.** `/manual` (section index) and `/manual/{slug}` render every registered
  chapter without login; unknown slug → 404. The old flat `docs/manual/README.md` index is
  regenerated to mirror the same six sections (repo and site never disagree on structure).
- **AC2 — Markdown pipeline.** Chapters render from the same `docs/manual/*.md` files the repo
  shows: headings, tables, links (intra-manual `foo.md` links rewritten to `/manual/foo`),
  callout blockquotes styled. No raw HTML injection (markdown source is trusted repo content,
  but the renderer escapes inline HTML anyway).
- **AC3 — Generated reference pages.** At minimum: **Units** (per tribe: stats, costs, upkeep,
  train time, prerequisites), **Buildings** (purpose, prerequisites, max level, key per-level
  values for Warehouse/Granary/Main Building), **Mechanics numbers** (CP thresholds, expansion
  slots, settlers, loyalty, walls & ram/catapult durabilities, merchants, protection/lifecycle
  windows, fair-play limits). Every figure comes from `WorldRules`/balance structs at render time
  — zero hand-written numbers on these pages. Values equal the loaded preset's TOMLs (tested).
- **AC4 — World-awareness.** With a world selected, reference pages show that world's preset
  values and speed-adjusted durations plus the world banner; anonymous shows classic + banner.
  Switching worlds switches the numbers (tested with a speed-preset world).
- **AC5 — Navigation & appearance.** Sidebar with six sections (current chapter highlighted),
  breadcrumbs, prev/next, styled tables/callouts consistent with the game theme. The manual is
  linked from the site footer, the register page, and the in-game nav/help spot.
- **AC6 — Prose rework.** Every chapter is rewritten task-oriented per the Concepts voice rules;
  big number tables are moved out of prose into the reference (prose keeps only illustrative
  values with links). The PR #141 factual audit still holds (no number regressions — spot-check
  tests on rendered pages for a few known facts).
- **AC7 — P11.** Manual pages are cheap: content embedded at compile time, rendered per request
  from memory (or cached), the only DB work being the standard session/world lookup middleware
  already on every page.

## Roles & permissions

Per [roles.md](../../roles.md): the manual is public (Visitor-readable); no role changes. No
mutating routes. World-awareness reads the viewer's own session world only.

## Out of scope

- Screenshots/illustrations pipeline (a later art pass; the CSS leaves room for images).
- Full-text manual search (the site search stays game-entity search).
- Localization.
- Operator docs (`docs/operations/`, agent/spectator API contracts) — repo-only remains correct.
