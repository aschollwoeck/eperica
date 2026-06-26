# Feature 068 — the Wonder page

## Why

The last page still on the plain-panel layout is the **Wonder of the World** race page (021). Unlike the
in-village building pages, it is a **public, world-level** standings page (`WorldScope` — no village/economy),
so it takes the building-page **hero band** but **not** the resource ribbon, and its alliance-by-level table
becomes a **progress leaderboard** (each alliance a bar toward level 100, the leader highlighted), with the
victory banner foregrounded once the round is won.

Presentation only — **no domain/sim change** (P3), and no handler/struct change (the existing `WonderTemplate`
already carries everything). Reuses the 066/067 building-page chrome and the `wonder.webp` art.

## Acceptance criteria

- **AC1 — Hero band.** The page shows the `wonder.webp` art as a hero band with a monument crest, the title
  "Wonder of the World", a one-line note (the win condition, or the winner once decided), and a ← Village
  return. Graceful art fallback (no broken-image, no layout shift).
- **AC2 — Progress leaderboard.** The standings render as a ranked list: each alliance with its rank, name +
  tag, a **progress bar toward level {max}**, and its **`{level} / {max}`** readout. Rank 1 is highlighted; a
  completed (level ≥ max) Wonder is marked. The empty state ("no alliance has begun") is kept.
- **AC3 — Victory.** Once the round is won, a prominent victory banner names the winning alliance and states
  the world is frozen (the existing "The round is over" copy preserved).
- **AC4 — Public + behaviour preserved.** Still reachable by anyone in the world (visitor included); no new
  data, no rule change — a reskin only.

## Roles (see specs/roles.md)

- **Visitor / Player / Moderator / Admin** — all may read it (public, 021/058). No role-specific UI.

## Constitution

- **P3** — pure domain untouched; one template + CSS.
- **P11** — no new query (the handler is unchanged).

## Out of scope

- The **village page** (fortress plan) — its own slice.
