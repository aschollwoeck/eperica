# Authentication & sessions

**Status:** Current
**Date:** 2026-06-10 · **Slice:** 001

## Context
Competitive multiplayer must be server-authoritative (P4) on a stateless tier that scales (P5).

## Design
- Passwords are hashed with **argon2id** (`Argon2Hasher`); only hashes are stored.
- Sessions are **encrypted cookies** (`axum-extra` `PrivateCookieJar`) holding the player id. No
  server-side session store is needed, so any instance can serve any request (P5). The cookie key
  comes from `SESSION_SECRET` (≥64 bytes) or an ephemeral dev key.
- The `AuthUser` extractor reads/decrypts the cookie; a missing/invalid cookie redirects to `/login`,
  enforcing Player-only access server-side (P4, roles.md).

## Consequences
- Stateless and simple — chosen over DB-backed `tower-sessions` for slice 001.
- Auth POSTs are intentionally slow (argon2) and are exempt from the P11 read-path latency budget.
- Rotating `SESSION_SECRET` invalidates existing cookies (users re-login); accounts/villages persist.

## Links
specs/constitution.md (P4, P5, P11); crates/web/src/auth.rs; crates/infrastructure/src/security.rs.
