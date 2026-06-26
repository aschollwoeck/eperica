# Feature 077 — the communication pages redesign

## Why

The communication surfaces — **notifications**, the **messages** inbox, a **conversation** (DM chat), the
**alliance forum** thread list, and a **forum thread** — are the social group still on plain panels. This slice
brings them onto the design system (the `.phead` header + styled lists, chat bubbles, and forms).

Presentation only — **no domain/sim change** (P3), no routing/auth change (P4); the live SSE chat stream, every
POST, the world/account-scoped links, and the privacy of feeds are unchanged.

## Acceptance criteria

- **AC1 — Notifications.** `.phead` header + the feed as cards, unread rows carrying a gold accent; the alert
  links + empty state preserved.
- **AC2 — Messages inbox.** `.phead` + the conversations as clickable cards (title + presence + unread badge +
  last-message preview).
- **AC3 — Conversation.** `.phead` (title + presence) + the thread as **chat bubbles** (mine ember/right,
  theirs left) + a styled inline send form; the **live SSE append** is preserved (and matches the new bubble
  markup).
- **AC4 — Alliance forum.** `.phead` + the threads as cards (announcement badge) + the "start a thread" form;
  a **forum thread** shows its posts as bubbles + the reply form (or the locked-announcement notice).
- **AC5 — Behaviour preserved.** Every route/link/POST, the SSE stream, the announcement lock, and the
  privacy/scope of each feed work exactly as before — a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — uses all five. Alliance forum gated to members (unchanged); announcements gated to officers
  (unchanged, P4).

## Constitution

- **P3** — pure presentation; templates + CSS only. **P4** — no auth/scope change. **P11** — no new query.

## Out of scope

- The alliance overview page itself (`/alliance`) and the global chat channel UI beyond the conversation view.
