# Feature 117 — cropland has its own upgrade cost (faithful Travian)

**Status:** In progress
**Depends on:** 003 (construction/build queue), 013 (field caps), 116-adjacent field-cost fix (#134).
**Roadmap:** balance-faithfulness follow-up to #134.

## Goal

Make **cropland** upgrades cost their own, cheaper-in-crop amount — faithful Travian, where the four field
types do **not** all cost the same: **woodcutter, clay pit and iron mine share one cost table**, and the
**cropland has its own** (ratio 7:9:7:2). This corrects the remaining half of the field-cost story left open
by #134 (which fixed the shared table's per-resource distribution but still applied it to croplands).

## Concepts

- A new **`crop_field_cost`** table sits beside the shared `field` cost in `BuildRules`. Croplands share the
  field's **time** and **level caps** — only the per-resource **cost** differs — so this is a cost table only.
- `BuildRules::field_cost(resource, level)` returns the cropland table for `Crop` and the shared table for
  wood/clay/iron. Every place that prices a field routes through it so the **shown cost equals the charged
  cost** (P4): the upgrade-panel display (`build_row`) and the order-time debit (`order_build`), both of which
  have the acting village and thus the field's `ResourceKind`.
- `BuildTarget::Field` is unchanged (still `{ slot }`): the build queue persists a field as `("field", slot)`
  with no resource, and cost is only ever computed where the village is in hand, so the resource is derived
  there rather than encoded in the persisted target.

## Acceptance criteria

- **AC1 — Cropland cost.** A cropland's upgrade cost uses the cropland table — a level-1 cropland is
  **70 wood / 90 clay / 70 iron / 20 crop** (ratio 7:9:7:2, wood = iron), continuing the canonical Travian
  curve to level 10 and the same ratio to the capital cap (level 20).

- **AC2 — Other fields unchanged.** Woodcutter, clay pit and iron mine keep the **shared** table (level-1
  40/100/50/60, #134). All three are identical to each other (faithful Travian).

- **AC3 — Shown = charged (P4).** The cost on the field's upgrade panel equals the cost the server debits on
  order — both derive from `field_cost(field.resource, level)`. A cropland never displays one price and
  charges another.

- **AC4 — Shared time & caps.** Croplands use the same build **time** and the same normal/capital **level
  caps** as the other fields; only the cost differs.

- **AC5 — Both presets.** `classic` and `speed` both carry `[field.crop_cost]`; the table runs to the level-20
  capital cap. Cost is speed-independent (identical across presets); only time scales (P7).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Player** | Upgrade their own croplands at the cropland cost (AC1–AC4). | Any other player's village (existing 003/P4 ownership checks). |
| **Visitor / Moderator / Administrator** | N/A (considered) — no field-cost surface of their own. | — |
| **System** | *(none)* — pure balance data + read-time pricing (P1). | — |

## Out of scope

- Differentiating **woodcutter / clay pit / iron mine** from *each other* — in Travian they are identical;
  only the cropland differs.
- Any change to field **time**, **level caps**, or **production**.

## Decisions

- **Only cropland differs.** Confirmed faithful Travian: the three non-crop fields share one table; the
  cropland is its own (cheaper crop). This is the authoritative game behaviour, not a simplification.
- **Derive the resource at pricing time, don't persist it.** Keeps `BuildTarget`/the queue schema unchanged;
  the two pricing sites (display + debit) both already hold the village.
