# Plan — 123 agent activity

**Status:** Draft (spec approved)

One change at one chokepoint: `bearer_account` (crates/web/src/api.rs) — the shared resolution
core of `AgentAccount` and `AgentGame` — gains a fire-and-log `touch_activity(key.user, now)`
after successful auth (identical error posture to the web presence middleware, lib.rs). The port
is already throttled (ACTIVITY_THROTTLE_MS) so cost is one small UPDATE per window per bot (P11).
No schema, no domain change. Constitution: P4 untouched (server-side, post-auth); P3 n/a.

Tests: unit-level none (glue); integration (crates/web/tests/integration.rs or api tests file):
stale `last_activity` → agent GET /api/me → users.last_activity refreshed (AC1/AC2); failed-auth
request leaves it stale (AC1); existing api tests keep passing (AC4).
