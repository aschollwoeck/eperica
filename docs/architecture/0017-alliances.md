# Alliances & diplomacy — membership, roles/rights, stances, and the trusted-group effects

**Status:** Current
**Date:** 2026-06-13 · **Slice:** 015

## Context
Alliances are the **group layer** (GDD §10): players band together via the **Embassy**, hold an
**alliance role** with a granular **rights** set, and their alliance sets pairwise **diplomacy**
(war / confederation / neutral). Belonging to an alliance — or a **confederation** of allied alliances —
grants **shared visibility** (rosters + allied village lists) and **coordinated defence** (incoming-attack
awareness + reinforcement). This slice delivers the alliance as server-authoritative relational state and
its read-side effects; the **communication** layer (forum/chat/report-sharing, GDD §10.4) stays app-layer
and out of scope, as do alliance bonuses, war statistics (016), and the Wonder win condition (end-game).

## Design
- **The Embassy is a new ordinary building.** `BuildingKind::Embassy` rides the 003 build path verbatim
  (catalog cost/time/prereq in `construction.toml`, population in `economy.toml`); no exclusivity, any
  village. Eligibility uses the player's **highest** Embassy across villages, computed on read (P1):
  **≥ 1 to join**, **≥ 3 to found** (faithful Travian). *Note:* the enum drives `building_str`,
  `balance::parse_building`, the repo's `parse_building` (DB→enum), and the web label/id/slot/menu — all
  string-keyed, so adding a variant is a manual sweep, not a compile error.
- **Pure group rules (`domain/alliance.rs`, P3).** `AllianceRole` (Founder > Leader > Member, with
  `outranks`), `AllianceRight` + `RightSet` (a `u8` bitset persisted as an int), `has_right`
  (Founder⇒all / Member⇒none / Leader⇒granted), `can_expel` (the Expel right **and** a strictly higher
  rank), `AllianceRules` (`can_found`/`can_join`/`at_cap`), and the diplomacy state machine
  `next_stance`: **war** is unilateral & immediate and overrides any confederation; **confederation** is
  `Proposed` until the counterpart flips it `Active`; cancel → Neutral; transitions are idempotent;
  proposing while at war is rejected; exclusivity (one stance per pair). All unit-tested without I/O.
- **Relational persistence (migration 0024).** `alliances` (id, unique name + tag, founder); a single
  `alliance_members` row per player (**`player_id` PK ⇒ one alliance per player** at the DB), carrying
  `role` + a `rights` int; `alliance_invitations` keyed `(alliance, invitee)` (absence = resolved);
  `alliance_diplomacy` as a **normalised unordered pair** (`alliance_lo < alliance_hi` CHECK + composite
  PK) so **self-diplomacy and two-stances-per-pair are structurally impossible**, with `proposed_by` for a
  pending confederation. All FKs `ON DELETE CASCADE`, so **disband** is one `DELETE FROM alliances`.
- **Application use-cases (`application/alliance.rs`, P4).** Thin orchestration over the
  `AllianceRepository` port: load the actor's membership, run the **pure** role/rights/eligibility/
  diplomacy checks, then mutate — `found`, `invite`/`revoke`, `respond` (accept/decline), `leave`,
  `expel`, `set_member_role` (no privilege escalation: outrank both the target and the new role, so only
  the Founder mints Leaders), `transfer_founder`, `disband`, and `set_diplomacy` (threading `next_stance`
  + the "only the counterpart accepts" identity check). Each returns a typed `AllianceError` enforced
  before any write. The **cap** is a single guarded conditional insert (`add_member`); the **one-alliance**
  guard is the PK (→ `Duplicate`).
- **Read-side visibility & defence (P1/P4).** `alliance_view(viewer)` assembles the page — roster,
  diplomacy, allied villages, incoming attacks — scoped **structurally** to the viewer's alliance + its
  **active, one-hop** confederations (so a viewer can never see outside their set). `incoming_against`
  selects in-transit `attack`/`raid` movements whose `deliver_village` is an allied village and exposes
  **only target + ETA** — it does **not** join `movement_troops`, so the attacker's composition stays
  hidden (§7.3) until scouted/resolved (010/009). The shared village list and incoming view are plain
  queries derived at request time — no caches, no events.
- **Interface (`web`).** `/alliance` shows the found/join controls when alliance-less, else the roster +
  diplomacy + incoming-defence overview + rights-gated management; POST endpoints map `AllianceError` to a
  log + redirect (the client is never trusted). The alliance **tag** is public (§7.3): `villages_at`
  LEFT-JOINs the owner's alliance so the **map** shows each village's tag.

## Consequences
- **No new combat math and no scheduler tick.** Reinforcing allies is the existing 007 movement; the
  alliance only makes allied villages **findable** and their peril **visible**. Confederation visibility
  is **one hop** (no confederate-of-confederate) to bound the read. Attacking your own
  alliance/confederates is **not** blocked (faithful Travian; betrayal is a tactic).
- **War records a stance only.** The kill/war statistics it enables are 016. A player can be reduced to
  zero villages by conquest (014) and still hold an alliance role — membership is per-player, not
  per-village.
- All alliance state is reproducible from persisted rows (P2); the same history yields the same alliances.

## Links
specs/constitution.md (P1–P4, P7, P11); specs/features/015-alliances/; specs/balance/alliance.toml
(cap + Embassy gates), construction.toml + economy.toml (Embassy);
crates/domain/src/alliance.rs (roles, rights, eligibility, the diplomacy state machine), building.rs
(Embassy variant);
crates/application/src/alliance.rs (use-cases + alliance_view), ports.rs (AllianceRepository + DTOs);
crates/infrastructure/src/repo.rs (PgAccountRepository alliance impl, villages_at tag join),
balance.rs (alliance_rules);
crates/web/src/handlers.rs (the /alliance page + POSTs, the map tag), templates/alliance.html;
migrations/0024_alliances.sql.
