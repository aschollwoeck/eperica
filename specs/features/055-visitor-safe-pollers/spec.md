# Feature 055 — visitor-safe background pollers (fix: landing page dumps raw HTML)

**Status:** Verified
**Type:** Bug fix. **Touches:** 024 (message unread), 026 (notifications), 030 (account sitting), 035
(auth-aware nav).

## Problem

The base template (every page) runs background JS pollers for a logged-in user: `/me`, `/messages/unread`,
`/notifications/unread`, `/sitting/status`, and an EventSource on `/notifications/stream`. Four of these are
gated by a **redirecting** auth extractor (`AuthUser`/`RealUser`), which `303`-redirects a logged-out visitor
to `/login`. `fetch` follows the redirect and receives the **full login-page HTML** (HTTP 200) instead of the
small expected body.

- **`/sitting/status`** (the visible bug): the sitting-banner JS does
  `bar.textContent = "You are operating " + name + "'s account."` with `name` = the response text. For a
  visitor that text is the **entire HTML document**, so the landing page renders a giant block of raw HTML
  markup between the topbar and the hero — exactly the reported symptom.
- **`/messages/unread`, `/notifications/unread`**: the JS `parseInt`s the body, so the login HTML silently
  becomes `0` (badge hidden) — not visible, but each poll ships 6.8 KB of login HTML every 20 s per visitor.
- **`/notifications/stream`**: the EventSource gets `text/html` instead of `text/event-stream`, logging a
  console error and a failed connection on every page for visitors.

`/me` is already correct (it uses the non-redirecting `MaybeAuthUser` and returns JSON). `/messages/stream`
is only opened on the conversation page (logged-in), so it is out of scope.

## Goal

- **AC1 — No HTML leak to visitors.** `/sitting/status`, `/messages/unread`, `/notifications/unread`, and
  `/notifications/stream` never return the login page (or any HTML redirect) to a logged-out caller. A visitor
  gets the small, correct empty/zero response, so the sitting banner and badges stay hidden and the page shows
  no stray markup.
- **AC2 — Logged-in behaviour unchanged.** For an authenticated user every poller behaves exactly as before
  (owner name when sitting; real unread counts; live notification events).

## Design

- **`sitting_status`** — `RealUser` → `MaybeRealUser`; `None` ⇒ empty `200` (the existing empty-name path).
- **`messages_unread` / `notifications_unread`** — `AuthUser` → `MaybeAuthUser`; `None` ⇒ `200 "0"`.
- **`notifications_stream`** — `AuthUser` → `MaybeAuthUser`; `None` ⇒ `204 No Content` (the SSE "do not
  reconnect" signal — no `text/html`, no console error). Authenticated path is the unchanged SSE stream.
- Routes that 030/024/026 exclude from activity-touch are unchanged.

## Out of scope

- Embedding/relocating static assets (separate latent concern: `ServeDir` uses a cwd-relative path —
  noted for a future change, not this fix). `/messages/stream` (not a base-template poller).
