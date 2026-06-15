# Feature 036 — Admin role + dashboard shell

**Status:** Draft
**Depends on:** 022 (Moderator role, sanctions), 035 (auth-aware nav)
**Roadmap:** M9 multi-world & administration, slice 1 of 6 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** This slice introduces the **Administrator** role and the console *shell*. It operates on
the single active world (read-only status); creating/archiving worlds is slice 040, after the multi-world
refactor (037–039). It is independently valuable as the admin/account-management foundation.

## Goal

Implement the **Administrator (Operator)** role from `roles.md` and a gated `/admin` console for
operational tasks that previously required shell/DB access (role grants) or were impossible in-app
(server status).

- **AC1 — Administrator role.** `users.is_admin` (additive to Player/Moderator). A `require_admin` gate
  mirrors `require_moderator` (022). At startup, the `ADMINS` env var (comma-separated usernames,
  mirroring `MODERATORS`) idempotently grants the role. All gating is server-authoritative (P4).
- **AC2 — Gated console.** `GET /admin` is reachable only by an administrator; a non-admin player gets
  403, a visitor is redirected to `/login`. The gate keys on the **real** human (`RealUser`), so admin
  capability is **never delegated through a 030 sit** — a sitter operating an admin's account cannot reach
  `/admin` (an admin grant would otherwise persist beyond the sit).
- **AC3 — Role administration.** From the console an admin can **promote/demote the Moderator and
  Administrator roles** of any account, in-app (no longer env-only). A `?q=` box searches **any** account
  by username (reusing the 028 player search) and surfaces its role forms — not only the recent listing.
  An admin **cannot remove their own** Administrator role (anti-lockout); the `ADMINS` env re-grants on
  restart regardless. Sanctions reuse the existing 022 account-inspect page (linked, not duplicated).
- **AC4 — Read-only world/server status.** The console shows the active world's configuration (speed,
  radius, seed, created-at, artifact/Wonder release schedule, win state) and live counts (accounts,
  villages, pending scheduled events) — all derived from the DB on read (P1/P5).
- **AC5 — Navigation.** An administrator sees an **Admin** link in the topbar (the 035 `/me` probe gains
  an `admin` field; `base.html` reveals `#nav-admin`), matching the server-side gate.

## Design

- **Role storage & gate.** `is_admin boolean NOT NULL DEFAULT false` on `users` (migration 0042), surfaced
  on `UserRecord`. A new `application::admin` module provides `require_admin`, the gated role-setting
  use-case (with the self-demotion guard), the account search/listing, and the gated console reads.
  Promotion reuses the existing `set_moderator`; a new `set_admin` repo method mirrors it.
- **Real-human gate (anti-escalation).** The web admin surfaces (`/admin`, `/admin/role`) take the
  `RealUser` extractor, and the `/me` `admin` flag follows the real human (a new `MaybeRealUser`
  extractor) — so sitting an admin's account confers no admin power. Moderator visibility keeps the 035
  effective-player behavior.
- **Console reads.** A small `AdminRepository` port supplies the overview counts (`accounts`, `villages`,
  `pending_events`), a recent-accounts listing, and a by-id account lookup (used to resolve search hits'
  roles), all read-only.
- **Bootstrap.** `bootstrap_admins` in `main.rs` mirrors `bootstrap_moderators` — idempotent, logs unknown
  names. Ensures there is always at least one admin via config even if all in-app admins are demoted.
- **No domain rules.** This slice is identity/role + I/O plumbing + presentation; the pure `domain` crate
  is untouched (P3). No world-config mutation yet (P7 unaffected).

## Out of scope (later M9 slices)

- Creating / configuring / starting / archiving worlds (040), and anything multi-world (037–039).
- New sanction surfaces (reuse 022's account-inspect page).
- Editing balance/config toggles from the console.
