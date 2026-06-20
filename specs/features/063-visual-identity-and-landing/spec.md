# Feature 063 — visual identity, landing & per-building art

**Status:** Verified
**Slice type:** Presentation layer (web/templates/CSS). No domain/sim change (P3); one read-only worlds
query + a reused join on the register path.

This is the first **visual** slice after the dependency-ordered roadmap (001–046) and the per-world program
(047–053) — explicitly anticipated by CLAUDE.md ("further work, e.g. visual theming/imagery, starts as a
fresh slice"). It gives Eperica a distinctive identity and turns the bare landing into a conversion page.

## Problem

The app is functionally complete but visually templated: a flat brown theme, a one-panel landing
("Eperica / Forge a village… / Create account · Log in"), a generic unstyled top-nav, a one-line footer,
no imagery, and feature-led copy ("Faithful · Persistent · Real-time", "server-authoritative") that means
nothing to a cold visitor. There is also no way to see or pick a world before committing to an account, and
static assets are served with no `Cache-Control` (browsers cache stale CSS/HTML after edits).

## Goal & acceptance criteria

- **AC1 — A distinctive theme.** A "Grim Forged Steel" theme — cold gunmetal night, blued-steel actions,
  tarnished-brass highlights, ember/torch warmth, a global vignette + fine grain — built on the existing
  `--c-*` token system so the whole app reskins from one theme file. The original "Ash & Rust" theme file is
  retained for A/B (swap is a one-line `<link>` change).
- **AC2 — A redesigned landing.** An immersive hero whose signature is a **live "Raid inbound" war-table**
  (a real ticking impact clock — the sub-second-timing mechanic shown, not stated) with a coordinate-map
  fragment; a **fortress skyline with flickering torches, rising embers and a cold moon**; emotional
  battle-cry copy; a "Choose your battlefield" worlds section; a "Dispatches from the front" war-log; three
  benefit pillars; and a closing CTA. Responsive to one column; **`prefers-reduced-motion` freezes all
  motion**; the marketing header is decluttered (no in-game search).
- **AC3 — Worlds on the landing → register into the world.** The landing lists the **open** worlds; clicking
  one goes to `/register?world=<id>`, which preselects it (heading + hidden field) and, on success, drops
  the new account **straight into that world's village** (joining it if it isn't the home world). Registering
  without a world still lands on the lobby. Server-authoritative (P4): only a real, not-won world the registry
  runs is honoured; anything else falls back to the lobby.
- **AC4 — Header & footer.** A restyled global header (sticky bar, crest + wordmark, themed search, an ember
  "Create account" CTA, an SVG bell) with the empty nav-badge leak fixed; and a real site footer with
  **Impressum / Privacy / Terms** pages (German `§ 5 DDG` Impressum + GDPR/ToS scaffolds, clearly marked as
  operator-fill placeholders).
- **AC5 — Per-building backgrounds.** Every building page shows a **darkened building image**
  (`/static/buildings/<slug>.webp`) behind its content, with a graceful fallback to the plain theme when the
  file is absent (no broken-image artifact, no layout shift). An art-direction **prompt sheet**
  (`docs/art/building-backgrounds.md`) gives a consistent on-theme prompt per building.
- **AC7 — Per-unit roster portraits.** Each training-roster row shows a small **unit portrait thumbnail**
  (`/static/units/<tribe>_<id>.webp`, tribe-prefixed because unit ids collide across tribes), with the same
  graceful fallback as AC5 (a dark placeholder tile, no broken-image artifact). The same prompt sheet gains a
  **Troop & unit art** section (one figure portrait per unit, all tribes + wild animals).
- **AC6 — Fresh assets.** `/static` **and** the dynamic HTML send `Cache-Control: no-cache`, so a normal
  reload always reflects the latest CSS/templates (cheap 304s otherwise).

## Roles (see specs/roles.md)

- **Visitor** — the new landing, worlds list, register-into-world flow, and legal pages are all public.
- **Player** — in-game pages inherit the theme + header/footer and gain per-building backgrounds.
- **Moderator / Admin** — inherit the theme/header; no role-specific UI here (their pages restyle for free).

## Constitution

- **P3** — purely presentation + a read-only `list_worlds` and a reused `create_player_in_world` on register;
  the `domain` crate is untouched.
- **P4** — the register-into-world path validates the world server-side (real + open + run by the registry).
- **P11** — inline SVG + CSS only (no JS framework, no build step); one small vanilla countdown; building
  images budgeted (WebP, < ~250 KB, 16:9); animation respects reduced-motion.
- **P10** — the prompt sheet + dir README document the building-art workflow.

## Out of scope

- The actual **building artwork** (prompts + drop-in mechanism only; art is generated externally).
- A live, user-facing **theme switcher** UI (both theme files exist; switching is a manual `<link>` swap).
- Real **legal text** (the Impressum/Privacy/Terms are operator-fill scaffolds).
- Suppressing the harmless `404` a building page logs for a not-yet-added image (optional follow-up: scan
  `static/buildings/` at startup and only emit the background for slugs that exist).
