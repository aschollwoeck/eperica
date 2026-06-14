# Feature 025 — Player profiles & presence — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. Presence derives from the existing 019 `last_activity` — no new activity signal.

## Domain

- [x] **T1 — Presence rule + bio validation (`domain/presence.rs`; P3/P7).** `Presence` + `presence(last,
  now, window)`; `valid_bio` + `MAX_BIO`; `presence_online_secs` added to `LifecycleRules` + lifecycle.toml.
  **Unit tests:** online-window boundary, `valid_bio` bounds, rules load (AC3, AC1).

## Persistence & use-cases

- [x] **T2 — `users.bio` + profile read/edit (migration `0036`).** `set_bio`, `profile_of -> ProfileView`
  (name + bio + last_activity). `edit_bio`/`view_profile` use-cases + `ProfileError`. **DB tests:** bio
  set/clear round-trips; `profile_of` returns the fields; (use-case) invalid bio rejected (AC1, AC2, AC7).

## Web profile + presence freshness

- [x] **T3 — Profile page (bio + presence) + edit + touch middleware.** Public profile shows the bio + a
  presence badge; own profile gets an "Edit bio" form → `POST /profile/bio` (owner-scoped). A presence-touch
  middleware keeps `last_activity` fresh on navigation, excluding `/static`, `/messages/stream`,
  `/messages/unread`. **Integration tests:** owner edits bio → shows on profile; the edit only affects the
  actor; profile shows online vs last-seen; a navigation touches activity but the unread poll does not (AC1,
  AC2, AC5, AC6).

## Surfaces

- [x] **T4 — Presence on leaderboard, conversations & map.** `LeaderboardRow` + the conversations read carry
  `last_activity`; a shared template helper renders the presence indicator on leaderboard rows, the
  conversations list + DM header, and map markers (markers already carry it). **Tests:** the board/conversation
  reads include the activity timestamp; a presence indicator renders (AC4).

## Acceptance

- [ ] **T5 — Docs + review.** rustdoc on new public items; `docs/architecture/0027-profiles-and-presence.md`;
  `docs/manual/` profile/presence note; `CLAUDE.md` active slice → 025. Full gates + P11; `eperica-reviewer`
  until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC7** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
