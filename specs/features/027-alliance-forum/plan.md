# Feature 027 — Alliance forum — Plan

**Spec:** ./spec.md · **Status:** Verified

An alliance-scoped threaded forum mirroring the 024 conversations UI. Threads + posts, member-gated reads
and writes (P4), announcements gated by the 015 `Announce` right + locked. Persisted (P2), reuses the
mutating-action guards.

## Domain (pure, P3) — `crates/domain/src/forum.rs`

- `MAX_THREAD_TITLE` + `valid_thread_title(&str) -> bool` (non-empty after trim, ≤ cap). Post bodies reuse
  the 024 `valid_body` / `MAX_MESSAGE` (same chat-message rules). No I/O.

## Persistence (migration `0038`)

- `alliance_threads (id uuid pk, world_id uuid, alliance_id uuid → alliances on delete cascade,
  author_id uuid → users, title text, announcement bool not null default false,
  created_at timestamptz, last_post_at timestamptz)`.
- `alliance_posts (id uuid pk, world_id uuid, thread_id uuid → alliance_threads on delete cascade,
  author_id uuid → users, body text, created_at timestamptz)`.
- Indexes: threads `(alliance_id, last_post_at desc)`; posts `(thread_id, created_at)`.

## Application (ports + use-cases)

- Extend `AllianceRepository` (default no-ops):
  - `create_thread(alliance, author, title, announcement, now) -> u128` (also seeds the first post + sets
    `last_post_at`).
  - `list_threads(alliance, limit) -> Vec<ThreadSummary>` (`{ id, title, author_name, announcement,
    post_count, last_post_ms }`, most-recent first).
  - `thread_head(thread) -> Option<ThreadHead>` (`{ alliance, title, announcement }`) — access + lock check.
  - `add_post(thread, author, body, now) -> u128` (bumps `last_post_at`).
  - `list_posts(thread, limit) -> Vec<ForumPost>` (`{ author_name, body, created_ms }`, oldest→newest).
- `crates/application/src/forum.rs`:
  - `list_forum(repo, viewer)` — member-gated (`alliance_of`), returns the thread list.
  - `open_thread(repo, viewer, thread)` — member-gated **and** the thread's alliance must equal the viewer's
    (AC5); returns head + posts.
  - `start_thread(repo, viewer, title, announcement)` — member; an announcement requires `has_right(.., Announce)`.
  - `reply(repo, viewer, thread, body)` — member; thread in viewer's alliance; rejected if the thread is an
    announcement (locked).
  - `ForumError` (NotAMember, MissingRight, NotFound, Locked, Invalid, Backend).

## Web (`crates/web`)

- Routes: `GET /alliance/forum` (list + new-thread form), `POST /alliance/forum/new`,
  `GET /alliance/forum/{id}` (thread + posts + reply form), `POST /alliance/forum/{id}/reply`.
- Templates `forum.html` (thread list; a "new thread" form with an "announcement" checkbox shown only when
  the viewer holds `Announce`) and `forum_thread.html` (posts oldest→newest; a reply form unless locked),
  mirroring `messages.html` / `conversation.html`.
- A **Forum** link in the alliance page (`alliance.html`), shown to members.
- Posts/threads are mutating `POST`s ⇒ the existing 021 freeze + 022 sanction + rate-limit guards apply
  unchanged.

## Reuse / decisions

- **Mirror 024 conversations** — the list + thread + post-form shapes are the same; no new live infra (the
  forum is a page read this slice).
- **Reuse the `Announce` right** for announcements (faithful: alliance announcements are exactly that) — no
  new right, no rights-system churn.
- **Member-gated by construction** — every use-case loads the viewer's `alliance_of` and compares the
  thread's owning alliance, so cross-alliance access is impossible (P4); leaving the alliance immediately
  revokes access (membership is read per action).

## Risks / testing

- **Domain tests:** `valid_thread_title` bounds.
- **DB tests:** create thread (+ first post) → `list_threads`/`list_posts` reflect it; `add_post` bumps
  `last_post_at`; `thread_head` returns the owning alliance + announcement flag.
- **Application tests (fakes):** non-member rejected (read + write); announcement requires `Announce`;
  reply to a locked thread rejected; a thread of another alliance is NotFound for a viewer; invalid title/
  body rejected.
- **Web tests:** a member starts a thread + replies and sees them; a non-member gets 403/redirect; the
  announcement checkbox/right is enforced server-side (a forged announcement post without the right is
  rejected); a second alliance's member cannot open the thread.
- **Performance (P11):** thread list + post list are single bounded, index-backed queries; `post_count`
  via an aggregate bounded by the page.
