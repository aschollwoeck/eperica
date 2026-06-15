# Feature 034 — Action error feedback

**Status:** Verified
**Depends on:** every POST action handler (003 build, 005 train/research/smithy, 007/009/010/012/013
movement, 008 trade, 015 alliance, 021 Wonder, 022 reporting, 024 messaging, 030 sitting)
**Roadmap:** app-layer UX — a usability fix on top of the **UX information** pass (031/032/033).

## Problem

When a POST action is rejected server-side (P4) — e.g. training more troops than the village can
afford — the handler logged the error and PRG-redirected back to the same page. To the player the page
just refreshed with nothing changed and **no explanation**. Every use-case already carries a precise,
user-appropriate reason in its error `Display` (e.g. "not enough resources", "not enough troops",
"a marketplace is required to trade"); it was simply discarded.

## Goal

Surface the rejection reason to the player, without changing any rule (P3/P4 untouched — this is
presentation only).

- **AC1 — Reason shown.** When an action POST is rejected, the next page shows a one-shot error banner
  carrying the use-case's reason (capitalized for display). A successful action shows nothing.
- **AC2 — No internal leakage.** A storage/backend failure (`storage error: …`) is never shown verbatim;
  it collapses to a generic "Something went wrong — please try again." (P4: don't expose internals).
- **AC3 — One-shot.** The banner appears once, on the page the redirect lands on, and does not persist on
  subsequent navigation.
- **AC4 — All actions covered.** Every action handler that previously swallowed its error
  (`tracing::warn!` + redirect) now attaches the reason: build, field/building upgrade, research, smithy
  upgrade, train, rally send (settle / scout / attack / raid / oasis-attack / reinforce / oasis-reinforce),
  return, oasis recall, market send, Wonder build, report, grant-sitter, and the alliance actions
  (found / invite / revoke / respond / leave / disband / expel / transfer / role / diplomacy), messaging.

## Design

- **Flash cookie.** The rejected handler attaches a short-lived (`Max-Age=30`), JS-readable (non-HttpOnly)
  `flash` cookie carrying the percent-encoded message on the redirect response (`with_flash`). The
  `base.html` script reads it, renders an `.alert--danger` banner, and **clears it immediately**
  (`Max-Age=0`) so it shows once (AC3). The error logging (`tracing::warn!`) is retained.
- **Message sanitization** (`user_msg`): pass through the use-case `Display` (capitalizing the first
  letter), except strings beginning with `storage error`, which become the generic message (AC2).
- **Why a cookie, not server-side flash state.** Stateless web tier (P5) — no per-session server store;
  a self-clearing cookie keeps the handler a pure function of its inputs and survives the PRG redirect.

## Out of scope

- Inline field-level validation / live form errors (the client previews from 031 already preview cost).
- Success/confirmation toasts (only failures were silent; success already reflects in the page state).
- Visual theming of the banner beyond the existing `.alert--danger` component (the later styling pass).
