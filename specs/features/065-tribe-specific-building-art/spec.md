# Feature 065 — tribe-specific building backgrounds

## Why

063 gave every building page a darkened background plate (`/static/buildings/<slug>.webp`), one shared image
regardless of tribe. But a Roman forge, a Teuton forge, and a Gaul forge are built differently (dressed stone
vs rough timber vs carved oak), and the game already leans on tribe identity everywhere else (units got
per-tribe portraits in the 063 follow-up). This slice lets a building page show **its village's tribe** plate
when one exists, while keeping the neutral plate as the universal fallback. Art-direction prompts already
ship a per-tribe architecture descriptor (`docs/art/building-backgrounds.md`).

Presentation only — **no domain/sim change** (P3).

## Acceptance criteria

- **AC1 — Tribe plate, layered.** Each building page (Main Building, Academy, Smithy, Barracks/Stable/Workshop,
  Rally Point, Marketplace) resolves `/static/buildings/<tribe>_<slug>.webp` for the village's tribe **on top
  of** the neutral `/static/buildings/<slug>.webp`.
- **AC2 — Graceful fallback (no broken-image).** When the tribe plate is absent the layer 404s to transparent
  and the neutral plate shows through; when both are absent the plain dark theme shows — a pure-CSS cascade,
  no broken-image artifact, no layout shift (exactly the 063 fallback behaviour, one layer deeper).
- **AC3 — Tribe drives only the architecture.** The tribe is selected server-side from the village
  (`village.tribe`), never the client; an unknown/absent tribe emits no tribe layer (neutral only). The Wonder
  page stays neutral (an endgame monument, not tribe-flavoured).

## Roles (see specs/roles.md)

- **Player** — sees their village's tribe plates; the only role affected.
- **Visitor / Moderator / Admin** — unaffected.

## Constitution

- **P3** — pure domain untouched; CSS + template + a read-only `village.tribe` slug.
- **P4** — the tribe is server-resolved from the owned village, not client input.
- **P11** — at most one extra (cached, conditionally-emitted) image request per building page; the CSS layer is
  free.
