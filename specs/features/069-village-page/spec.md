# Feature 069 — the village page (the fortress plan)

## Why

The village page is the heart of the game and the last page on the old stacked-tables layout. The design work
settled on **"the command table"**: a stamped command header, the shared resource ribbon, a top-down
**fortress plan** of the village (the buildings as clickable plots inside a rampart, the leader's keep at the
centre), the 18 resource fields as a grid below, and a **war-room rail** of live feeds — with a **click-to-
inspect** affordance that drives the upgrade action. This slice turns that mockup into the real page.

Presentation only — **no domain/sim change** (P3). Every build/upgrade stays server-authoritative (P4): the
plot only *shows* `level`/`can_order`/`at_max`; the existing `/village/{v}/build` POST and its slot/kind
re-validation are unchanged. Reuses the 066/067 chrome (ribbon partial, `.bld-*`, cards) and the 063/065 art.

## Acceptance criteria

- **AC1 — Command header.** A stamped header with the village name (+ capital star), a coordinate chip, and a
  sub-line of tribe / **population** / village-slots / protection; a **loyalty** dial; the multi-village
  switcher; and quick-links (Map, Rally, Market, Academy, Smithy, training, Reports, Quests, Alliance).
  World-won, wonder-site and protection notices are surfaced here. (Oases held are shown in the war-room rail
  with their reinforce/recall actions — see AC6 — not the header.)
- **AC2 — Resource ribbon.** The shared `_ribbon.html` ribbon (refactored: `VillageTemplate` uses
  `ResourceRibbon`), matching the building pages.
- **AC3 — Fortress plan.** A top-down rampart (towers + gate) with the **buildings** as plots inside, placed by
  kind (the Main Building a central keep). Each plot shows its icon, level, name; an **affordable upgrade**
  glows ember; an **under-construction** building shows a live **countdown** on the plot (true % progress isn't
  available without a build start time, so no progress bar is faked); an **empty/buildable** slot reads as
  available. Clicking a plot opens the **inspector**.
- **AC4 — Inspector.** A panel below the plan that, for the selected plot, shows its name, level → next, the
  effect, the next cost, an **Upgrade/Build** action (the `/build` form, gated by `can_order`/`at_max`), and —
  for buildings with their own page — an **Enter** link.
- **AC5 — Resource fields.** The 18 fields as a compact grid (coloured by resource, level, ember when
  upgradeable), each opening the same inspector.
- **AC6 — War-room rail.** Incoming movements, the build queue (active constructions, with countdowns),
  garrison + upkeep, oases held (with reinforce/recall), stationed/abroad reinforcements, and the
  culture/expansion stats.
- **AC7 — Behaviour preserved.** Every action (build/upgrade, oasis recall, reinforcement send-back, the
  switcher, the wonder-build), the gating (P4), the protection/world-won notices, the empty states, and the
  countdowns work exactly as before — a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — owns the page; the only role affected. Ownership + gating stay server-authoritative (P4).
- **Visitor / Moderator / Admin** — unaffected (login-gated).

## Constitution

- **P3** — pure domain untouched; web template/handler/CSS/JS only.
- **P4** — the plot is advisory; the use-case re-validates the build (slot/kind/affordability) server-side.
- **P11** — the economy is already loaded; no new queries.

## Out of scope

- A coordinate-accurate slot map (the plan is a stable, authored layout, not the world map).
