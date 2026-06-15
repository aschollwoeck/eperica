# Account sitting

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 030

## Context
Players go away; a trusted friend should be able to keep their account alive — without being handed the
keys. Account sitting lets an owner authorise sitters who can **operate the account within limits**, with
the takeover **server-enforced** (P4), **restricted**, and **audited**. No simulation change: a sitter acts
*as* the owner.

## Design
- **Effective vs real identity.** The auth layer resolves two players: the **real** logged-in human, and
  the **effective** player they act as. `effective_identity` reads the `uid` (auth) cookie for the real
  player and the optional `sit` cookie for an owner; the owner is "effective" **only** when the sit cookie
  is set *and* a live authorisation check passes. `AuthUser` returns the **effective** player — so all 54
  gameplay handlers act as the owner transparently when sitting, with no per-handler change. `RealUser`
  returns the human, used by the sitting-management handlers so a sitter always manages their *own* account.
- **Authorisation, re-checked every request (P4).** Owners grant/revoke sitters (`account_sitters`, capped
  at `MAX_SITTERS`). `authorize_sit(owner, sitter)` = the grant exists **and** the owner isn't banned/
  suspended. Because the extractor + guards re-run it per request, **revoking ends an in-progress sit
  immediately** (the next request reverts the sitter to their own account) — no server-side session state to
  invalidate.
- **Restrict + audit in one guard.** `sitting_guard` is the **innermost** middleware (runs just after the
  021/022 freeze+sanction `action_guard`, just before the handler). On a mutating `POST` made while sitting
  it refuses the owner-only set — `/settings/*`, `/profile/bio`, `/sitting/{grant,revoke,start}` — and
  records every other action to `sitter_actions` (the owner's audit log). Reads are never audited.
- **Sanctions & freeze hold through a sit.** `action_guard` now rejects when **either** the real actor or
  the effective owner is `account_blocked` (022): a banned sitter can't act (even starting a sit is a
  guarded POST), and a sanctioned owner's account can't be operated. The 021 round-freeze applies as ever.
- **Presence.** `presence_touch` (025) touches the **effective** player, so operating an account via a sit
  keeps the *owner* active (and holds off the 019 inactivity sweep) — faithful: a sat account isn't idle.
- **The banner.** A persistent "You are operating X's account — Stop" bar is injected by a small `base.html`
  script polling `GET /sitting/status` (the owner's name when validly sitting, else empty) — no per-template
  plumbing, mirroring the 026 bell.

## Persistence (migration 0041)
- `account_sitters (owner_id, sitter_id, granted_at, PK(owner_id, sitter_id))` + an index by `sitter_id`
  (the "accounts you sit for" list). The per-request authorisation is a PK probe (P11).
- `sitter_actions (id, owner_id, sitter_id, action, created_at)` + `(owner_id, created_at desc)` for the
  owner's audit log.

## Reuse / decisions
- **Effective-player in the extractor** keeps the takeover transparent and revocation correct-by-recheck;
  no impersonation/session table.
- **Two extractors** cleanly separate "play as the owner" (`AuthUser`) from "manage my own account"
  (`RealUser`); the guard separates the two action classes.
- **Reuse 022 `account_blocked`** for both identities so a sit can never bypass a sanction.
- **DB cost** is paid only by sitting sessions (the sit cookie gates the authorisation query); ordinary
  requests are unchanged.

## Consequences
- A trusted player can keep a friend's account running, within limits, fully audited — with no new gameplay
  and no new session infrastructure.
- **Out of scope (deferred):** vacation/away mode (a sim mechanic); time- or scope-limited grants; a
  live "your sitter logged in" alert (the audit log is reviewed on demand); sitter-specific anti-pushing
  beyond the existing 022 surface.
