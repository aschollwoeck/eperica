# Alliance forum

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 027

## Context
An alliance needs a durable place to organise beyond live chat: **threaded discussion** scoped to the
alliance, where members start threads and reply, and leaders can post **announcements**. Server-authoritative
(P4): every read and post is gated by membership; announcements by a role right. Persisted (P2/P6).

## Design
- **Threads + posts, alliance-scoped.** A thread is a titled discussion owned by one alliance, started by a
  member, holding a running list of posts (oldest→newest). The thread list is ordered by most-recent
  activity (`last_post_at`), bumped by each post.
- **Membership-gated by construction (P4).** Every use-case (`list_forum`, `open_thread`, `start_thread`,
  `reply`) first loads the actor's `alliance_of(...)`. A thread is only reachable when its owning alliance
  equals the actor's — a cross-alliance read/post resolves to `NotFound` (no existence leak), and leaving
  the alliance revokes access immediately (membership is re-read per action).
- **Announcements reuse the 015 `Announce` right.** A thread carries an `announcement` flag. Starting one
  requires `has_right(role, rights, Announce)` (Founder always; a Leader who holds it). Announcement threads
  are **locked**: `reply` rejects them. This is a faithful "alliance announcement" with no new right and no
  rights-system churn.
- **Validation in the pure crate (P3).** `domain::valid_thread_title` (non-empty, capped); post bodies
  reuse the 024 chat rules (`valid_body` / `MAX_MESSAGE`). Rendered as text (Askama auto-escape).
- **Mirrors 024 conversations.** The thread-list + thread + post-form shapes mirror `messages.html` /
  `conversation.html`. No new live infrastructure — the forum is a page-load read this slice (SSE/unread are
  future work).
- **Reuses the mutating-action guards.** Posting is an ordinary `POST`, so the 021 round-freeze, 022
  sanction, and rate-limit middleware all apply unchanged.

## Persistence (migration 0038)
- `alliance_threads (id, world_id, alliance_id → alliances ON DELETE CASCADE, author_id, title,
  announcement, created_at, last_post_at)` — index `(alliance_id, last_post_at DESC)` for the list.
- `alliance_posts (id, world_id, thread_id → alliance_threads ON DELETE CASCADE, author_id, body,
  created_at)` — index `(thread_id, created_at)` for a thread's posts. Disbanding an alliance cascades away
  its forum.

## Reuse / decisions
- **`Announce` right for announcements** — faithful, zero new rights.
- **`NotFound` for cross-alliance access** — uniform with "doesn't exist"; never reveals another alliance's
  thread.
- **First post written with the thread** — `create_thread` seeds the opening post in one transaction, so a
  thread always has content and `last_post_at` is meaningful from creation.

## Consequences
- A durable alliance discussion surface gated entirely server-side, with bounded, index-backed reads (P11).
- **Out of scope (deferred):** multiple boards / per-board access levels, **confederation** read access,
  edit/delete/move/pin & moderation, live SSE updates + per-thread unread, forum notifications (026), and
  rich text/attachments.
