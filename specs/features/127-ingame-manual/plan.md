# Plan — 127 the in-game manual

**Status:** Draft (spec approved)

## Constitution check

- **P3/P4:** no game rules touched; world-awareness resolves the preset server-side from the
  session's world context (never a client parameter).
- **P7:** speed-adjusted durations on reference pages use the world's `GameSpeed` through the
  same domain helpers the game uses — no wall-clock literals.
- **P11:** chapters embedded at compile time (`include_str!` via a registry macro/array);
  markdown rendered per request (small pages, no I/O) — measurably trivial next to the session
  middleware that runs anyway.

## Module changes

| Layer | Change |
|---|---|
| `crates/web/Cargo.toml` | + `pulldown-cmark` (default features minus raw-HTML passthrough) |
| `crates/web/src/manual.rs` (new) | the chapter **registry**: `SECTIONS: [(title, [(slug, title, include_str!("../../../docs/manual/<file>.md"))])]` — six sections per the spec; markdown → HTML rendering with: intra-manual link rewriting (`foo.md` → `/manual/foo`), callout classing (blockquote whose first strong is `Tip:`/`Warning:`/`Faithful:` gets a CSS class), heading anchors; inline HTML escaped |
| `crates/web/src/handlers.rs` | `manual_index`, `manual_chapter(slug)` (public — registered on the public router next to `/leaderboard`); reference handlers `manual_ref_units`, `manual_ref_buildings`, `manual_ref_mechanics` — resolve rules: session world's preset + speed if selected, else classic via `load_world_rules("classic")` (cached like the game does); render native templates from the rules structs |
| `crates/web/templates` | `manual_layout` block in a new `manual.html` family: sidebar (sections, active chapter), breadcrumbs, prev/next; `manual_chapter.html` (rendered markdown slot); `manual_units.html`, `manual_buildings.html`, `manual_mechanics.html` (tables straight from rules structs); world/preset banner partial |
| `crates/web/static/base.css` | `.manual` styles: sidebar, tables, callouts (`.co-tip/.co-warn/.co-faith`), prev/next — game-theme tokens only |
| `docs/manual/*.md` | full prose rework (all chapters, voice rules from the spec); big data tables replaced by links to `/manual/reference/...`; README.md regenerated as the six-section index (repo mirror) |
| `crates/web/src/lib.rs` | public routes: `/manual`, `/manual/{slug}`, `/manual/reference/{units,buildings,mechanics}`; footer/nav/register links |

## Key decisions

- **Reference pages are native templates, not markdown macros** — no invented template language
  inside markdown; prose links to reference instead of embedding generated fragments.
- **Registry is compile-time** — a missing file is a build error, a dead slug is impossible;
  the docs and the site cannot drift structurally (AC1 ties README.md to the same registry
  ordering via a test that parses README and compares slugs).
- **Classic fallback via the existing preset loader** (same cache the world registry uses) —
  no second source of balance truth.
- **Callouts as a markdown convention** (`> **Tip:** …`) — renders fine on GitHub too.

## Prose voice rules (for the rewrite, enforced in review)

Lead with the player's task; second person; short sections with descriptive headings; concrete
click-paths ("Village → the barracks slot"); one callout per ~screen max; every number either
small-and-illustrative or a link to reference; end with "See also" links; no spec/slice jargon
(no "slice 114", no "AC"); keep the audited facts (PR #141) — reword, don't re-derive.

## Test strategy

- Unit (`manual.rs`): link rewriting, callout classing, HTML escaping, registry slug uniqueness,
  every registered file non-empty + starts with an `# h1`.
- Integration: AC1 (public 200s for index + every chapter, 404 unknown); AC2 (a chapter's known
  heading + rewritten intra-link present); AC3 (reference pages contain values equal to the
  loaded classic TOMLs — assert a handful across all three pages, e.g. legionnaire attack 40,
  clubswinger cost, warehouse L10 12000, CP threshold list head, ram durability 180);
  AC4 (login + select a `speed`-preset world → the changed value appears + banner; anonymous →
  classic + banner); AC5 (sidebar/breadcrumb/prev-next markers, footer + register links);
  AC6 spot-checks (a few PR-#141 facts still present in rendered prose).
- README-mirror test: registry sections/slug order == README.md structure.

## Risks

- **Prose rewrite volume** (30 chapters): parallel writer agents per section with the voice
  rules + per-chapter fact anchors; review gate re-checks facts against balance data.
- **Markdown edge cases** (tables/links in pulldown-cmark): unit tests on the actual chapter
  corpus — every chapter must render without panicking and contain no unrendered `{{`/`](`
  artifacts.
- **Speed-adjusted display**: only durations get adjusted; produce/costs stay preset values —
  the banner says exactly what is adjusted (avoids implying speed scales costs).
