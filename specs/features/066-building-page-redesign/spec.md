# Feature 066 — building-page redesign (Smithy first)

## Why

The building pages are functional tables on a faintly-tinted background. The design work (mockups) settled on a
distinctive **building-page pattern**: a full-bleed **hero band** of the building's art with the title/crest
overlaid, the shared **resource ribbon**, a building-specific **main panel**, and a focused **aside**. This
slice implements that pattern for the **Smithy** — the unit-upgrade table becomes a **forge armoury roster**
(per-unit forge level with a pip track to the smithy's cap, the stat gain, cost, an ember "affordable" cue, the
unit currently at the anvil highlighted) with an aside for the live upgrade + the forge.

Presentation only — **no domain/sim change** (P3). It reuses the existing art wiring (063 unit thumbnails,
065 per-tribe building backgrounds) and establishes the shared building-page chrome that the other building
pages (and the village page) will adopt next.

## Acceptance criteria

- **AC1 — Hero band.** The Smithy page shows its building art (`<tribe>_smithy.webp`, else `smithy.webp`) as a
  **hero band** across the top, with an anvil crest, the title "Smithy", its **level**, a one-line note, and a
  **← Village** return — overlaid with a scrim so the text stays legible. Graceful fallback to the dark theme
  when no art (no broken-image, no layout shift).
- **AC2 — Resource ribbon.** A four-gauge resource band (current / capacity, production rate, and a
  fill bar) is shown, fed by the live economy (`load_economy` rates + capacities) — the same numbers the
  village page shows. (Enables the ribbon for every page that loads `village_view_data`.)
- **AC3 — Forge armoury roster.** Each researched unit is a row with: its **portrait** thumbnail
  (`/static/units/<tribe>_<id>.webp`, graceful fallback), name + role, a **forge-level pip track** (current
  level filled, the smithy's cap as the track length), the stat gain, the upgrade cost, the forge time, and an
  **Upgrade** action — or the reason it's gated (cap reached / smithy too low / insufficient resources / in
  progress). An **affordable** row carries an ember cue; the unit **currently upgrading** is highlighted.
- **AC4 — Aside.** A side panel shows **At the anvil** (the one-at-a-time upgrade in progress with a live
  countdown, or a cold-anvil empty state) and **The Forge** (the smithy level / cap and a link to raise it on
  the village).
- **AC5 — Behaviour preserved.** The upgrade POST, gating (P4: cap / smithy level / affordability
  re-validated server-side), the "build a Smithy first" requirement notice, and the active-upgrade countdown
  all work exactly as before — this is a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — owns the page; the only role affected.
- **Visitor / Moderator / Admin** — unaffected (login-gated, P4).

## Constitution

- **P3** — pure domain untouched; web template/handler/CSS only.
- **P4** — every gate (cap, smithy level, affordability) stays server-authoritative; the page only *shows*
  state.
- **P11** — one extra economy read already performed by `village_view_data`; no new queries on the hot path.

## Out of scope

- The other building pages (Academy/Barracks/Stable/Workshop/Rally/Market) and the village page — they adopt
  the same shared chrome in follow-up slices.
- A mobile-tailored layout beyond a sensible responsive stack.
