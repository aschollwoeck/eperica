# Feature 071 — UI fixes: console hygiene + always-show research cost

## Why

Two small front-end corrections found while exercising the redesigned pages:

1. **Console errors.** No favicon is declared, so every page triggers an automatic `/favicon.ico` request that
   404s and logs a console error; the login/register password fields also lack `autocomplete` attributes
   (a browser console hint and worse password-manager UX); and **optional building/unit art** that hasn't been
   added yet 404s on the building pages (the 065/066 graceful-fallback design covers the *visual* but the
   network 404 still clutters the console).
2. **Academy hides the research cost when a unit isn't orderable.** The Academy roster (067) only renders the
   cost under `can_order`, so a unit gated by resources or Academy level shows *only* the gate reason with **no
   cost** — inconsistent with the training/build pages, which always show the cost.

Presentation only — **no domain/sim change** (P3); the Academy gating stays server-authoritative (P4), this
just always *displays* the cost.

## Acceptance criteria

- **AC1 — Favicon.** A brand favicon (`/static/favicon.svg`) is declared on every page (base layout +
  styleguide), so the browser stops requesting `/favicon.ico` and the 404 console error is gone.
- **AC2 — Autocomplete.** The login and register inputs carry the right `autocomplete` attributes
  (`username` / `current-password` / `email` / `new-password`), clearing the console hint and helping password
  managers.
- **AC3 — Academy cost always shown.** Every non-researched unit shows its research cost + time; the
  **Research** action appears only when orderable, otherwise the gate reason — but the cost is always visible.
- **AC4 — Missing art is quiet.** A request for a missing file under `/static/buildings/` or `/static/units/`
  returns a transparent **200** (a 1×1 SVG) instead of a 404, so absent-art console errors are gone while the
  blank visual is unchanged. A genuinely missing **non-art** static file (CSS/JS) still **404s** — real errors
  are not masked — and present art keeps its normal 200/304.
- **AC5 — Behaviour preserved.** No auth/gating change; the favicon + transparent fallback are static, the
  autocomplete hints are advisory, and the Academy POST + gating are untouched.

## Roles (see specs/roles.md)

- All roles benefit (cleaner console / clearer Academy). No authority change.

## Constitution

- **P3** — no domain change. **P4** — Academy research stays server-gated; the cost is display-only.
  **P11** — no new query.

## Out of scope

- A raster/`.ico` favicon or app-manifest icons (the inline SVG covers modern browsers).
