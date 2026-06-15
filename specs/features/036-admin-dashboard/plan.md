# Feature 036 — Admin role + dashboard shell — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Slice 1 of the M9 multi-world program. Operates on the **single** active world (read-only status); world
creation/archival is slice 040. This slice is the **Administrator role + console shell**: identity/role +
I/O plumbing + presentation. **No pure-domain rules** (P3 untouched) — there is nothing to compute, only a
role to gate on and DB reads/writes to expose. So, unusually for this project, there is no domain-first
task; the first tests live in the `application` layer (the gate + use-cases against fakes).

## Layers

- **Persistence (migration 0042).** `users.is_admin boolean NOT NULL DEFAULT false` (additive to
  `is_moderator`). Surface `is_admin` on `UserRecord` (read in both `find_user_by_*` queries +
  `row_to_user`, default `false` on create).
- **Ports.** New `AdminRepository` (default no-op methods so non-admin fakes are untouched):
  `set_admin`, `admin_overview` (world row + live counts), `recent_accounts`, `admin_account` (by-id, to
  resolve search hits' roles). New `AdminOverview` / `AdminAccount` view structs.
- **Use-cases (`application::admin`).** `require_admin` (mirrors `require_moderator`); gated
  `admin_overview`, `list_accounts`, `search_accounts` (reuses the 028 `search_players` + `admin_account`),
  and `set_role` (grant/revoke Moderator/Administrator, with the self-demotion anti-lockout guard and a
  not-found check). `ElevatedRole` enum + slug parse. Tested against fakes.
- **Infra.** `AdminRepository for PgAccountRepository`: `set_admin` UPDATE; `admin_overview` (one world
  row + three `count(*)` aggregates); `recent_accounts` / `admin_account` SELECTs.
- **Web.** `bootstrap_admins` from the `ADMINS` env (mirrors `bootstrap_moderators`). `MaybeRealUser`
  extractor; `/admin` + `/admin/role` gated on `RealUser` (anti-escalation — sitting confers no admin);
  `/me` gains an `admin` flag (real-human-keyed). `admin.html` console: overview + account search/listing +
  role forms. Topbar `#nav-admin` link revealed by the `/me` probe (035 pattern).

## Key decisions

- **`RealUser`, not `AuthUser`, for admin surfaces.** Admin grants persist beyond a sit, so delegating
  admin through 030 sitting would be a privilege-escalation; the gate keys on the real human. Moderator
  visibility keeps the 035 effective-player behavior (moderation does not create persistent state).
- **Registry/worlds deferred.** No world mutation here; the overview is read-only and single-world. This
  keeps the high-risk multi-world refactor (037–039) out of the shell.

## Risks

- Adding a `UserRecord` field touches every constructor (test fakes + `create_account`); caught at compile
  time. Low risk, no behavior change.
