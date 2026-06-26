# Feature 070 — live-ticking resource counters

## Why

The resource ribbon (067) shows a static snapshot of stored resources. Travian-style games make the stores
feel alive by **counting up** between page loads at the production rate. This slice adds that purely as a
**client-side visual estimate**: from the amounts + rates + caps the ribbon already carries, the displayed
numbers tick up (and the gauge fills grow) since page load — **with no server impact whatsoever**.

The server stays the sole authority (P4): the estimate is never read back, never posted, and the true amounts
are recomputed server-side on every request/action (P1 lazy evaluation). A reload re-syncs to the truth.

## Acceptance criteria

- **AC1 — Live count.** On any page with the resource ribbon, each resource number increases over time at its
  shown hourly rate (estimated from page-load time), and its gauge fill grows to match — a smooth, continuous
  illusion, no reload needed.
- **AC2 — Bounds.** A resource is clamped to its storage cap (warehouse / granary) — it never displays past
  full — and **crop counts *down*** when its net rate is negative (starving), floored at 0.
- **AC3 — Purely cosmetic.** No new server data, no request, no write; the initial render still shows the
  server's exact amounts (the ticking only starts after load), and a reload snaps back to the authoritative
  values. Disabling JS leaves the static (correct) snapshot.
- **AC4 — Everywhere.** Because it lives in the shared `_ribbon.html` partial, it applies to the village page
  and every building page uniformly.

## Roles (see specs/roles.md)

- **Player** — sees the effect. No role-specific behaviour; no authority change.

## Constitution

- **P1/P4** — server stays authoritative and lazy; the client estimate is advisory eye-candy, re-synced on
  every load. **P3** — no domain change. **P11** — no new query or data; reuses the ribbon's existing fields.

## Out of scope

- Persisting or reconciling the estimate against a server timestamp (page-load-relative is enough for a gimmick).
