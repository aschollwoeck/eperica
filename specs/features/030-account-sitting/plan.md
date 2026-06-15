# Feature 030 — Account sitting — Plan

**Spec:** ./spec.md · **Status:** Reviewed

A player operates a trusted owner's account within limits, audited. The takeover is an **effective-player**
resolution in the auth layer, so existing gameplay handlers act as the owner unchanged. Server-enforced
(P4), persisted (P2), bounded (P11).

## Domain (pure, P3) — `crates/domain/src/sitter.rs`

- `MAX_SITTERS` (cap per owner) + `can_grant_sitter(owner, target, current_count) -> bool` (target ≠ owner,
  under cap). Pure; unit-tested.

## Persistence (migration `0041`)

- `account_sitters (owner_id uuid → users on delete cascade, sitter_id uuid → users on delete cascade,
  granted_at, PRIMARY KEY (owner_id, sitter_id))`.
- `sitter_actions (id uuid pk, owner_id uuid → users on delete cascade, sitter_id uuid → users on delete
  cascade, action text, created_at)` + index `(owner_id, created_at desc)`.

## Application (ports + use-cases)

- `AccountRepository` (default no-ops): `grant_sitter` / `revoke_sitter` / `is_sitter(owner,sitter)->bool`
  / `count_sitters(owner)` / `sitters_of(owner)->Vec<PlayerHit>` / `sitting_for(sitter)->Vec<PlayerHit>` /
  `log_sitter_action(owner,sitter,action,now)` / `sitter_actions(owner,limit)->Vec<SitterActionView>`.
  `SitterActionView { sitter_name, action, created_ms }`.
- `crates/application/src/sitting.rs`:
  - `grant_sitter(accounts, owner, sitter_username)` — resolve username→id, `can_grant_sitter` (≠ self,
    under cap via `count_sitters`), persist. `revoke_sitter(accounts, owner, sitter)`.
  - `list_sitters` / `list_sitting_for` / `sitter_log`.
  - `authorize_sit(accounts, owner, sitter, now) -> bool` — `is_sitter` **and** the owner is not
    banned/suspended (`account_blocked`), so a sit can't operate a sanctioned account.
  - `record_sitter_action(accounts, owner, sitter, action, now)`.
  - `SittingError` (NotFound, SelfSit, AtCap, NotAuthorized, Backend).

## Web — identity (`crates/web/src/auth.rs`, `lib.rs`)

- `SIT_COOKIE = "sit"` (encrypted) holds the owner being sat. Helper `effective_identity(parts, state) ->
  (real: PlayerId, sitting_owner: Option<PlayerId>)`: `real` = `AUTH_COOKIE`; `sitting_owner = Some(A)` iff
  `SIT_COOKIE = A` **and** `authorize_sit(A, real)`.
- **`AuthUser(PlayerId)` = the effective player** (`sitting_owner.unwrap_or(real)`) — every existing
  gameplay handler now acts as the owner when sitting, unchanged.
- New **`RealUser(PlayerId)`** = the logged-in human (`AUTH_COOKIE` only) — for the sitting page + the
  grant/revoke/start/stop/status handlers (they always act on the real account).
- `action_guard` (022/021): reject if **either** the real or effective player is `account_blocked`
  (a banned sitter can't act; a banned owner can't be operated), plus the existing freeze.
- `sitting_guard` (new middleware, mutating `POST`): when actively sitting (`sitting_owner.is_some()`):
  - refuse (403) the restricted set — `/settings/*`, `/profile/bio`, `/sitting/grant|revoke|start`;
  - else `record_sitter_action(owner, real, "<METHOD path>")` then proceed.
- `presence_touch` (025): touch the **effective** player (a sat account counts as active).

## Web — pages (`crates/web`)

- `GET /sitting` (RealUser): "Your sitters" (+ grant form / revoke), "Accounts you sit for" (+ Sit
  buttons), "Recent sitter activity" (the owner's audit log). A notice if currently sitting.
- `POST /sitting/grant` / `POST /sitting/revoke` (RealUser, owner-scoped).
- `POST /sitting/start` (RealUser): authorise then set `SIT_COOKIE`; redirect to `/village`.
- `POST /sitting/stop` (RealUser): clear `SIT_COOKIE`; redirect to `/sitting`.
- `GET /sitting/status` (background poll, excluded from presence-touch): the owner's name when sitting,
  else empty — drives a persistent **"You are sitting for X — Stop"** banner injected by a small `base.html`
  script (no per-template plumbing). A **Sitting** nav link.

## Reuse / decisions

- **Effective-player in the extractor** — keeps all 54 `AuthUser` gameplay call sites unchanged; the
  takeover is transparent and revocation is enforced per request (re-checked, so a revoke ends the sit).
- **Two identities, two extractors** — `AuthUser` (effective, gameplay) vs `RealUser` (human, sitting
  management); the guard separates "play as owner" from "manage my account".
- **Restrict + audit in one middleware** — the restricted set is refused, everything else logged; reads
  are never audited.
- **Reuse 022 `account_blocked`** for both actor + owner so sanctions/freeze hold through a sit.

## Risks / testing

- **Domain tests:** `can_grant_sitter` (self, cap boundary).
- **DB tests:** grant/revoke/is_sitter/count round-trip; `sitter_actions` log + ordered read.
- **Application tests (fakes):** grant rejects self/over-cap/unknown user; `authorize_sit` false for
  non-sitter and for a blocked owner; revoke removes authorisation.
- **Web/integration tests (the critical ones):**
  - an authorised sitter starts sitting → `/village` shows the **owner's** village; stop → back to own;
  - a non-authorised player cannot start (403/redirect, no takeover);
  - revoking mid-sit reverts the sitter to their own account on the next request (AC3);
  - while sitting, `/settings/notifications` + `/profile/bio` + `/sitting/grant` are **refused** (AC4), but
    a normal action (e.g. a build) runs as the owner and is **audited** (owner's log shows it, AC5);
  - the owner's audit log lists the sitter's action; reads aren't logged;
  - a banned owner cannot be operated; a banned sitter cannot sit (AC6).
- **Performance (P11):** the per-request authorisation is one PK probe (only when `SIT_COOKIE` is set);
  lists + log are bounded.
