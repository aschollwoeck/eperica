# Feature 030 — Account sitting

**Status:** Verified
**Depends on:** 001 (auth/sessions + the `AuthUser` extractor), 022 (sanction guard — a banned actor/owner is blocked), 029 (the settings surface a sitter must not change)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Account & meta UX → "Account sitting & vacation mode" — the *sitting* half; vacation mode stays deferred).

## Goal

Let a player authorise a **trusted other player to operate their account while they're away** — and stop
trusting them at any time — with the takeover **server-enforced** (P4), **restricted** (a sitter cannot
seize the account or change its owner-only settings), and **audited** (the owner sees what their sitters
did). No simulation change: a sitter simply acts *as* the owner within limits.

## Concepts

- **Sitter authorisation.** An owner grants named **sitters** (other players), up to a small cap, and may
  revoke any at any time. Owner-scoped: only the owner manages their own sitter list (P4).

- **Acting as the owner ("sitting").** A logged-in player who is an authorised sitter of an owner can
  **start sitting** that owner: their session then acts **as the owner** for all gameplay — the *effective*
  player is the owner, while the human remains the *actor*. **Stop sitting** returns them to their own
  account. Revoking a grant ends any in-progress sit immediately (re-checked every request).

- **Restrictions (P4).** While sitting, the sitter may play the owner's game (build, train, send troops,
  trade, message, …) but **cannot**: manage the owner's sitter list, change the owner's settings or profile,
  start a nested sit, or wield the owner's **moderator** powers (`/mod/*` — enforcement is never delegable).
  These are refused server-side (prefix-matched, so a future settings/profile endpoint is restricted by
  default), not merely hidden.

- **Audit trail.** Every mutating action a sitter takes on an owner's account is recorded (the owner, the
  sitter, what, when). The owner can review this log.

- **Sanctions interplay (022).** A banned/suspended **actor** cannot sit, and a banned/suspended **owner's**
  account cannot be operated by a sitter (the existing mutating-action guard applies to the effective +
  actor identities). The round-freeze (021) applies as ever.

## Acceptance criteria

> All authorisation, the takeover, restrictions, and audit are server-authoritative (P4) and reproducible
> from persisted rows (P2/P6).

- **AC1 — Grant / revoke sitters (owner).** An owner can add a sitter by username (not themselves; not past
  the cap; the target must exist) and revoke any. The list persists and is owner-only.

- **AC2 — Start / stop sitting (authorised sitter).** A player authorised by an owner can start sitting
  them; their session then acts as the owner for gameplay reads + writes. Stopping returns them to their own
  account. A player who is **not** an authorised sitter cannot start sitting that owner (server-enforced).

- **AC3 — Revoke ends the sit.** If an owner revokes a sitter mid-sit, the sitter immediately reverts to
  their own account (authorisation is checked on every request, not just at start).

- **AC4 — Restrictions.** While sitting, the actor cannot manage the owner's sitters, change the owner's
  settings/profile, or start a nested sit — these are refused (server-side). Ordinary gameplay proceeds as
  the owner.

- **AC5 — Audit trail.** A sitter's mutating actions on an owner's account are recorded with the actor +
  action + time, and the owner can view the log. (Reads are not audited.)

- **AC6 — Sanctions & freeze.** A banned/suspended actor cannot sit; a banned/suspended owner's account
  cannot be operated by a sitter; the 021 freeze still blocks all mutating actions.

- **AC7 — Roles.** Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — (redirected to login). | Anything. |
| **Player (owner)** | Grant/revoke their own sitters; view their own audit log. | Manage another player's sitters; grant themselves. |
| **Player (sitter)** | Start/stop sitting an owner who authorised them; play as that owner within limits. | Sit an owner who didn't authorise them; change the owner's settings/profile/sitters; use the owner's moderator powers; nest sits; act for a banned owner. |
| **Moderator/Administrator** | (as Player). | — |

- **AC8 — Reproducibility & config.** The sitter list + audit log are persisted; the takeover is derived
  from the session + a per-request authorisation check (P1/P2). The sitter cap is a bounded constant (P11).

## Out of scope

- **Vacation / away mode** (attack immunity / away status) — a simulation mechanic, still deferred.
- Time-limited or scope-limited grants (e.g. "build only"), and a configurable max-concurrent-sitters-online.
- Sitter-specific anti-pushing rules beyond the existing 022 trade/sanction surface.
- Notifying the owner live when a sitter logs in (the audit log is reviewed on demand).
