# Feature 045 — Player multi-world UX — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving in
the home world — the existing suite is the regression oracle. No pure-domain task.

## Read correctness

- [ ] **T1 — Re-point the 13 per-row cross-player name reads through `players`.** Rewrite the affected `JOIN
  users u ON u.id = <game id>` reads in `repo.rs` (map owners, `reinforcements_at`/`_of`, battle-report
  attacker/defender, oases, alliance members + member villages, invitations, forum thread/post authors,
  scout scouter/target) to `JOIN players p ON p.id = <game id> JOIN users u ON u.id = p.user_id`. DB test: a
  second-world player's name resolves. Suite green (home parity). (AC1/AC5)

## Lobby + join + switch

- [ ] **T2 — World lobby, join flow, switcher, nav.** `GET /worlds` (joined + joinable lists), `POST
  /worlds/join` (`create_player_in_world` + select + redirect, server-authoritative + idempotent), nav link
  + current-world label; switching reuses `POST /world/select`. (AC2/AC3/AC4)

## Acceptance

- [ ] **T3 — End-to-end multi-world loop + regression.** Integration: from the lobby, join a 2nd world →
  land in its village → its name resolves (re-pointed reads) → switch back home. Full suite green (home
  behaviour, names, public pages unchanged). Spec/plan/tasks. (AC5)
