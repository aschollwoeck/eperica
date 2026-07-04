# Plan — 120 AI players: seeding, visibility & carve-outs

**Status:** Verified (built as planned; Decisions #6–#7 recorded during review)

## Constitution check

- **P1:** no new scheduled work — the sweep carve-out is a predicate inside the existing 019 sweep;
  visibility is a per-world constant read once at world start (registry cache).
- **P3:** no domain change. `is_ai`, keys, visibility are account/world/app concerns.
- **P4:** the NPC tag derives server-side (`is_ai` + the world's setting); seeding/revoking are
  admin-gated fail-closed (118 rule); nothing grants agents new capability.
- **P7:** nothing time-scaled; the sweep predicate is time-independent.
- **P11:** sweep exclusion is one SQL predicate on the existing victim-select; visibility adds zero
  per-request queries (cached in `WorldMeta`); board queries gain one join on `users.is_ai`;
  seeding is admin-rare and capped.

## Decisions

1. **Sweep carve-out = one SQL predicate.** The 019 victim-select (`repo.rs` sweep query, which
   already guards `is_npc = false`) gains
   `AND NOT (is_ai AND EXISTS (SELECT 1 FROM agent_keys WHERE user_id = users.id AND revoked_at IS NULL))`.
   No port/application change; "enabled = holds an unrevoked key" stays derived (no new state).
2. **Signals carve-out, not report carve-out.** `account_signals` short-circuits for an `is_ai`
   subject (zeroed `AccountSignals` — no port calls), and `ip_association_count` excludes
   `is_ai` rows so a fleet on the server's IP never flags *humans* by association. **Player-filed
   reports against bots stay possible** — refusing them would leak the disguise on a `disguised`
   world (AC4 outranks; the spec's AC5 "signal queue" wording is corrected to "signal surfaces").
   The mod account view gains an `is_ai` badge (always shown to moderators). The inhuman-rate
   detector reads the `'action'` rate key, which agent traffic never populates (it counts under
   `'agent'`), so the short-circuit is belt-and-braces there.
3. **Visibility storage & plumbing.** Migration `0051_ai_visibility.sql`:
   `ALTER TABLE worlds ADD COLUMN ai_visibility text NOT NULL DEFAULT 'labeled'` (existing worlds
   → labeled, AC7). Collected on the admin create-world form (select, default labeled), threaded
   through `create_world` → the worlds INSERT. Read **once per world start** into `WorldMeta`
   (registry) and surfaced as a `pub ai_labeled: bool` on `GameContext`/`WorldScope` via
   `context_for` (the world-row read precedent, but cached — zero per-request queries).
4. **Tag surfaces (AC3), all gated on `ai_labeled`:**
   - Boards: `LeaderboardRow` gains `is_ai` (join `users.is_ai` in the five player-board queries);
     `LeaderboardRowView` renders an "NPC" badge next to the name.
   - Stat page: `player_stats_page` adds a `find_user_by_id` read → `PlayerStatsTemplate.is_ai`.
   - Map: `VillageMarker` gains `is_ai` (join in `villages_at`); the inspector label appends
     `" (NPC)"` — which flows to the draggable client and the agent map window for free (both
     serialize the same cells).
   On a `disguised` world all three render byte-identically to a human's (AC4).
5. **Bulk seeding.** `POST /admin/agents { world, count, tribe_mix }` (admin-gated fail-closed,
   `count` clamped to 1..=50 per action — P11): loops the 118 four-step mint with names from a
   static pool (web-layer constant — names are not sim rules) + numeric discriminator retry on
   `RegisterError::Taken`. The response renders a **one-time key manifest** — a JSON block
   (`[{username, token}]`) in a copy textarea; nothing persisted beyond hashes (118 rule), so
   re-rendering never reveals keys (AC1). The console also lists the selected world's bots
   (name, tribe, enabled = unrevoked-key EXISTS, created) with per-bot revoke and fleet-wide
   revoke (POST /admin/agents/revoke).
6. **`tribe_mix`**: `even` (round-robin romans→teutons→gauls — deterministic, testable, guaranteed
   mix; labeled "even mix" in the UI) | `romans` | `teutons` | `gauls`. (Originally "random uniform";
   changed to round-robin during build for determinism — P6-friendly and directly assertable.)
7. **Cross-world fleets (recorded semantics).** The 118 mint composes `register`, which always
   places a home-world player+village before joining the target world. Consequences, accepted and
   surfaced rather than hidden: a bot seeded into world X also exists (idle) in the home world and
   appears in both worlds' fleet lists; **fleet revoke disables the bot's ACCOUNT keys** — its
   presence in every world — so the button is labeled with that blast radius. A home-placement-free
   mint would need a new account-creation path; deferred until a real multi-world fleet needs it.

## Test strategy

- **Repo (`#[sqlx::test]`):** sweep spares an enabled bot however stale, sweeps the same bot once
  revoked (mirrors `sweep_abandons_inactive…`); `ip_association_count` excludes bots; signals
  short-circuit for `is_ai` subjects (mirrors `detection_signals_are_reproducible`).
- **Integration:** bulk seed N=3 (distinct names, villages placed, keys work via `/api/me`;
  manifest present exactly once); fleet list + revoke (key then 401, bot listed disabled);
  non-admin denied; labeled world → NPC badge on board row + stat page + map label; disguised
  world (created via the new form field) → none of the three; mod account view shows the badge on
  both; a filed report against a bot still lands in the queue (disguise preserved).

## Tasks

See [tasks.md](tasks.md). Gates every task: `cargo fmt --all -- --check`,
`clippy --all-targets -- -D warnings`, `cargo test --workspace`, P11.

## Key risks

- **`context_for` tuple arity** touches every caller — mechanical but broad; compiler-guided.
- **Board SQL joins**: five queries to touch consistently; the repo test pins one and the
  integration test pins the rendered badge.
- **Name pool exhaustion**: the discriminator retry is bounded (give up after a few attempts per
  bot with a clear flash) so a hostile pool state cannot loop the handler.
