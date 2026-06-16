# Feature 042 — Player-FK switch-over

**Status:** Draft
**Depends on:** 037 (players table + backfill), 020/021 (Natar/Wonder NPC)
**Roadmap:** M9 multi-world & administration — the first of the **player multi-world UX** sub-program
(042 FK switch-over → 043 request context → 044 handler migration → 045 lobby/join/switch). See
[ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The 037-deferred **switch-over**: re-point the per-world game foreign keys from
`users(id)` to `players(id)` so a player that is **not** the account (a future second-world player, or the
Natar NPC) can own villages, movements, culture, alliance membership, etc. **Behaviour-preserving** — every
existing owner still has `player.id == user.id` (the 037 backfill + an NPC player that reuses the NPC user
id), so all `owner→user` reads keep working and the suite stays green.

## Problem

`villages.owner_id` and ~15 other game columns reference `users(id)`. A second world's player has a **fresh**
id (not a user id), so it cannot own anything — every game insert violates the FK. The Natar NPC has the
same problem: it owns villages but is not a normal account. Until the game FKs point at `players`,
no non-account player can exist.

## Goal

- **AC1 — Game FKs reference `players`.** Re-point the per-world game columns (`villages.owner_id`,
  movement/trade `owner_id`, battle-report attacker/defender + `battle_defenders`, scout-report
  scouter/target, `player_culture`, alliance founder/member/invitee, `population_snapshots`,
  `player_achievements`, `player_quests`, `notifications`) from `users(id)` to `players(id)`, preserving
  each column's `ON DELETE`. **Account-level** tables (sitting, messaging, fair-play, notification
  settings/mutes) stay on `users(id)`.
- **AC2 — The NPC is a player.** The Natar/Wonder release creates a per-world **NPC player whose id equals
  the NPC user id** (reuse-UUID, like the home backfill), so NPC-owned villages satisfy the new FK and every
  `owner→user` read for the NPC still resolves.
- **AC3 — Join-a-world primitive.** A repo can create a player for an existing account in its world
  (`create_player_in_world`): a fresh player id + a starting village placed on **that world's** map (shared
  with registration via an extracted `place_starting_village`). Re-joining is rejected. (Not yet wired into
  a UI — that is 045.)
- **AC4 — Behaviour preserved.** `owner_id == user_id` still holds for every existing owner (home players +
  NPC), so all reads are unchanged and the full existing suite passes. Pure `domain` untouched (P3).

## Design

- **Migration 0045** — `ALTER TABLE … DROP/ADD CONSTRAINT` per game column to `players(id)` (values already
  satisfy it via the 037 backfill; no data change). On an empty schema (`#[sqlx::test]`) a wrong constraint
  name fails the migration loudly, so the names are validated by the suite.
- **NPC player (reuse-UUID)** — in the artifact (020) + Wonder (021) release, after the `Natars` NPC user is
  ensured, `INSERT INTO players (id, user_id, world_id, tribe) VALUES (npc, npc, world, 'romans') ON
  CONFLICT DO NOTHING`, so the NPC's player id == its user id; NPC villages own by that id.
- **Placement reuse** — extract `place_starting_village(tx, owner, tribe, template)` from `create_account`
  (registration tests guard it); add `create_player_in_world(user, tribe, template) -> PlayerId`.

## Out of scope (later sub-program slices)

- The request-path `GameContext` + `world` cookie (043); migrating game handlers (044); the lobby / join /
  switch UI **and re-pointing `owner→user` reads through `players`** for second-world players (045 — needed
  only once such players are reachable).
