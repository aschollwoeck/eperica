# Eperica Constitution

These are the **non-negotiable principles** of the project. Every Game Design Document decision,
feature spec, technical plan, and line of code must conform to them. A principle changes only by a
deliberate **amendment** recorded in the changelog below — never silently. If a feature seems to
require violating a principle, that is a signal to reconsider the feature or amend the principle
consciously.

## Vision (the fixed star)

Eperica is a **faithful Travian-style competitive strategy MMO**: a persistent, real-time medieval
world where thousands of players grow economies, build villages, raise armies, wage war, form
alliances, and race toward a server-end victory. Game speed is configurable. We **clone proven
mechanics first** and innovate later. The project is built to be **genuinely playable and
portfolio-grade**.

## Principles

### P1 — Lazy, event-driven time. Never tick the whole world.
The world advances while players are offline, but the server MUST NOT periodically iterate all
entities to move them forward. Continuous processes (resource production) are stored as
`value + lastUpdated + rate` and computed on read. Discrete outcomes (build completion, troop
arrival, attack landing) are stored as **events with a due timestamp** and processed only when due.
*Rationale: the only simulation model that scales to thousands of villages.*

### P2 — Reproducible state.
Any game value at any moment MUST be derivable from persisted state + elapsed time + the rules. No
correctness-critical state lives only in memory or inside a scheduler.
*Rationale: survives restarts, enables exact testing, prevents drift.*

### P3 — Pure domain core.
All game rules — production, costs, build times, combat — live in a **pure, deterministic domain
layer** with zero I/O, framework, or database dependencies.
*Rationale: this layer is the actual game; it must be unit-testable in isolation and reasoned about
precisely.*

### P4 — Server is authoritative; never trust the client.
All simulation, validation, and timing happen server-side in UTC. The client displays state and may
extrapolate for smoothness, but holds no authority over outcomes.
*Rationale: competitive multiplayer — fairness and anti-cheat are foundational, not bolted on.*

### P5 — Database is the source of truth; web tier is stateless.
Game state lives in the database. The web/app tier holds no per-player state required for
correctness, so it can scale horizontally.
*Rationale: the path to thousands of concurrent players.*

### P6 — Deterministic, seeded randomness.
Any randomness (combat variance, map generation) draws from an explicit, persisted seed, so outcomes
are reproducible and auditable.
*Rationale: testability, fairness, and being able to explain "what happened to my army."*

### P7 — Time scale is a parameter.
Game speed (1x, 5x, …) is server configuration. No duration, production rate, or cost is hardcoded to
a wall-clock value; all derive from base design values × speed.
*Rationale: configurable speed is a day-one requirement, and retrofitting it is painful.*

### P8 — Spec before behavior.
Behavior is defined by a spec with testable acceptance criteria **before** it is built or changed.
The spec is the source of truth; code conforms to it.
*Rationale: this is a spec-driven project.*

### P9 — Faithful first.
Where a mechanic exists in Travian, match its proven design before inventing alternatives. The twist
is deferred, deliberate, and spec'd separately.
*Rationale: lowest design risk; learn the genre's balance before bending it.*

### P10 — Portfolio-grade by default.
Clean layering, meaningful tests around the domain core, and documentation are part of "done," not
optional polish.
*Rationale: shipping and showcasing are both explicit goals.*

### P11 — Performance & timing precision are first-class.
Eperica is a real-time competitive game in which **sub-second timing changes outcomes** — attacks
landing, dodging, sniping a reinforcement between two arrivals. Performance is a design input at every
layer, never a later optimization:
- **Authoritative, precise time.** Server time is the single source of truth, UTC, at millisecond
  resolution; game timestamps carry sub-second precision.
- **Deterministic ordering at an instant.** Events due at the same moment resolve in a documented,
  deterministic order — no outcome is left to scheduling chance (with P6).
- **Low, bounded latency.** The command path (player action → authoritative, persisted effect) is
  designed to complete well under a second under expected load; the due-event scheduler fires events
  with minimal jitter relative to their due time.
- **Performance-aware by default.** Hot read/write paths use efficient queries, indexing, and caching
  of hot state from the outset; full scans and N+1 access are avoided. A slice touching a hot path
  states a latency/throughput **budget** and tests against it.
- **Never at the cost of correctness.** Speed never overrides server authority (P4), reproducibility
  (P2), or determinism (P6); it is achieved within them.
*Rationale: in this genre, the milliseconds are the gameplay.*

## Amending this document

Edit a principle here and add a changelog entry. Never let code or a spec quietly contradict a
principle that still stands.

## Changelog

- **v1 (2026-06-10)** — Initial constitution.
- **v2 (2026-06-10)** — Added **P11** (performance & timing precision as first-class), reflecting that
  sub-second timing is game-impacting.
