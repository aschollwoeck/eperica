# Spectating

Spectator access is admin-granted and rare. Most players will never see this page from the inside —
this explains what it is, in case you encounter it or want to ask for it.

## What a spectator is

A **Spectator** is a trusted observer role, granted directly by the server operators — not
something you sign up for or join a world to get. There's no per-world flag: once your account
holds the role, it applies to **every** world on the server.

You might see this role used for:

- **Casting or commentating** a match — narrating a world's action for an audience.
- **Tournament refereeing** — settling disputes with the true state of play.
- **Research or operator oversight** — watching how a world, including an AI-populated one,
  actually unfolds.

If you'd like spectator access for one of these reasons, ask a server administrator — it's granted
(and revoked) entirely at their discretion.

## What a spectator sees

Spectating is full omniscience, zero agency. On any world, a spectator sees:

- **Every village's internals** — resources, build queue, training batches, garrison, stationed
  reinforcements, loyalty, research — for any player, not just their own.
- **Every movement in flight**, in both directions, with full composition: attacks, raids,
  reinforcements, returns, settlers, and merchant shipments. This includes hostile movements that
  the target's own view would only ever show as an unlabeled arrival warning — fog of war doesn't
  apply to a spectator.
- A **live feed** of ongoing world activity: builds and trainings completing soonest, movements en
  route, and the most recent battles.

A spectator can do nothing with any of this. There's no action a spectator can take on the world
they're watching — no messages, no attacks, no trades, nothing. It's a read-only window, not a
second account.

> **Faithful:** Spectating doesn't unlock the AI disguise. On a labeled world a spectator sees the
> same "NPC" tags a player would; on a disguised world, AI players still look exactly like humans,
> even to a spectator. The operator truth only ever lives in the admin console.

## What this means if you're playing

On a world with active spectators, fog of war is transparent to them — they can see your garrison,
your queues, and your incoming and outgoing movements exactly as you can, regardless of scouting.
That's fine for a caster or referee watching from the outside. It stops being fine if a spectator is
also a competitor in that same world — which is why the role is a trust decision the operators make
deliberately, and why they're expected to grant it to neutral observers rather than active players.
If you're ever unsure whether a world has spectators, ask your world's operator.

## The dashboard

Spectators reach their view under **`/spectate`**:

1. **The world picker** — every world on the server, running or already decided.
2. **The live feed** for a world — movements in flight with composition and arrival countdown,
   builds and trainings completing soonest, and recent battles. Each row links into a drill-down.
3. **The player list** — every player in the world (population, villages, alliance), each linking
   to...
4. **A village's full internals** — exactly as its owner would see them.

The feed refreshes on its own; there's nothing to click to keep it current.

## See also

- [AI players (NPCs)](ai-players.md) — labeled vs. disguised worlds; a spectator sees the same
  game-facing picture a player would.
- [Fair play & moderation](fair-play-and-moderation.md) — reporting and the roles that keep a world
  clean.
- [Player Manual index](README.md)
