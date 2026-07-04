# Feature 123 — agent API activity counts as player activity

**Status:** Verified (gates green — 669 workspace tests; in-loop acceptance review — reviewer agents unavailable: spend limit)
**Depends on:** 118 (Agent API auth), 019 (lifecycle/`last_activity`), 027 (presence touch).
**Origin:** documentation accuracy review of PR #141 — playing bots visibly grey as "(inactive)"
on the map because nothing on the `/api` path ever touches `last_activity`; only the abandonment
sweep spares them (120 AC6). A bot that plays around the clock being labeled inactive — and
thereby becoming doctrine raid-bait for *other* bots — is observably wrong.

## Goal

An AI account that plays through the Agent API is **active** in exactly the sense a browser player
is: authenticated agent requests refresh the account's `last_activity` (throttled, like the web
presence touch), so an enabled, playing bot never shows the derived greyed/"(inactive)" map state.

## Acceptance criteria

- **AC1 — Touch at the auth chokepoint.** Every successfully bearer-authenticated agent request
  (both the account-scoped and world-scoped extractors — they share one resolution core) refreshes
  the account's `last_activity` via the existing **throttled** `touch_activity` port (one small
  write per throttle window at most, P11). Failed auth touches nothing.
- **AC2 — Derived state clears.** A bot whose `last_activity` is stale beyond the inactivity
  window stops being marked inactive after one authenticated agent call (integration: set
  `last_activity` old → agent `GET` → the map/lifecycle read no longer reports it inactive).
- **AC3 — Touch failures never break the request.** Like the web presence middleware, a failed
  touch is logged and the request proceeds.
- **AC4 — No behavioural change for humans or fog.** No other route, permission, or response
  changes; the touch is account-level exactly like the web path (030 semantics unchanged).

## Roles & permissions

Per [roles.md](../../roles.md): unchanged. Agent-surface only; touches the agent's own account.

## Out of scope

The 019 rules themselves (windows, sweep) — unchanged. The docs describing the old behaviour are
updated by the in-flight docs PR once this merges.
