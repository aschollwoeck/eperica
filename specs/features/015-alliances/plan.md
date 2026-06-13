# Feature 015 — Alliances & diplomacy — Technical Plan

**Status:** Draft
**Spec:** ./spec.md

Alliances are the **group layer**: players join via the **Embassy** (a new infrastructure building, built
on the 003 path), hold an **alliance role + a granular rights bitset**, and their alliance sets pairwise
**diplomacy** (war / confederation / neutral). Belonging to an alliance or a **confederation** grants
**shared visibility** (rosters + allied village lists) and **coordinated defence** (incoming-attack
awareness + reinforcement). All of it is server-authoritative relational state with **pure** role/rights
and eligibility logic in the domain; the visibility/defence views are **read-side derivations** (P1) over
existing 003/007/009/013 rows. **No new combat math, no scheduler tick.**

## Constitution check

- **P1 (event-driven / lazy):** no alliance tick and no denormalised caches. Embassy eligibility is the
  **max Embassy level computed on read** from existing building rows; the shared-visibility and
  incoming-defence views are **queries derived at request time** from current membership + `troop_movements`
  rows. The Embassy build is an ordinary 003 **due-event**.
- **P2 (reproducible):** all alliance state (members, roles/rights, invitations, diplomacy) is persisted;
  every transition is a guarded statement set; **disband** cascades in **one transaction**. The same
  history yields the same alliances. No randomness.
- **P3 (pure domain):** `alliance.rs` holds the pure rules — the rights bitset + `has_right`/`can_*`
  predicates, the `(Founder ⇒ all, Member ⇒ none)` role model, eligibility (`can_found`/`can_join` over an
  Embassy level + `AllianceRules`), and the diplomacy state machine (propose/accept/cancel, exclusivity).
  No I/O. Persisting/loading membership is infrastructure.
- **P4 (server authority):** the client only posts intents (found / invite / accept / set-stance / …).
  Every role/rights check, the cap, eligibility, the “one alliance per player”, “no self-diplomacy”, and
  the **visibility gate** (you only see rosters/allied villages/incoming for **your** alliance + direct
  confederations) are enforced server-side. The incoming view exposes **target + ETA only**, never the
  attacker's troop child rows (P4/§7.3).
- **P7 (configurable):** `maxAllianceMembers` and the join/found Embassy levels are balance
  (`alliance.toml`); the Embassy's costs/time/prerequisites are balance (`construction.toml`). No
  hardcoded sizes or levels. (World speed doesn't scale alliance state; it already scales the Embassy
  build time via the 003 path.)
- **P11 (performance):** the read-side views are bounded — eligibility is one `MAX(level)` over the
  player's buildings; the roster/visibility queries are keyed by the viewer's (single) alliance id and its
  confederate id set (≤ a handful, one hop); the incoming view is one indexed scan of in-transit hostile
  movements whose `target_village ∈ allied villages`. No N+1, no per-tick work.

## Domain (`domain`, pure)

- `building.rs` — add `BuildingKind::Embassy` (the enum is exhaustive on purpose, so this surfaces every
  persistence/label/catalog site that must handle it: `infrastructure::repo::building_str`,
  `balance::parse_building`, `web::handlers::{building_label, building_kind_id, building order index}`).
  Embassy is an **ordinary** building — no exclusivity (unlike Residence/Palace), any number of villages.
- `alliance.rs` (new) — the pure group rules:
  - `AllianceRole { Founder, Leader, Member }`.
  - `AllianceRight { Invite, Expel, Diplomacy, Announce, ManageRoles }` + `RightSet` (a `u8` bitset with
    `contains` / `insert` / `remove` / `all` / `none`).
  - `fn has_right(role, rights, right) -> bool` — `Founder ⇒ true` for every right; `Leader ⇒
    rights.contains(right)`; `Member ⇒ false`.
  - `fn can_expel(actor: AllianceRole, target: AllianceRole) -> bool` — only a strictly **higher** rank
    (Founder > Leader > Member) with the Expel right; never the Founder, never self (self handled by id).
  - `AllianceRules { max_members, join_embassy_level, found_embassy_level }` + `can_found(embassy_level)`
    / `can_join(embassy_level)` / `at_cap(member_count)` (pure gate fns).
  - `DiplomacyStance { War, Confederation }`, `DiplomacyStatus { Proposed, Active }`, and a pure
    `next_stance(current: Option<(stance,status)>, action) -> Result<Option<(stance,status)>, DiplomacyError>`
    state machine encoding AC7: War → immediately `Active` (unilateral, clears any confederation);
    Confederation → `Proposed`, accept → `Active`; cancel → `None` (Neutral); re-declaring the active
    stance is a no-op; proposing confederation while at war is rejected; exclusivity (one stance per pair).
  All functions are unit-tested without a DB.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `alliance.toml` (new) — `max_members = 60`, `join_embassy_level = 1`, `found_embassy_level = 3`. Loaded
  into `AllianceRules` by a fail-fast `alliance_rules()` (mirroring `loyalty_rules()`).
