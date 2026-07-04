# Feature 121 — the bot runner (`eperica-bots`): rule-based reflexes on humanized cadence

**Status:** Verified (built on feature/121-bot-runner; reviewer APPROVE)
**Depends on:** 118–120 (the complete Agent API + fleet seeding/key manifests), ADR 0036.
**Roadmap:** slice 4 of the AI-players program (118–122).

## Goal

A world an admin has seeded **visibly lives**: bots grow their villages, keep their storage from
overflowing, train troops, raid farms, and settle new villages — for days, unattended. `eperica-bots`
is a **separate binary** (new workspace crate) that consumes a 120 key manifest and plays through the
Agent API as a true client. No server change of any kind in this slice; the LLM strategist (122)
plugs in later — the runner is fully functional without it.

## Concepts

- **A client, not a service.** The runner holds N bot identities (from the manifest), talks only
  HTTP (`docs/agent-api.md`), and keeps no state the server doesn't already have — restart-safe by
  construction: every tick starts from a fresh digest.
- **Reflex policies are pure functions.** `policy(digest, persona, now) -> Vec<Intent>` — no I/O,
  fully unit-testable (the client-side analogue of P3). Intents are a small enum (Build, Train,
  Research, Raid, Settle, …); an **executor** maps intents to API calls and handles the error
  contract (409 = a rule said no, fine; 429 = back off by `retry_after_secs`; 401 = key dead, drop
  the bot).
- **Personas & humanized cadence** (the difficulty knob): each bot derives a deterministic persona
  from its username (hash → activity window, tick interval band, aggression). Bots act only inside
  their activity window, with jitter between ticks — no bot ever plays 24/7 or reacts instantly.
- **The reflex set (opening doctrine):**
  - **Economy:** keep the build queue busy by priority — resource fields (lowest level first,
    crop-biased when crop net is low) → warehouse/granary when any store is near capacity → core
    buildings (main building, barracks) per a small doctrine table; train a modest garrison;
    research when affordable.
  - **Raiding:** scan the map window for **inactive-marked** villages (the map label carries the
    existing `(inactive)` marker) in range; send small raids on a loop; never raid when the
    garrison is below a floor.
  - **Settling:** when CP allows another village and settlers are affordable, train settlers and
    settle the nearest free valley from the map window.
  - **Defence (modest):** if an incoming attack lands soon and the bot has a second village, evacuate
    the garrison there (reinforce own village) and recall after; a single-village bot just keeps
    building (dodging needs somewhere to go).

## Operation

`eperica-bots --server http://host:8080 --world <uuid> --keys agents.json [--dry-run] [--tick-secs N]`
(flags or env). `agents.json` is the 120 manifest verbatim. `--dry-run` logs intents without POSTing.
Structured per-bot logs (bot name, tick, intents, outcomes). Graceful shutdown on ctrl-c.

## Acceptance criteria

- **AC1 — Manifest in, fleet up.** The runner loads the manifest + server URL, validates each key
  via `GET /api/me` at startup, runs all valid bots, and cleanly drops (with a log) any dead key —
  one bad key never stops the fleet.

- **AC2 — Pure, deterministic policies.** Every reflex is a pure function of (digest snapshot,
  persona, now): same inputs ⇒ same intents. Unit tests pin the doctrine: field priority (incl.
  crop bias + storage-upgrade trigger), raid target choice (inactive, in range, garrison floor),
  settle trigger (CP + settlers), and the evacuate reflex.

- **AC3 — Executor honours the API contract.** Intents become the documented calls; `409` outcomes
  are logged and skipped (the server said no — never retried in the same tick), `429` backs off by
  `retry_after_secs`, `401` retires the bot, `5xx`/network errors back off with jitter. The digest
  is fetched **once per tick** per bot (the documented budget-friendly loop).

- **AC4 — Humanized cadence.** Personas derive deterministically from the bot name: activity window
  (start hour + length), tick interval band, aggression. A bot outside its window does nothing; two
  ticks of one bot are never closer than its minimum interval. Unit-tested (fixed names ⇒ fixed
  personas; window boundaries respected).

- **AC5 — Fleet loop.** N bots run concurrently on one runtime with bounded in-flight requests
  (never more than a configured cap), so a 50-bot fleet cannot stampede the server (the 118 budget
  is per bot; the cap protects the process and the box). Ctrl-c drains cleanly.

- **AC6 — End-to-end tick.** Against a spawned in-process server (the web crate's test harness):
  a seeded bot's single forced tick reads the digest and produces real orders — a build order
  (per doctrine — storage first on the seeded state: granary) and a training batch appear in the
  next digest. `--dry-run` produces the same intents with zero server writes.

- **AC7 — No server change.** The slice adds the `eperica-bots` crate only; the server workspace
  builds bit-identically (no migrations, no route changes, no balance changes).

## Roles & permissions

Per [roles.md](../../roles.md): the runner is an **external client** holding 118 agent keys — it has
exactly the Agent's surface, nothing more. No new server roles or permissions. (Operator concerns —
where the runner runs, key custody — are deployment, not game rules.)

## Out of scope

- The LLM strategist, diplomacy/messaging behaviour (122 — the persona/intent seams are built for it).
- Alliance play, oasis raids, catapult targeting doctrine, multi-wave attacks — later doctrine work.
- Server-side anything (120 closed that); a supervisor/daemonized deployment story.
- Difficulty *tiers* as named presets — 121 ships the persona knobs; tiers are a config file away.

## Open questions (for plan.md)

- HTTP client: `reqwest` (already in the workspace as a dev-dependency of web tests) — promote to a
  normal dependency of the new crate.
- Whether the map scan caches between ticks (a raid-target list is stable for minutes) — leaning a
  small per-bot TTL cache to spare the rate budget.
- Tick scheduling: one tokio task per bot vs a single scheduler loop — leaning single loop + bounded
  semaphore (AC5's cap falls out naturally).
