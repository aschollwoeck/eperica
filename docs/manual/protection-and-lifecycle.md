# Protection & a living world

A new village is fragile, and an MMO map clogged with dead villages goes stale. Two systems keep the
start fair and the world fresh.

## Beginner's protection

When you register, your village is **immune to attack** for a protection window (shorter on faster
servers). No one can raid or attack you while you find your feet — your village page shows how much
protection remains.

Protection is a **foothold, not a fortress**. It ends early when either:

- you **grow up** — once your population passes a threshold, you're established and protection lifts; or
- you **go on the offensive** — the moment you launch your own attack or raid, your protection ends (you
  can't hit others while hiding behind it).

Once it ends, it doesn't come back.

## Inactivity & abandonment

The world reclaims players who stop playing, in two stages:

1. **Inactive (farmable).** After a stretch with no activity, an account is marked **inactive** — its
   villages show **greyed** on the map so active players can spot them as farms. (Inactive players can be
   attacked under the normal rules; nothing else changes.)
2. **Abandoned.** After a much longer absence, the account is **retired**: its villages are **removed from
   the map**, freeing those valleys for new settlement, and the account can no longer log in.

This keeps the map alive and reclaimable instead of frozen around players who have left. Staying active —
even just logging in — keeps your account out of the lifecycle.

*(Beginner protection scales with world speed; the inactivity and abandonment windows are real
wall-clock time regardless of speed.)*

**AI bot players** (ordinary game participants with an active API key) are never **removed** by
the abandonment sweep; once all keys are revoked they re-enter the normal lifecycle like any
quiet player. They may still appear as greyed/inactive on the map — the sweep exemption covers
removal, not the inactive marker.

**Natar accounts** — the synthetic accounts that hold end-game artifact villages — are a
separate entity, excluded from the abandonment sweep and the leaderboards. (Their villages may
still carry the map's "inactive" marker — that marker is derived purely from account activity.)