- `construction.toml` — add `[buildings.embassy]` (`time_secs`, `cost`, `prerequisites` — faithful: Main
  Building L1). `BuildingsDto` gains `embassy: LevelSpecDto`; the `build_rules()` catalog loop gains the
  `(BuildingKind::Embassy, &dto.buildings.embassy)` pair.
- `economy.toml` — add the Embassy **population** row (the `[population]` table is a name-keyed map, so a
  new `embassy = [...]` entry is picked up with no DTO change), keeping population/upkeep correct (005).

## Persistence (`infrastructure` + migration `00NN_alliances.sql`)

- `alliances (id uuid pk, name text not null unique, tag text not null unique, founder_id uuid not null
  references users(id), created_at timestamptz not null default now())`.
- `alliance_members (player_id uuid primary key references users(id), alliance_id uuid not null references
  alliances(id) on delete cascade, role text not null, rights int not null default 0, joined_at timestamptz
  not null default now())` — **`player_id` as PK enforces one alliance per player** (AC12) at the DB; an
  index on `alliance_id` powers roster/cap/visibility queries.
- `alliance_invitations (id uuid pk, alliance_id uuid not null references alliances(id) on delete cascade,
  invitee_id uuid not null references users(id), created_at timestamptz not null default now(), unique
  (alliance_id, invitee_id))` — a pending invite is a row; accept/decline/revoke **delete** it (accept also
  inserts the member, in one tx). (No status column needed — absence = resolved.)
- `alliance_diplomacy (alliance_lo uuid not null, alliance_hi uuid not null, stance text not null, status
  text not null, proposed_by uuid, created_at timestamptz not null default now(), primary key
  (alliance_lo, alliance_hi), check (alliance_lo < alliance_hi))` — the **normalised** unordered pair: the
  `lo < hi` check makes **self-diplomacy structurally impossible** and the composite PK makes **two stances
  per pair impossible** (AC7/AC12). Both ids `references alliances(id) on delete cascade` so a disband
  clears its diplomacy automatically.
- `PgAllianceRepository` (new) implements the `AllianceRepository` port: the membership/invitation/
  diplomacy reads + the guarded mutations, each a small tx. **Disband** is one tx: delete the alliance row;
  `on delete cascade` removes members, invitations, and diplomacy. The Embassy needs **no** schema change
  (buildings are generic) — only the catalog wiring above + the `building_str` mapping `"embassy"`.

## Application (`application`)

- `ports.rs` — add `AllianceRepository`: eligibility (`max_embassy_level(player) -> u8`), `alliance_of(
  player) -> Option<Membership>`, `member_count`, `roster`, `members_villages(alliance_ids)` (the shared
  village list), `pending_invites_for(player)` / `invites_of(alliance)`, `diplomacy_of(alliance)`,
  `confederate_alliances(alliance)`, and the mutations (`create_alliance`, `add_member`, `remove_member`,
  `set_role_rights`, `transfer_founder`, `insert_invite`/`delete_invite`, `upsert_diplomacy`/
  `delete_diplomacy`, `disband`). The **incoming-defence** read (`incoming_against(village_ids)`) returns
  `{ target_village, coordinate, arrive_at }` only — selected from `troop_movements` where `kind ∈
  {attack, raid}` and `target_village = ANY($ids)` and `status='in_transit'`, **without** joining
  `movement_troops` (so no composition leaks).
- `alliance.rs` (new use-cases) — thin orchestration: load the actor's membership, run the **pure**
  role/rights/eligibility/diplomacy checks, then call the repo. Commands: `found_alliance`,
  `invite_player`, `respond_invite { accept|decline }`, `revoke_invite`, `leave_alliance`, `expel_member`,
  `set_member_role` (promote/demote + rights), `transfer_founder`, `disband_alliance`, and `set_diplomacy
  { declare_war | propose_confederation | accept_confederation | cancel }`. Each returns a typed
  `AllianceError` (NotEligible, NameTaken, AlreadyInAlliance, NotAMember, MissingRight, AtCap,
  InviteeIneligible, RankTooLow, SelfTarget, BadStance, …) enforced **before** any write (P4).
- Read-side: `alliance_view(viewer)` (roster + my role/rights + diplomacy + incoming overview, all
  visibility-gated), `eligibility(viewer)` (max Embassy → can found/join), and `visible_villages(viewer,
  target)` (allowed only if `target` is a fellow member or a one-hop confederate, else `Denied`).

## Interface (`web`)

