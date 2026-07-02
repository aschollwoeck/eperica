# Feature 115 — tribe-themed primary button & mobile UX pass

**Status:** In progress
**Depends on:** 004 (tribes), 042/045 (per-world players), 052 (per-world rules), 069 (village page),
the design direction (stylish, tribe-themed, styles-only).
**Roadmap:** app-layer visual/UX pass — presentation only; no game rules.

## Goal

Make the app feel like a game: give the **primary action button** a characterful, **tribe-coloured**
"war-banner" identity, and make the existing pages **usable on mobile**. This is a **styles-only** pass —
no layout/markup changes to game logic — plus one tiny read-only data path so the theming can pick the
right tribe.

## Concepts

- **War-banner primary button.** `.btn--primary` becomes a brushed banner (SVG turbulence/displacement
  filter `static/brush.svg` frays the edges; the text stays crisp). On hover an accent brush wipes across
  and ember sparks rise; no geometry change (no resize). Progressive enhancement — without the filter it is
  a solid banner.
- **Tribe theming.** The banner's colours come from `[data-tribe]` on `<body>` (Romans wine→crimson, Gauls
  green, Teutons black-blue). Tribe is **per-world** (a player may run a different tribe per world, 042/045),
  so inside a world `<body data-tribe>` is set from the **world** tribe (`GET /w/{world}/me`); on account
  pages it falls back to the **account** tribe (`GET /me`).
- **Mobile pass.** Below the responsive breakpoints: a hamburger topbar menu; slimmer buttons; wide tables
  reflow into cards (`.table--cards`); the village fortress plan sits in grid cells (no off-screen offsets);
  the status rail / building aside stack above the plan/units; the building-page hero aligns cleanly.

## Acceptance criteria

- **AC1 — Primary button (styles-only).** `.btn--primary` renders as the war-banner on all screens with no
  markup change; hover animates (wipe + sparks) without changing the button's box size; disabled reads as
  off. Server-rendered HTML and existing render tests are unaffected.

- **AC2 — Tribe theming source (P4).** `GET /me` includes the account `tribe`; `GET /w/{world}/me` returns
  the acting player's tribe **in that world** (from their own village). `base.html` sets `data-tribe` from
  the account tribe, then overrides with the world tribe when inside a world. Both endpoints expose only the
  requester's own tribe. No world context ⇒ account tribe (or none ⇒ default theme).

- **AC3 — Per-tribe palette.** With `data-tribe` in {romans, gauls, teutons}, the primary button uses that
  tribe's palette; unknown/absent ⇒ the Roman default. Adding a tribe is overriding a few colour tokens.

- **AC4 — Mobile: no overflow.** On a phone-width viewport the topbar collapses to a hamburger, the village
  page (command header, resources, plan, rail), building/troop pages, and the world-selection list all fit
  with **no horizontal page overflow**; the world-selection Enter button is fully visible (card reflow).

- **AC5 — Desktop unchanged.** Above the mobile breakpoints the layouts are as before (tables stay tables,
  rail stays on the side, hero stays a row) — the responsive rules are scoped to the breakpoints.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | Sees the themed button + responsive layout on public pages; default (Roman) theme when logged out. | `/w/{world}/me` (world scope requires a joined player → redirected to the lobby/login). |
| **Player** | Their own account tribe (`/me`) and per-world tribe (`/w/{world}/me`) drive the theming. | Any other player's tribe/data. |
| **Moderator / Administrator** | N/A (considered) — no moderation/admin surface; superset behaviour. | — |
| **System** | *(none)* — presentation only; no background job. | — |

## Out of scope

- The **village status strip** (incoming attacks / training / culture) and the **drill-yard portrait** —
  those are their own spec'd slice (116).
- **Per-tribe stylesheets / imagery** beyond the button palette (planned as a later slice).
- Any change to game rules, forms, or server actions.

## Decisions

- **Styles-only + one read endpoint.** The visual work is pure CSS/SVG/JS; the only server addition is the
  read-only tribe lookups used to pick the palette (`/me` tribe field + `/w/{world}/me`).
- **World tribe wins inside a world.** The account tribe is a fallback; the per-world tribe is authoritative
  for the page you're on (042/045 — tribe is a per-world player property).
