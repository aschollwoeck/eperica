# Feature 027 — Alliance forum

**Status:** Reviewed
**Depends on:** 015 (alliances — membership, roles & the `Announce` right), 024 (the conversations/thread UI patterns this mirrors), 021/022 (the freeze/sanction/rate-limit guards a post reuses), 001 (auth/sessions)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Communication → "Alliance forum") — threaded boards scoped to an alliance, with per-role posting.

## Goal

Give an alliance a durable place to organise: **threaded discussion** scoped to the alliance, where any
member can start a thread and reply, and **announcements** (a one-way broadcast) can be posted by members
holding the `Announce` right. Server-authoritative (P4): every read and post is gated by alliance
membership; announcements are gated by the role right. Persisted (P2/P6).

## Concepts

- **Thread.** A titled discussion owned by one alliance, started by a member, containing a running list of
  **posts** (oldest→newest). The thread list is ordered by most-recent activity.

- **Post.** A member's message in a thread (body text, validated like a chat message — non-empty, length
  capped, no markup). Posting bumps the thread's activity.

- **Announcement thread.** A thread flagged as an announcement — a **one-way broadcast**: it can only be
  started by a member with the `Announce` right (015), and it is **locked** (no replies). Every member
  reads it. Ordinary threads are open to all members for both starting and replying.

- **Scope & access.** A forum belongs to exactly one alliance. Only its **current members** can read the
  thread list, read a thread, start a thread, or reply (P4). A thread is only reachable by members of the
  alliance that owns it — a member of another alliance (or a Visitor) cannot read or post.

## Acceptance criteria

> All access + posting is server-authoritative (P4) and reproducible from persisted rows (P2/P6). Posting
> reuses the existing mutating-action guards (021 freeze, 022 sanction + rate limit).

- **AC1 — Read the forum (members).** A member sees their alliance's thread list (most-recent activity
  first) and can open any thread to read its posts (oldest→newest). A non-member / Visitor cannot.

- **AC2 — Start a thread (members).** A member can start a thread with a title + first post. It appears in
  the list and is readable by every member.

- **AC3 — Reply (members).** A member can reply to an ordinary thread of their alliance; the post appears
  and bumps the thread's activity. Replying to a **locked** (announcement) thread is rejected.

- **AC4 — Announcements (the `Announce` right).** Starting an **announcement** thread requires the
  `Announce` right (Founder always; a Leader who holds it). A member without the right cannot start one
  (server-enforced). Announcement threads are locked to replies.

- **AC5 — Scope isolation.** A member of alliance A cannot read or post in alliance B's threads, and cannot
  read/post once they have left A (membership is checked on every action, P4).

- **AC6 — Validation.** A thread title and a post body are validated (non-empty after trim, length-capped,
  rendered as text — no markup). Invalid input is rejected.

- **AC7 — Roles.** Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — (redirected to login). | Any forum read/write. |
| **Player (alliance member)** | Read the forum; start ordinary threads; reply to ordinary threads. | Read/post in another alliance's forum; start an announcement without the `Announce` right; reply to a locked thread. |
| **Player (member with `Announce`)** | (as member) + start announcement threads. | — |
| **Player (no alliance)** | — | Any forum read/write (not a member). |
| **Moderator/Administrator** | (as their membership allows). | — |

- **AC8 — Reproducibility & config.** Threads + posts are persisted; reads recompute from rows (P1/P2).
  The thread-list + post-list page sizes are bounded (P11).

## Out of scope

- Multiple boards/sub-forums, per-board access levels, and **confederation** read access — a single
  alliance forum this slice; cross-alliance/board visibility is future work.
- Editing / deleting / moving threads & posts, pinning, and moderation tools.
- Live (SSE) updates of the forum, unread/read tracking per thread, and forum notifications (026) — future
  work; the forum is a page-load read for now.
- Rich text / attachments / reactions.