- **Embassy / alliance page** (new handler + Askama templates, obeying the ui-style-guide):
  - **No alliance:** show eligibility — *Found* form (name + tag) when Embassy ≥ 3; the list of **pending
    invitations** with *Accept* / *Decline* when Embassy ≥ 1; otherwise the "build an Embassy" hint.
  - **In an alliance:** the **roster** (members, roles, rights), the **diplomacy** list (stances +
    pending confederation proposals), the **incoming-defence overview** (AC9), and — gated by the
    viewer's rights — the **invite / expel / promote-demote / set-rights / diplomacy** controls and
    (Founder) **transfer / disband**.
- **Shared village visibility:** from the roster, a member can open a fellow member's / confederate's
  **village list** (coordinates + names only); the server re-checks the visibility gate (P4).
- **Reinforcement coordination:** no new order — the existing 007 reinforce (the Rally Point form) is
  reachable for an allied village's coordinates surfaced by the roster/incoming views.
- **Tag surfacing:** the alliance **tag** appears on the **map** tile owner and the player's view (the map
  already exposes ownership; the tag joins it). 
- All POST handlers map `AllianceError` to a flash/redirect; the client is never trusted (P4).

## Test strategy

| AC | Test |
|----|------|
| AC1 | balance/domain: Embassy loads in the catalog; `can_found`/`can_join` gate on the level; infra: building a level-1 Embassy via the 003 path reads back; eligibility = max across villages. |
| AC2 | infra (DB): found creates the alliance + Founder member; duplicate name/tag, already-in-alliance, Embassy < 3 are rejected. |
| AC3 | infra (DB): invite → accept inserts a member + deletes the invite; decline/revoke delete it; accept rejected when alliance-less no longer holds / Embassy < 1 / at cap. |
| AC4 | domain: `at_cap`; infra: the (max+1)th accept is rejected; count is exact. |
| AC5 | infra (DB): leave removes the member; Founder leave rejected; expel a lower rank works, expelling Founder/equal rejected; **disband** clears members + invites + diplomacy in one tx. |
| AC6 | domain: `has_right`/`can_expel` truth table (Founder all, Member none, Leader only granted); app: an action without the right is denied; only manage-roles/Founder changes roles. |
| AC7 | domain: the diplomacy state machine (war unilateral + clears confed; confed propose→accept; cancel→neutral; exclusivity; idempotent); infra: the normalised pair rejects self + double-stance. |
| AC8 | app/infra: a member sees fellow members' & confederates' village lists + roster; a non-member/non-confederate is `Denied`; troop counts never appear. |
| AC9 | app/infra (DB): an inbound attack/raid on an allied village shows in the viewer's incoming list (target + ETA only); a resolved/cancelled movement drops off; non-allied targets are excluded. |
| AC10 | web/infra: a member can issue a 007 reinforcement to an allied village (existing path); no combat-math change (covered by 007/009 tests staying green). |
| AC11 | web integration: found → invite → accept → set rights → declare war / propose+accept confederation → see roster + incoming; tag shows; non-member is refused the roster. |
| AC12 | infra (DB): one-alliance-per-player (PK), no double-accept, cap guard, no self/dup diplomacy (constraints), disband cascade exactly once; all reproducible from rows. |

## Notes / open risks

- **The Embassy enum variant touches every exhaustive `BuildingKind` match** (persistence string, balance
  parse, web label/id/order). The compiler enumerates them; T1 lands the variant + all catalog/label
  sites + the balance entry together so the workspace stays green.
- **Diplomacy normalisation is the subtle invariant.** Always store/lookup the pair as `(min(id), max(id))`
  with `lo < hi`; the application must normalise before every read/write so the PK + check carry the
  "no self / one stance per pair" guarantees. War is one row read as mutual; confederation is `Proposed`
  until the counterpart flips it `Active`.
- **Visibility gate is security-sensitive (P4).** Every roster / allied-village / incoming read must
  recompute the viewer's alliance + one-hop confederate set server-side and filter to it — never trust a
  posted alliance/player id. The incoming query must **not** join `movement_troops` (composition stays
  hidden, §7.3).
- **Disband / leave coherence.** Disband relies on `on delete cascade`; verify the cascade reaches
  members, invitations, and **both** orientations of diplomacy. A Founder must `transfer_founder` or
  `disband` before leaving (AC5) — guard + test. Removing a member must also drop their pending invites?
  (No — invites are to *non-members*; a member has none.)
- **Incoming view is the first per-player incoming surface.** It also lets a player see **their own**
  village's incoming (a member's allied set includes themselves). Keep the query and the template generic
  so a future per-player "incoming attacks" view reuses it.
- **Phasing (T1–T8)** lands each phase green: Embassy building + balance + eligibility (T1); pure
  `alliance.rs` rights/roles/eligibility/diplomacy state machine (T2, test-first); membership persistence
  + found/invite/accept/leave/expel/roles (T3); diplomacy persistence + state machine wiring (T4);
  shared-visibility + incoming-defence reads (T5); web pages + reinforcement surfacing (T6); docs (T7);
  review (T8).
