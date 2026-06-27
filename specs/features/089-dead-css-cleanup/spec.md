# Feature 089 — remove CSS orphaned by the build-flow rework (087/088)

## Why

Removing the village inspector (087) and moving the resource fields into a ring (088) left a handful of CSS
rules referenced by nothing. This deletes them so `base.css` documents only what ships (flagged in the 088
review).

Pure cleanup — no behaviour change (P3); every removed selector was confirmed to have **zero** references
across all templates, handler-generated class strings, and inline JS.

## Acceptance criteria

- **AC1 — Dead rules removed.** `.vinspect`/`.vinspect__*` (the 087-removed inspector, 9 rules), `.vplot--sel`
  (its selection state), and `.feed__ico--atk` (a feed-dot variant never emitted) are removed; a stale comment
  on the plot rule is corrected.
- **AC2 — No regression.** Every live component keeps its styling; the suite stays green.

## Constitution

- **P3** — pure presentation; CSS only. **P11** — no query.

## Out of scope

- Dynamically-composed classes (`.vplot--{kind}`, `.vfield--{res}`) — all live, kept.
