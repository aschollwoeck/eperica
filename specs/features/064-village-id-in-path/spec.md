# Feature 064 — village id in the URL path

## Why

Slice 056 moved the world out of a hidden cookie and into the URL (`/w/{world}/…`). The **acting village**,
though, was still carried as an orthogonal `?village=<id>` **query parameter**, and the id was rendered as a
bare 38-digit **decimal**. That is the same "essential navigation state riding along invisibly" smell 056
removed: the URL doesn't read as a clean, shareable address for *this village's* barracks. This slice finishes
the job — the village id becomes a first-class **path segment**, in the same **hyphenated-UUID** form as the
world segment, with the building as the trailing segment.

```
before:  /w/<world>/village/troops/barracks?village=37317251661476117285173572729594273487
after:   /w/<world>/village/1c130a8e-d107-4def-aaa1-9d63979d5ecf/barracks
```

Presentation/routing only — **no domain or sim change** (P3). Server-side ownership re-validation is unchanged
(P4): the path id selects *among the player's own villages*; a foreign or unparseable id falls back to the
capital.

## Acceptance criteria

- **AC1 — Village in the path.** Every village-coupled page lives under `/w/{world}/village/{village}/…`, where
  `{village}` is the village's **hyphenated UUID**. The building is the trailing segment:
  `/village/{v}/academy`, `/village/{v}/smithy`, `/village/{v}/rally`, `/village/{v}/market`, and the three
  training pages **`/village/{v}/barracks`**, **`/village/{v}/stable`**, **`/village/{v}/workshop`** (the old
  `/village/troops/{building}` segment is gone). The overview is `/village/{v}`.
- **AC2 — POSTs carry the village in the path too.** `build`, `academy/research`, `smithy/upgrade`, `train`,
  `rally/send`, `rally/return`, `oasis/recall`, `market/send` all live under `/village/{village}/…`; their
  forms **drop the hidden `village` input** — the path is the sole carrier. After the action the PRG redirect
  returns to the same village's path.
- **AC3 — No `?village=` anywhere.** No link, form action, or redirect emits `?village=`. Orthogonal target
  query params unrelated to identity (`x`, `y`, `host` on rally/market) are unaffected.
- **AC4 — Canonical entry + graceful fallback.** `/w/{world}/village` (no id) **303-redirects** to the
  player's **capital** (or first) village's canonical path, so the nav "Village" link and old bare links land
  somewhere valid. A syntactically bad or non-owned `{village}` resolves to the capital (no error, P4).
- **AC5 — Switcher & cross-links.** The multi-village switcher, the village's building links (Academy/Smithy/
  Rally/Market/Barracks/…), the map's "Reinforce", oasis "Reinforce/Recall", and the settle nudge all point at
  the new `/village/{v}/…` paths.

## Roles (see specs/roles.md)

- **Player** — owns the village-coupled pages; the only role whose URLs change. Ownership stays
  server-authoritative (P4) — the path id never grants access to a village the player doesn't own.
- **Visitor / Moderator / Admin** — unaffected (no village-coupled pages).

## Constitution

- **P3** — pure domain untouched; this is web routing only.
- **P4** — the path id is *advisory*: the use-case still resolves the village within the authenticated
  player's own set and falls back to the capital. No new trust in client input.
- **P7/P11** — no timing or perf change (same queries; one extra UUID parse per request).

## Out of scope

- Short/sequential per-world village ids (would need a new column + backfill) — the UUID is reused as-is.
- Any non-village route (`/map`, `/leaderboard`, `/reports`, `/alliance`, …) — those keep their 056 shape.
