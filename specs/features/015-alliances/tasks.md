# Feature 015 — Alliances & diplomacy — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first). Each task is a commit; gates
(`fmt`/`clippy -D warnings`/`test` + P11) pass before advancing. Alliances add **no combat math and no
scheduler tick** — the bulk is relational state with **pure** role/rights/diplomacy logic and **read-side**
visibility/defence views.

## Domain & balance

- [ ] **T1 — The Embassy building.** Add `BuildingKind::Embassy` (handle every exhaustive site the
  compiler flags: `repo::building_str`, `balance::parse_building`, `web::{building_label, building_kind_id,
  order index}`). Add `[buildings.embassy]` to `construction.toml` (time/cost/prereq: Main Building L1) +
  `BuildingsDto.embassy` + the `build_rules()` catalog pair; add the Embassy `population` row to
  `economy.toml`. Tests: the Embassy loads in the catalog with costs/prereqs; building it rides the 003
  path (a DB build reads back); no exclusivity (**AC1**).

- [ ] **T2 — Pure alliance rules (`domain/alliance.rs`).** `AllianceRole`, `AllianceRight` + `RightSet`
  (u8 bitset), `has_right` (Founder⇒all / Member⇒none / Leader⇒granted), `can_expel` (strictly higher
  rank), `AllianceRules { max_members, join/found_embassy_level }` + `can_found`/`can_join`/`at_cap`, and
  the `DiplomacyStance`/`DiplomacyStatus` + `next_stance` state machine (war unilateral & clears confed;
  confed propose→accept; cancel→neutral; exclusivity; idempotent re-declare). `alliance_rules()` loader
  + `alliance.toml` (fail-fast). Unit tests: the rights truth table, expel ranks, eligibility gates, and
  every diplomacy transition (**AC4**, **AC6**, **AC7**).

## Persistence — membership

- [ ] **T3 — Membership persistence + lifecycle.** Migration `00NN_alliances.sql` (`alliances`,
  `alliance_members` with `player_id` PK ⇒ one-per-player, `alliance_invitations`). `AllianceRepository`
  (port + `PgAllianceRepository`): `max_embassy_level`, `alliance_of`, `member_count`, `roster`, invites
  read/write, and the guarded mutations. Application use-cases: `found_alliance`, `invite_player`,
  `respond_invite` (accept/decline), `revoke_invite`, `leave_alliance`, `expel_member`, `set_member_role`
  (promote/demote + rights), `transfer_founder`, `disband_alliance` (one-tx cascade via `on delete
  cascade`). DB tests: found + duplicate name/tag/already-member/Embassy<3 rejected; invite→accept inserts
  member + deletes invite; decline/revoke; accept rejected at-cap / Embassy<1 / already-in; leave; Founder
  leave rejected; expel a lower rank (Founder/equal rejected); role/rights changes gated; disband clears
  members + invites (**AC2**, **AC3**, **AC4**, **AC5**, **AC6**, **AC12**).

## Persistence — diplomacy

- [ ] **T4 — Diplomacy persistence + state machine.** Add `alliance_diplomacy` (normalised `lo<hi` pair,
  PK, cascade) to the migration. Repo `diplomacy_of` / `confederate_alliances` / `upsert_diplomacy` /
  `delete_diplomacy`; the `set_diplomacy` use-case (`declare_war` | `propose_confederation` |
  `accept_confederation` | `cancel`) threading `next_stance` and **normalising** the pair before every
  read/write. DB tests: war is mutual + unilateral; confederation propose→accept; cancel→neutral; self /
  double-stance rejected by the constraints; declaring war clears a confederation; only a diplomacy
  rights-holder may act (**AC7**, **AC12**).

## Application — visibility & defence

- [ ] **T5 — Shared visibility + incoming-defence reads (P1/P4).** `members_villages` /
  `visible_villages(viewer, target)` gated on fellow-member-or-one-hop-confederate (else `Denied`);
  `incoming_against(village_ids)` selecting in-transit `attack`/`raid` toward allied villages —
  `{ target, coordinate, arrive_at }` only, **no** `movement_troops` join. `alliance_view(viewer)`
  assembling roster + my role/rights + diplomacy + the incoming overview, all visibility-gated. Tests: a
  member sees fellow/confederate villages + the roster; a non-member/non-confederate is `Denied`; the
  incoming list shows allied targets (target + ETA only), excludes non-allied, and drops resolved
  movements; no troop counts leak (**AC8**, **AC9**, **AC10**).

## Interface — web

- [ ] **T6 — Embassy & alliance pages + tag.** Embassy page (found / accept-decline invites / manage,
  per eligibility) and the alliance page (roster, diplomacy, incoming overview, rights-gated controls,
  Founder transfer/disband), obeying the ui-style-guide. Surface the alliance **tag** on the map/player
  view; make allied village coordinates reachable for the existing 007 reinforce. Integration tests:
  found → invite → accept → set rights → declare war / propose+accept confederation → roster + incoming
  visible; a non-member is refused the roster; the tag shows (**AC8**, **AC9**, **AC10**, **AC11**).

## Docs & acceptance

- [ ] **T7 — Technical/end-user docs.** No new scheduler tick (Embassy rides the 003 build tick;
  alliance state is request-driven). rustdoc; `docs/architecture/00NN-alliances.md`; `docs/manual/`
  alliances guide; `CLAUDE.md` active slice → 015.

- [ ] **T8 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md`/`plan.md` **Verified**, roadmap updated.
