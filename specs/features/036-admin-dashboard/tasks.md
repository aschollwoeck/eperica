# Feature 036 — Admin role + dashboard shell — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. No
pure-domain rule in this slice (role/identity + I/O + presentation); the first tests are application-layer.

## Persistence & ports

- [x] **T1 — `is_admin` column + `UserRecord` (migration 0042).** `users.is_admin boolean NOT NULL DEFAULT
  false`; read it in both `find_user_by_*` queries + `row_to_user`; default `false` on create. New
  `AdminRepository` port (`set_admin`, `admin_overview`, `recent_accounts`, `admin_account`) + `AdminOverview`
  / `AdminAccount` views.

## Use-cases

- [x] **T2 — `application::admin` (gate + reads + role grants).** `require_admin`; `admin_overview` /
  `list_accounts` / `search_accounts` (reuses 028 `search_players` + `admin_account`); `set_role` with the
  self-demotion anti-lockout guard + not-found check; `ElevatedRole`. **Tests (fakes):** non-admin rejected
  (incl. missing actor); grant/revoke moderator+admin; self-demotion rejected but own-moderator allowed;
  not-found; non-admin cannot set roles; search resolves roles + is gated (AC1/AC3).

## Infrastructure

- [x] **T3 — `AdminRepository for PgAccountRepository`.** `set_admin` UPDATE; `admin_overview` (world row +
  `accounts`/`villages`/`pending_events` counts); `recent_accounts` / `admin_account` SELECTs. Covered via
  the web integration test (real DB).

## Web — identity, console, nav

- [x] **T4 — Real-human gate + `/me` admin flag.** `MaybeRealUser` extractor; `/admin` + `/admin/role`
  gated on `RealUser`; `/me` gains `admin` (real-human-keyed, moderator stays effective). `bootstrap_admins`
  from `ADMINS` env in `main.rs` (mirrors moderators). (AC1/AC2/AC5)
- [x] **T5 — Console handlers + template + routes.** `admin` (overview + recent/search listing) and
  `admin_role_submit` (gate-first, flash on self-demotion/not-found) handlers; `admin.html` (overview +
  search box + role forms); routes `/admin`, `/admin/role`; topbar `#nav-admin` link revealed by `/me`.
  (AC2/AC3/AC4/AC5)

## Acceptance

- [x] **T6 — Web integration test.** `admin_console_gates_and_manages_roles`: non-admin 403 + `/me`
  `admin:false`; after promotion the console loads, the derived counts (active accounts, villages) render
  with their real values, and `/me` reports `admin:true`; search finds any account by username and carries
  role forms; grant moderator persists; self-demotion is rejected with a flash and the role is preserved; a
  non-admin POST is 403; a sitter operating an admin's account is 403 at `/admin` and `/me` reports
  `admin:false` (anti-escalation). (AC1–AC5)
- [x] **T7 — Docs.** `.env.example` documents `ADMINS`; ADR 0034 + M9 roadmap entries; this spec/plan/tasks.
