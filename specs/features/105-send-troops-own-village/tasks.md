# Tasks — 105 send troops to own village

Branch `feature/105-send-troops-own-village`.

- [x] **T1**: `map_cells` — set the Rally Point `href` for every village marker (drop the `owner_name !=
  username` guard), so own villages get the "Send troops" shortcut.
- [x] **T2 — Verify**: live — own village (`--self`) now carries the rally href; enemy unchanged. Tests: the
  `/map/tiles` test asserts the own village has a `/rally?x=` href; the send-shortcut test now expects the own
  village to be a send target.
- [ ] **T3 — Reviewer + PR.**
