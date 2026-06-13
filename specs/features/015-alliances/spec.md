# Feature 015 — Alliances & diplomacy

**Status:** Draft
**Depends on:** 003 (construction — the Embassy is a normal infrastructure building, built as a 003 due-event), 001 (accounts/players, the actor an alliance groups), 007 (troop movement — the reinforcement + incoming-movement data the shared-defence views ride), 009 (combat — the hostile movements the incoming view surfaces), 013 (the per-player village list the shared-visibility views expose), 006 (the seeded map whose alliance tags are public)
**Roadmap:** M5 · slice 015 · GDD §10, §4.2, §7.3 — the **group layer**: players band into **alliances** via the **Embassy**, hold **roles & granular rights**, and set pairwise **diplomacy** (confederation / war / neutral). Confederation/alliance unlocks **shared visibility** and **coordinated defence** (incoming-attack awareness + reinforcement).

## Goal

A player stops fighting alone: they **found or join an alliance** through the **Embassy**, take an
**alliance role** with **granular rights**, and their alliance sets **diplomacy** with others. Belonging
to an alliance (or a **confederation** of allied alliances) grants **shared visibility** (members and
confederates see each other's villages, profiles, and the alliance roster) and **coordinated defence**
(members see **incoming attacks** against any allied village and can **reinforce** them). This slice
delivers the alliance as server-authoritative game state — membership, roles/rights, diplomacy, and the
visibility/defence effects — not the social/communication layer (chat, forum, report-sharing) which is
app-layer (GDD §10.4) and stays out of scope.

## Concepts

- **Alliance.** A named group of players with a unique **name** and a short unique **tag** (shown on the
  map and profiles — public, §7.3). Founded and held through the **Embassy**. Bounded to a
  **server-config maximum membership** (`maxAllianceMembers`, faithful default ~60, P7). An alliance is
  a per-**player** grouping (not per-village): a player belongs to **at most one** alliance at a time.
- **The Embassy (a new building).** An infrastructure building (GDD §4.2) built like any other (003,
  P1 due-event), with costs / build-time / prerequisites in balance (P7). Its level is the **gate**:
  **level 1** lets a player **join** an alliance; **level 3** lets a player **found** one (faithful
  Travian). The gate is checked against the player's **highest** Embassy across their villages.
- **Alliance roles (layered on Player, roles.md §3).** **Founder** — full control (one per alliance).
  **Leader** — holds an explicit, granular **rights** set; only what is granted. **Member** — belongs,
  with no management rights. Rights are: **invite** (invite/revoke), **expel**, **diplomacy** (set/accept
  stances), **announce** (post the alliance announcement), and **manage-roles** (promote/demote leaders
  and grant rights). The Founder implicitly holds every right.
- **Invitations.** Joining is by **invitation**: a rights-holder invites a player; the player **accepts**
  (joins) or **declines**; the inviter may **revoke** a pending invite. Acceptance is gated on the
  invitee still being alliance-less, holding **Embassy ≥ 1**, and the alliance being **below the cap**.
- **Diplomacy (pairwise stances).** Between two alliances, set by a **diplomacy** rights-holder:
  - **War** — declared **unilaterally** (one side's declaration puts both into a mutual *war* state);
    formal hostility (the kill/war statistics it enables are **016**, not built here).
  - **Confederation** — trusted allies; requires **mutual consent** (one side proposes, the other's
    diplomacy holder **accepts**) before it takes effect. Grants the confederation visibility/defence
    effects below.
  - **Neutral** — the default **absence** of a stance; either side may **cancel** a war or confederation
    back to neutral.
- **Shared visibility.** Within an alliance — and across a **confederation** — members may view each
  other's **profiles**, **village lists**, and the **alliance roster / diplomacy page**. This is the
  trusted-group exception to §7.3 (which otherwise hides a player's holdings); troop counts and
  resources stay hidden (only village existence/locations + public profile, exactly the public map plus
  the roster). Outside the alliance/confederation, nothing beyond the public tag is revealed.
- **Coordinated defence.** Two effects: **(a) incoming-attack awareness** — a member sees the
  **incoming hostile movements** (attack / raid / siege, incl. a conquest attack) targeting **any
  village owned by a fellow member or a confederate**: the target village + coordinate and the **arrival
  time** only — never the attacker's troop composition (still hidden, P4/§7.3, until scouted/resolved by
  010/009). (This also introduces the **defender's own** incoming view, which did not exist before.)
  **(b) reinforcement** — via the shared village list a member/confederate can send a 007 reinforcement
  to an allied village. **No combat-math change**: reinforcements already defend per 007/009; the
  alliance only makes allied villages **findable** and their peril **visible**.

## User stories

- As a **player**, I want to **build an Embassy and found an alliance**, so I can lead a group.
- As a **player**, I want to **invite** trusted players and **accept** invitations, so we fight together.
- As a **founder**, I want to grant **leaders** specific **rights**, so I can delegate without handing
  over the whole alliance.
- As a **leader with diplomacy rights**, I want to declare **war** or form a **confederation**, so our
  group's politics are formal and visible.
- As a **member**, I want to **see incoming attacks** on my alliance's villages and **reinforce**
  allies, so we defend together.
- As a **member**, I want to see my **alliance's roster and diplomacy** and my allies' villages, so we
  can coordinate.

## Acceptance criteria

> All alliance state is **server-authoritative** (P4) and persisted; every membership, role/rights, and
> diplomacy transition is checked server-side and is reproducible from persisted rows (P2). Visibility
> and defence views are **derived** from current state on read (P1) — there is no alliance "tick".

- **AC1 — The Embassy gates alliance actions (P7).** The Embassy is an infrastructure building built via
  the 003 path (costs/build-time/prerequisites from balance, a discrete due-event, P1). A player's
  alliance eligibility uses their **highest** Embassy level across villages: **≥ 1** to **join**, **≥ 3**
  to **found**. With no Embassy (level 0) both are denied.

- **AC2 — Found an alliance.** Given a player **not** in an alliance with **Embassy ≥ 3**, when they
  found an alliance with a **name** and **tag**, then the alliance is created, the player becomes its
  **Founder** and sole member, and the name + tag are recorded. Founding is **rejected** if the name or
  tag is already taken, the player is already in an alliance, or their Embassy is **< 3**.

- **AC3 — Invite, accept, decline, revoke.** A member holding the **invite** right can invite a player
  who is alliance-less; the invitee may **accept** (becoming a Member) or **decline**, and the inviter
  may **revoke** a pending invite. Acceptance is **rejected** if, at accept time, the invitee is already
  in an alliance, holds **Embassy < 1**, or the alliance is **at the cap** (AC4). A player may hold
  multiple pending invites but join only one.

- **AC4 — Membership cap (P7).** An alliance holds at most `maxAllianceMembers` members (server-config,
  default 60). The join that would exceed the cap is **rejected**; the count is exact and computed
  server-side.

- **AC5 — Leave, expel, disband (System cascade).** A Member may **leave** (an ordinary member or
  leader; the **Founder cannot leave** without first transferring the role or disbanding). A holder of
  the **expel** right may expel a **lower-ranked** member (a leader cannot expel the founder or another
  leader of equal rank; no one expels themselves). The **Founder** may **disband** the alliance: in one
  transaction the **System** clears all memberships, pending invitations, and diplomacy stances
  involving it. Leaving/expelling/disbanding never deletes the player or their villages.

- **AC6 — Roles & granular rights (P4).** The **Founder** holds **every** right and may **promote** a
  member to **Leader**, **grant/revoke** that leader's individual rights (invite, expel, diplomacy,
  announce, manage-roles), and **demote** a leader to member. A **Leader** can perform **only** the
  actions for the rights they have been granted; any action without the matching right is **denied**
  server-side. Only the **manage-roles** right (or the Founder) may change roles/rights.

- **AC7 — Diplomacy stances.** A holder of the **diplomacy** right may, on behalf of their alliance, set
  a pairwise stance with **another** alliance: **War** takes effect **unilaterally** (both alliances are
  at war); **Confederation** is **proposed** and takes effect only when the **other** alliance's
  diplomacy holder **accepts**; either side may **cancel** a war or confederation back to **Neutral**.
  An alliance cannot set a stance **with itself**; a pair has **at most one** active stance; transitions
  are idempotent (re-declaring an existing stance is a no-op, not an error).

- **AC8 — Shared visibility (the trusted-group §7.3 exception).** A member may view **fellow members'**
  and **confederates'** profiles and **village lists**, plus the **alliance roster** and **diplomacy
  page**. A player who is **neither** a member **nor** a confederate is **denied** these views
  server-side and sees only the **public** tag/name (map, public profile). Troop counts and stored
  resources are **never** exposed by these views (only village existence/coordinates + public profile).

- **AC9 — Incoming-attack awareness (coordinated defence).** A member sees the **incoming hostile
  movements** (attack / raid / siege, including a conquest attack) targeting **any village owned by a
  fellow member or a confederate**: for each, the **target village + coordinate** and the **arrival
  time** — and **never** the attacker's troop composition (hidden, P4/§7.3, until scouted/resolved).
  The list is **derived on read** from current movement state (P1) for the viewer's alliance +
  confederations; a movement that resolves or is cancelled drops off.

- **AC10 — Reinforcement coordination (no combat-math change).** From the shared village list a member
  or confederate may send a **007 reinforcement** to an allied village (the existing movement order).
  Combat math is **unchanged** — reinforcements defend exactly as in 007/009; the alliance only makes
  allied villages **findable** and their incoming peril **visible**.

- **AC11 — Alliance interface.** The **Embassy page** offers, per eligibility, **found** (Embassy ≥ 3,
  alliance-less) / **accept-or-decline pending invites** (Embassy ≥ 1) / the **alliance management**
  entry (members). The **alliance page** shows the **roster** (members + roles/rights), the
  **diplomacy** list, an **incoming-defence overview** (AC9), and — per rights — the invite / expel /
  role / diplomacy controls. A player's **profile** and the **map** show the alliance **tag**.

- **AC12 — Authority, guards & exactly-once (P2/P4).** Every transition is server-authoritative and
  guarded: a player cannot be in two alliances, double-accept an invite, exceed the cap, confederate
  with themselves, hold two stances with one alliance, set a stance without the right, or act on an
  alliance they don't belong to. Disband cascades **exactly once**. All state is reproducible from
  persisted rows; the same history yields the same alliances.

## Roles & permissions

Per [roles.md](../../roles.md). Alliance roles (Founder / Leader / Member) are **ownership/in-game
roles layered on Player** (§3); a request is authorized by both the account role and the alliance
role/rights.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | View **public** alliance tag/name (map, public profile). | Found/join/manage an alliance, set diplomacy, view rosters/village-lists/incoming (redirected to login). |
| **Player — non-member** | **Found** an alliance (Embassy ≥ 3); **accept/decline** invitations to them (Embassy ≥ 1); view public tags. | Act on any alliance they don't belong to (invite, expel, set diplomacy, view its roster/members' villages/incoming); found with Embassy < 3 or while already in an alliance. |
| **Player — Member** | **Leave**; view the **roster + diplomacy**, fellow members' & confederates' **profiles/village lists**, the **incoming-defence overview**; **reinforce** (007) allied villages. | Invite, expel, set diplomacy, change roles/rights, disband (no right); expel/leave as Founder without transfer. |
| **Player — Leader** | Everything a Member can, **plus** exactly the granted **rights** (invite/revoke, expel lower-ranked, set/accept **diplomacy**, **announce**, **manage-roles**). | Any action whose **right** they lack; expelling the Founder or an equal/higher rank; disbanding (Founder only). |
| **Player — Founder** | Every right; **promote/demote** leaders, **grant/revoke** rights, **transfer** the founder role, **disband**. | Leaving while still Founder (must transfer or disband first). |
| **Moderator** | N/A (considered) — alliance moderation tooling (force-rename/disband for fair play) is a later moderation slice. | — |
| **Administrator** | Sets `maxAllianceMembers` and the Embassy balance (world config); superset. | — |
| **System** | *(system-initiated)* Enforce the cap, eligibility, and all guards at the resolution instant; **cascade a disband** (clear memberships, invitations, diplomacy) exactly once; **derive** the shared-visibility and incoming-defence views from current state on read. | — |

## Out of scope

- **Communication** — alliance **forum**, in-game **chat**, **report-sharing**, and the alliance
  **announcement** *content surface* are **app-layer** (GDD §10.4, [social-and-meta-features.md](../../social-and-meta-features.md)) → a later social slice. (015 stores the **announce right**; the rich forum/messaging UI is deferred.)
- **Alliance bonuses** — contribution-funded alliance-wide bonuses (GDD §10.3) → later T4 slice.
- **War / alliance statistics & leaderboards** — kill tracking, aggregate population, attack/defence
  points, alliance leaderboards (GDD §11.2) → slice **016**. 015 records the **war stance**; the stats it
  *enables* are 016.
- **Wonder of the World / Natar / artifacts** (the alliance win condition, GDD §11.3) → end-game slices
  (020+).
- **Sitter** (roles.md §3, GDD §12.4) — temporary delegated account access; deferred.
- **New combat math.** Alliances add **no** battle-formula change; defence is the existing 007/009.
  Friendly-fire (attacking your own alliance/confederates) is **not blocked** (faithful Travian allows
  it; see Open questions).

## Decisions

- **Alliance is per-player, one-at-a-time, persisted relationally.** An `alliances` row (id, name, tag,
  founder, created) + an `alliance_members` row per player (alliance, role, rights bitset, joined) +
  `alliance_invitations` (alliance, invitee, status) + `alliance_diplomacy` (a normalised unordered
  alliance pair, stance, status — `proposed`/`active` for confederation, `active` for war). A player's
  single membership is a uniqueness constraint (one row per player), enforcing AC12 at the DB.
- **Eligibility reads the player's highest Embassy (lazy, P1).** No new stored counter: the join/found
  gate computes `max(embassy_level)` across the player's villages on demand from existing building rows
  (003). Embassy is added to the **building catalog + balance** (a new `BuildingKind`), with costs /
  build-time / prerequisites in `buildings` balance — no new construction mechanic.
- **Diplomacy asymmetry is faithful.** **War** is unilateral (one `active` row makes both at war);
  **Confederation** is a `proposed` row that the counterpart's diplomacy holder flips to `active`
  (mutual consent). Cancelling either deletes the row (→ Neutral). The pair is stored **normalised**
  (min/max alliance id) so "with itself" and "two stances for one pair" are structurally impossible.
- **Visibility & defence are pure read-side derivations (P1/P4).** Shared visibility and the incoming
  view are **queries** gated by the viewer's membership/confederation set computed at request time —
  no denormalised caches, no events. The incoming view reuses the 007/009 `troop_movements` rows
  (hostile kinds toward allied villages), exposing only target + ETA (never troop child rows), so P4/§7.3
  hold. This is also the **first** per-player incoming-attack view (no prior slice surfaced it).
- **Rights are a small bitset on the membership row.** invite / expel / diplomacy / announce /
  manage-roles. Founder is a role flag implying all bits; Member implies none. Checks are pure functions
  over `(role, rights)` in the domain (P3), enforced in the application layer (P4).
- **Balance (P7) extends `buildings` (Embassy) + a new `alliance.toml`** (`maxAllianceMembers`, the
  join/found Embassy levels) loaded fail-fast by `infrastructure::balance`.

## Open questions

- **Friendly-fire.** Should attacking/raiding a **fellow alliance member or confederate** be
  server-blocked (like the 014 own-village denial)? **Proposed:** **no** — faithful Travian permits it
  (betrayal is a real tactic), and blocking it adds combat-path surface. 015 leaves it allowed; revisit
  if balance wants a toggle. *(Listed as a denied-case candidate so the role review is explicit.)*
- **Confederation transitivity & visibility depth.** Does confederation visibility extend to a
  confederate-of-a-confederate? **Proposed:** **no** — visibility/defence span only the viewer's
  alliance + its **direct** confederations (one hop), to bound the read and match Travian.
- **War vs. confederation exclusivity.** Can two alliances be simultaneously at war *and* confederated?
  **Proposed:** **no** — one active stance per pair (AC7); declaring one clears the other (war declaration
  cancels a confederation; proposing confederation is rejected while at war until the war is cancelled).
- **Founder transfer pre-disband.** Is founder-transfer in-scope or can the founder only disband?
  **Proposed:** **in scope** — a minimal `transfer founder` (Founder → an existing member) so a founder
  can leave without destroying the alliance (AC5/AC6).
- **Incoming view for non-attack hostility (scouting).** Should inbound **scouting** show in the
  incoming view? **Proposed:** **no** — scouting is covert (010); only attack/raid/siege (force that
  *lands*) appears, matching what a defender would physically detect.
