# Feature 045 — Player multi-world UX — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Land the read correctness first (per-row name re-pointing), then the player-facing lobby/join/switch UI on
top. Each stage is behaviour-preserving in the home world (the existing suite is the oracle) and adds a
focused multi-world test. No domain change (P3). The aggregate boards + public read pages are a separate,
coherent world-scoping change (046).

## Stages (each a commit; suite green before advancing)

1. **Per-row read re-pointing (`repo.rs`).** Rewrite the 13 per-row cross-player `JOIN users u ON u.id =
   <game id>` reads to go through `players` (map owners, reinforcements_at/_of, battle-report attacker/
   defender, oases, alliance members + member villages, invitations, forum thread/post authors, scout
   scouter/target). Home parity holds via `player.id == user.id`. Add a DB test: a second-world player's
   name resolves (e.g. alliance roster / reinforcement / report). (AC1/AC5)
2. **Lobby + join + switch + nav.** `GET /worlds` (joined + joinable), `POST /worlds/join` (create player +
   select), nav link + current-world label; switching reuses `POST /world/select`. Integration: join a 2nd
   world through the lobby, land in its village, see its name resolve, switch back home. (AC2/AC3/AC4)
3. **Acceptance.** Full suite green; spec/plan/tasks; the end-to-end multi-world loop test. (AC5)

## Key decisions

- **Re-point through `players`, not a schema change.** The 042 FKs already point at `players`; only the read
  joins lagged. Rewriting the joins (not adding columns) keeps it a pure query change, home-parity by the
  reuse-UUID invariant, and NPC-safe (the NPC has a `players` row).
- **The lobby is the switch hub.** Rather than plumb a joined-worlds dropdown into every page header, the
  nav links to `/worlds` which lists joined worlds with switch buttons — full switching, minimal surface.
- **Join is server-authoritative & idempotent.** Only a running, not-already-joined world is honoured;
  `create_player_in_world`'s `Duplicate` is treated as success (already joined), so a double-submit is safe.

## Risk

- The re-pointing touches many queries; a missed/extra join is caught by `clippy`/compile (column refs) and
  the full suite (home-parity names) plus the new second-world name test. Each rewrite is identical in shape.
- Per-request cost unchanged: the added `players` hop is a single PK join on an already-joined row — within
  P11. No new hot-path query.
