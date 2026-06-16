# Feature 043 — Request world-context — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Add the seam, prove it with one handler, keep everything else on the home `AppState`. Behaviour-preserving
in the home world (the existing suite guards it). No domain change (P3).

## Stages (each a commit; suite green before advancing)

1. **Registry world-context.** `WorldRegistry` gains a `Mutex<HashMap<WorldId, WorldMeta{seed,radius,speed}>>`
   (populated in `start_world`) + the loaded `MapRules`; `context_for(world_id) -> Option<(repo, Arc<map>,
   speed, radius)>`. Unit/DB-ish test via the registry (build a context for a known world).
2. **`world` cookie + `GameContext` extractor.** `world_cookie`/`clear_world_cookie`; the extractor
   (cookie → world default home; effective account → `player_in_world`; home fallback if not joined; world
   must be registry-run). Lives in `auth.rs` next to the other extractors.
3. **Village proof migration.** `/village` uses `GameContext`. Existing village tests prove home-world
   parity; add a multi-world integration test: a player joined to a 2nd world + the `world` cookie set →
   the village page renders that world's village.

## Key decisions

- **Home fallback, not redirect.** Until the lobby (045) there is no `/worlds`; a `world` cookie pointing at
  a world the account has not joined falls back to the home world (always present), so `GameContext` never
  fails for a logged-in account.
- **`AuthUser`/`RealUser` unchanged.** The world cookie only affects the *game* identity (`GameContext`);
  account surfaces keep `AuthUser`/`RealUser` (the human). Moderator/admin/sitting/messaging are untouched.

## Risk

- Building a repo per game request is cheap (generate-on-read map); the only per-request DB cost is the
  `player_in_world` lookup (one indexed query) — within P11.
