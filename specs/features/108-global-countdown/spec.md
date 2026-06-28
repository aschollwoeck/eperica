# Feature 108 — global live countdown (fix: Marketplace/Rally Point upgrade countdown)

## Why

The build/upgrade panel (`_upgrade.html`) renders "Under construction — completes <countdown>" with a
`.countdown` span that JS ticks and reloads on completion. But that ticker JS was copied per-page, and the
**Marketplace** and **Rally Point** pages (which include the upgrade panel since 087) never got a copy — so on
those buildings the countdown sat static and the page never refreshed when the upgrade finished. (Three of the
per-page copies also had a `timer` TDZ bug if the deadline was already past.)

## Acceptance criteria

- **AC1 — One global ticker.** The `.countdown` ticker lives once in `base.html`, so every page that renders a
  countdown (build/upgrade, training, movement, …) gets it — including the Marketplace and Rally Point. The
  per-page copies are removed.
- **AC2 — Ticks + reloads.** A countdown counts down `data-deadline` and the page reloads ~1.5s after it hits
  zero (server-authoritative result), on every page — the regression case (Marketplace/Rally) included.

## Out of scope
- The build/training mechanics (unchanged); the countdown is a cosmetic client estimate (P4).
