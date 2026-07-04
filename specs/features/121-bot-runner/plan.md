# Plan — 121 the bot runner (`eperica-bots`)

**Status:** Draft (spec approved)

## Constitution check

- **P3 (spirit):** the runner is a client, but the same discipline applies client-side: every
  decision is a **pure function** in the lib (`policy`, `persona`), I/O confined to `client`/
  `executor`/`runner`. No decision logic touches HTTP.
- **P4:** untouched — the runner can only do what any key-holder can; AC7 pins zero server change.
- **P1/P7:** no server scheduling added; the runner's own clock is wall-clock (it is a *player*,
  not the sim — bots reading speed-scaled deadlines from the digest is exactly what humans do).
- **P11:** one digest fetch per bot-tick (the documented budget loop), a per-bot map-window TTL
  cache, and a fleet-wide in-flight semaphore (AC5) keep the runner inside the 118 budgets.

## Crate layout (new: `crates/bots`, package `eperica-bots` — lib + bin)

| Module | Contents | I/O |
|---|---|---|
| `digest.rs` | serde DTOs mirroring docs/agent-api.md **partially and defensively** (`#[serde(default)]` everywhere) — additive server changes never break the runner | none |
| `client.rs` | `ApiClient` (reqwest): `me`, `state`, `map`, and one thin call per action; returns `Result<Value-or-DTO, ApiFailure>` where `ApiFailure` classifies status + `{error, reason}` | HTTP |
| `persona.rs` | `Persona::from_name(&str)` via an **inline FNV-1a** (std `DefaultHasher` is per-process-random — unusable for AC4 determinism): activity window (start hour, length), tick band (min/max secs), aggression (0..3), raid range 5..=10 — the map-window clamp bounds it | none |
| `policy.rs` | `Intent` enum + `plan_tick(&Digest, Option<&MapWindow>, &Persona, now_ms) -> Vec<Intent>` — the doctrine below | none |
| `executor.rs` | `execute(...)`: intents → API calls; a **pure** `classify(status, body) -> Outcome` (Ok / RuleDenied / Backoff(secs) / RetireBot / TransientError) unit-tested without HTTP | HTTP |
| `runner.rs` | the fleet loop: single scheduler task; per-bot `next_tick` from persona + jitter; due bots tick through a `Semaphore` (in-flight cap); ctrl-c drain | HTTP |
| `main.rs` | flag/env parsing (std, no new CLI dep): `--server --world --keys --dry-run --tick-secs --cap`; tracing setup like eperica-web's main | — |

Dependencies: tokio, reqwest (no json feature; bodies via serde_json), serde, serde_json, tracing,
tracing-subscriber. Dev-deps for e2e: eperica-web, eperica-infrastructure, eperica-domain, sqlx
(the web integration harness pattern).

## The reflex doctrine (exact rules — AC2's test surface)

Evaluated in order; the first section that yields intents ends economy planning for the tick
(one build order per tick — the queue is one-deep anyway for non-Romans):

1. **Evacuate** (aggression-independent): an incoming attack arriving within 2× tick-band max AND
   ≥2 villages AND garrison non-empty ⇒ `Reinforce(other own village, whole garrison)`; recall
   intent on a later tick once no attack is inbound.
2. **Storage:** any resource `amount ≥ 90%·capacity` ⇒ build Warehouse (wood/clay/iron full) or
   Granary (crop full) — upgrade if present, place on a free slot otherwise.
3./4. **Fields ⇄ core buildings (interleaved):** while average field level < 2 ⇒ fields only
   (crop-net floor 25/h biases to the lowest crop field; else lowest field overall, ties
   wood>clay>iron>crop; stop at level 10 — the non-capital cap). Once the average reaches 2, the
   **core-building doctrine takes priority until complete** — Main Building →3, Barracks →3,
   Warehouse →3, Granary →3, Main Building →5, Academy →1, Residence →10 (the settler chain) —
   then fields resume to the cap. The doctrine is prereq-consistent against the classic preset;
   verified in the walk test (`doctrine_table_walks_to_completion`). (Clarified during build: the
   original "first section that yields ends planning" wording made the avg-2 gate unreachable —
   fields would monopolize until all-18-at-10.)
5. **Training:** garrison below `10 + 10·aggression` units ⇒ train the tribe's tier-1 infantry up
   to what ~25% of current resources afford (never drain the build budget).
6. **Settling:** `villages_used < villages_allowed` AND Residence ≥10 ⇒ train settlers (3) when
   affordable; when the digest garrison holds 3 settlers ⇒ `Settle(nearest free valley)` from the
   map window.
7. **Raiding** (aggression ≥1, garrison ≥ `15 + 5·aggression`): pick up to `aggression` targets
   from the map window labeled `(inactive)` within persona raid range, nearest first; raid each
   with `min(8 + 4·aggression, garrison/3)` tier-1 units; never below the garrison floor; skip
   targets already targeted by an own in-flight movement (digest `movements`).

Map windows are fetched at most once per `MAP_TTL_TICKS` (default 5) per bot and cached.

## Test strategy

- **Unit (lib):** persona determinism + window/band boundaries; every doctrine rule above with
  hand-built digest fixtures (incl. the negative gates: full queue ⇒ no build intent, garrison
  floor blocks raids, in-flight target skipped); `classify` for each outcome class; digest DTOs
  parse a canned real digest JSON (fixture from docs/agent-api.md shapes).
- **E2E (`crates/bots/tests/e2e.rs`, `#[sqlx::test]`):** spawn the in-process server (web-harness
  pattern), register + flag + key a bot over HTTP/SQL (the integration-test recipe), then drive
  **one forced tick** via the lib: digest read → intents → executed; assert the next digest shows
  the field order + training batch (AC6); re-run the same tick in `--dry-run` mode against a fresh
  bot and assert zero orders appear. A dead-key bot retires without failing the fleet (AC1).
- **AC5:** a unit test on the scheduler's due-computation; the cap is exercised in the e2e (cap=1).

## Key risks

- **Doctrine vs balance drift:** the doctrine reads only the digest (levels, rates, capacities) —
  no balance constants are duplicated except the field-cap-10 short-circuit and cost *ratios* are
  never assumed (affordability is asked by attempting; 409 `insufficient` is a normal outcome).
- **reqwest as a runtime dep** is new to the workspace (was dev-only): version-pin to the one
  already in Cargo.lock.
- **Clock skew:** cadence uses the server's `now_ms` from the digest where it matters (evacuate
  timing); local time only for activity windows (a persona trait, not a sim quantity).
