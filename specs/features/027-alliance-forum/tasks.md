# Feature 027 — Alliance forum — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. Mirrors the 024 conversations UI; reuses the 015 `Announce` right + the mutating-action guards.

## Domain

- [x] **T1 — Thread-title validation (`domain/forum.rs`; P3).** `MAX_THREAD_TITLE` + `valid_thread_title`
  (non-empty after trim, ≤ cap). **Unit tests:** empty/blank rejected, cap boundary (AC6).

## Persistence & ports

- [x] **T2 — Forum tables + repository (migration `0038`).** `alliance_threads` + `alliance_posts`.
  Extend `AllianceRepository` (default no-ops): `create_thread`, `list_threads`, `thread_head`, `add_post`,
  `list_posts`; `ThreadSummary` / `ThreadHead` / `ForumPost` in ports. **DB tests:** create thread (+first
  post) → list reflects it; `add_post` bumps `last_post_at`; `thread_head` returns owner + announcement
  (AC1–AC3, AC8).

## Use-cases

- [x] **T3 — Forum use-cases (`application/src/forum.rs`).** `list_forum`, `open_thread`, `start_thread`,
  `reply`; `ForumError`. Member-gated; announcement requires `Announce`; locked threads reject replies;
  cross-alliance access is NotFound. **Tests (fakes):** non-member rejected; announcement right enforced;
  locked reply rejected; other-alliance thread NotFound; invalid title/body rejected (AC1–AC7).

## Web

- [x] **T4 — Forum pages + alliance link.** Routes `GET /alliance/forum`, `POST /alliance/forum/new`,
  `GET /alliance/forum/{id}`, `POST /alliance/forum/{id}/reply`; `forum.html` + `forum_thread.html`
  (announcement checkbox shown only with `Announce`; reply form hidden when locked); a Forum link on the
  alliance page. **Integration tests:** a member starts a thread + replies → both show; a non-member is
  refused; a forged announcement without the right is rejected; another alliance's member cannot open the
  thread (AC1–AC7).

## Acceptance

- [x] **T5 — Docs + review.** rustdoc on new public items; `docs/architecture/0029-alliance-forum.md`;
  `docs/manual/` forum note; `CLAUDE.md` active slice → 027. Full gates + P11; `eperica-reviewer` until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
