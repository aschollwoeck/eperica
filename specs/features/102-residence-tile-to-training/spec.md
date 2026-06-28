# Feature 102 — clicking the Residence/Palace opens its training page

## Why

After 099/101 settlers train at the Residence/Palace, but clicking the **building** (its plot on the village
plan, or a build-target link) opened the generic upgrade-only **detail** page (`/building/residence`) — with
no settler training and no link to it. Only the header quick-link reached the training page, so a player who
clicked the building "didn't see" settlers. The troop buildings (Barracks/Stable/Workshop) already route their
plot to their training page; the Residence/Palace did not.

## Acceptance criteria

- **AC1 — Building opens training.** Clicking a built **Residence** or **Palace** opens its training page
  (`/residence`) — where settlers are trained and the upgrade panel still lives — not the upgrade-only detail
  page. Consistent with the troop buildings.

## Out of scope
- The training page itself (099); settler gating (101).
