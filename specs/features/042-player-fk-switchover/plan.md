# Feature 042 — Player-FK switch-over — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Behaviour-preserving infra change, guarded by the full existing suite (every existing owner keeps
`owner_id == user_id`, so all reads are unchanged). The only new capability is structural: the game FKs now
allow a non-account player (NPC today; second-world players in 045). No domain change (P3); no read
re-pointing yet (deferred to 045, when second-world players become reachable).

## Stages (each a commit; suite green before advancing)

1. **Placement reuse + join primitive.** Extract `place_starting_village` from `create_account`; add
   `create_player_in_world` (fresh player + village in this repo's world; `Duplicate` on re-join). DB test:
   an account joins a second world. (Registration tests guard the extraction.)
2. **NPC player.** In the artifact (020) + Wonder (021) release, ensure an NPC `players` row with id = NPC
   user id (reuse-UUID) before placing NPC villages.
3. **FK re-point migration (0045).** Re-point the ~16 game columns to `players(id)`, preserving `ON DELETE`.
   Fix any test fixtures that insert villages with an owner that lacks a `players` row (add the player, or
   route through the release/`create_account`).

## Risk & guard

- The migration's constraint names must be exact; a wrong name fails every `#[sqlx::test]` (fresh DB per
  test), so the suite catches it immediately.
- Dev DB: existing villages own by `user_id` and `players.id == user_id` (037 backfill), so the re-point
  holds; an NPC that already released (none expected pre-90-days) would need its player — verified on the
  live dev DB after restart.

## Verification

- Full workspace suite green (behaviour preserved). DB tests: join-a-second-world; NPC release still works
  with the FK in place.
