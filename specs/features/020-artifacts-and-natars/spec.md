# Feature 020 — Artifacts & Natar villages

**Status:** Verified
**Depends on:** 006 (the map's reserved **Natar** tiles), 009/011 (combat — the attack that captures), 014 (the won-attack resolution path capture hooks into), 004/002 (units, production, capacity, upkeep — the sim hooks artifacts modify), 012 (oases — the template for an NPC target whose effect folds into the read), 001/013 (world clock + Treasury-bearing villages)
**Roadmap:** M7 · slice 020 · GDD §11.3 (step 1) / §7.1 — the **end-game escalation**: at a configured date **artifacts** are released, held in **Natar** (NPC) villages, and **captured by attacking** them. Artifacts grant powerful bonuses to the holding village/account and can be **stolen** between players. (The Wonder of the World + victory is slice 021.)

## Goal

A finite round needs a scripted end-game. Slice 020 delivers its **first phase**: when the world reaches
its **artifact-release date**, the reserved Natar tiles **materialize into NPC Natar villages**, each
**holding one artifact**. An artifact grants a powerful, faithful **bonus** (faster troops, bigger stores,
cheaper upkeep, faster training, tougher buildings, sharper/【blinder】 scouting, …) to whoever holds it.
A player **captures** an artifact with a **winning attack** launched from a village that has a **Treasury**
(a new building) — and can likewise **steal** an artifact by beating a current holder. This turns the
late game into a contest over a handful of decisive, mobile power-ups, setting up the Wonder race (021).

Everything is **server-authoritative** (P4) and **reproducible** from persisted state + the seeded world
(P2/P6); the client never releases, captures, or applies an artifact.

## Concepts

- **Natar villages (NPC).** At the release date the world's reserved `Natar` tiles **materialize** into NPC
  villages with a strong, **seeded** defensive garrison (deterministic from the world seed, P6). They are
  **not ownable** — they are artifact vaults. One Natar village is created **per artifact** to be released.

- **Artifacts (the full faithful set).** Each artifact has a **type** and a **scope**:
  - **Types** (8, GDD §11.3): **Speed** (troop movement), **Storage** (warehouse/granary capacity),
    **Sustenance** (reduced troop crop upkeep), **Trainer** (faster troop training), **Architect**
    (building durability — less catapult/siege damage), **Eyes** (sharper scouting — offence),
    **Confuser** (harder to scout — defence), and **Fool** (a chaotic artifact — a **seeded** effect, see
    Decisions).
  - **Scopes:** **small** (affects only the **holding village**), **large** (affects **all** the holder's
    villages — account-wide), and **unique** (account-wide, **strongest**, and **one per type per world**).
  - The released set, types, scopes, and effect magnitudes are **config** (P7).

- **Effects fold into the read (like oasis bonus).** An artifact's effect is **not** a stored mutation; it
  is an **aggregated modifier** computed from the holdings and applied where the sim reads — troop speed in
  travel time, capacity in the economy read, upkeep in net crop, training time, siege damage, scouting
  power/defence. A village's effective modifiers are the combination of its own (small) artifacts and its
  account's (large/unique) artifacts. Removing/losing an artifact removes the modifier on the next read
  (P1 — no ticking, no migration of state).

- **Treasury (new building).** Holding an artifact requires a **Treasury** in the holding village (GDD §6).
  A village's Treasury must be at the artifact's required **level** (config; higher for large/unique). A
  village holds **at most one** artifact (one Treasury, one vault).

- **Capture & steal (defeat-and-claim).** A **winning attack** (any attack/raid that the attacker wins)
  **launched from a village with a qualifying Treasury** captures an artifact:
  - against a **Natar village** that still holds its artifact ⇒ the artifact **moves to the attacking
    village** (the Natar village remains as a now-empty vault);
  - against a **player village** that holds an artifact ⇒ the artifact is **stolen** to the attacking
    village.
  The transfer happens **in the battle-resolution transaction** (P2/P4), exactly once. If the attacker's
  village cannot hold it (no/low Treasury, or already holds an artifact), the attack resolves normally but
  **no artifact transfers** (it stays put).

- **Release schedule (world clock).** The **artifact-release date** is world config (GDD §13.2). Release is
  a **one-time due event** at that wall-clock instant (not speed-scaled — a fixed calendar date, like the
  017 settlement cadence): it materializes the Natar villages + their artifacts, exactly once.

## User stories

- As a **player**, when the end-game opens I want **artifacts to appear in Natar villages** so there's a
  clear new objective beyond growth.
- As a **player**, I want to **capture an artifact by force** (a winning attack from my Treasury village)
  and gain its bonus, and to **steal** one from a rival the same way.
- As a **player**, I want an artifact's **bonus to actually change the game** (my troops faster, stores
  bigger, etc.), scoped to the right villages.
- As an **administrator**, I want the **release date, the artifact set, scopes, magnitudes, and Treasury
  requirements** to be **config** (P7).

## Acceptance criteria

> Release, capture, theft, and effects are **server-authoritative** (P4) and **reproducible** from
> persisted state + the seeded world (P2/P6). The schedule, set, and magnitudes are **config** (P7).

- **AC1 — Release at the configured date.** Before the artifact-release instant there are no Natar villages
  or artifacts. At/after it, a one-time release **materializes** the configured artifacts, each in a Natar
  village with a seeded garrison, **exactly once** (re-running the release is a no-op).

- **AC2 — Natar villages are NPC vaults.** A materialized Natar village has a deterministic defensive
  garrison and is **not ownable**/settleable; it exists to hold (and, once emptied, to have held) an
  artifact.

- **AC3 — Treasury gates holding.** An artifact can be held only in a village with a **Treasury** at the
  artifact's required level; a village holds **at most one** artifact. The Treasury is a buildable building
  (cost/time/prereqs are config).

- **AC4 — Capture by winning attack from a Treasury.** A winning attack launched from a qualifying Treasury
  village against a **Natar village holding an artifact** moves that artifact to the attacking village, in
  the battle-resolution transaction, exactly once. Without a qualifying/empty Treasury (or if the attacking
  village already holds one), the battle resolves but no artifact transfers.

- **AC5 — Theft between players.** A winning attack from a qualifying Treasury village against a **player
  village that holds an artifact** transfers (steals) that artifact to the attacker, under the same rules
  as AC4.

- **AC6 — Effects apply by scope, on the read.** A held artifact's effect modifies the sim where it is
  read: **small** → only the holding village; **large**/**unique** → all the holder's villages. The wired
  effects are troop **Speed** (travel time), **Storage** (capacity), **Sustenance** (upkeep), **Trainer**
  (training time), **Architect** (siege/building durability), **Eyes** (scouting power), **Confuser**
  (scouting defence), and **Fool** (its seeded effect). Losing an artifact removes its effect on the next
  read (no stored mutation, P1).

- **AC7 — Reproducibility & config.** Natar garrisons and any seeded artifact behavior are deterministic
  from the world seed (P2/P6); the same release produces the same villages/garrisons/artifacts. The
  release date, artifact catalogue (types/scopes/magnitudes), and Treasury requirements are config.

- **AC8 — Interface.** Natar villages appear on the **map**; a player can see the **artifacts they hold**
  (type, scope, effect, holding village) on their view, and an artifact's presence/where it is, is public
  enough to be contested (its holder/location is visible, faithful to §7.3). No client action releases,
  captures, or applies an artifact (P4).

## Roles & permissions

Per [roles.md](../../roles.md). Artifacts are **earned by force**; release + transfer are **System**.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A — may see public Natar/artifact locations on the map (no account). | Capturing/holding anything. |
| **Player** | **Capture/steal** artifacts by winning attacks from a qualifying Treasury village; **hold** them (Treasury-gated) and receive their **scoped effects**; see their own holdings + public artifact locations. | Releasing artifacts; capturing without a winning attack / without a qualifying Treasury; holding more than one per village; mutating effects or another player's holdings client-side. |
| **Moderator** | N/A (considered) — no artifact moderation surface this slice. | — |
| **Administrator** | Configure (P7) the release date, artifact catalogue (types/scopes/magnitudes), Natar garrison strength, and Treasury requirements. | Granting/moving artifacts per-request from the client. |
| **System** | *(system-initiated)* **Release** the artifacts at the configured date (materialize Natar villages + garrisons + artifacts), **transfer** artifacts on a qualifying winning attack, and **aggregate** effects on read — all server-side, exactly once, deterministic. | — |

## Out of scope

- **Wonder of the World, Wonder building plans, and the victory condition + round archival** — slice
  **021** (this slice only releases/captures artifacts and applies their bonuses).
- **Natar offensive behavior** (Natars attacking players, Natar build-up) — Natar villages here are static
  defensive vaults; their wider AI is not in scope.
- **Conquering/owning Natar villages** — capture is **defeat-and-claim** of the artifact (Decided), not
  ownership transfer of the Natar village.
- **Great Warehouse/Granary & other end-game building plans** — not artifacts; deferred.
- **New combat math** — capture rides the existing won-attack resolution; artifacts only add a transfer
  step and read-time modifiers.

## Decisions

- **Capture is defeat-and-claim, Treasury-gated (chosen).** A winning attack from a qualifying Treasury
  village claims a Natar village's artifact (or steals a player's), transferring it in the battle
  transaction; the Natar village is **not** owned. (The alternative — conquering Natar villages via
  administrators — was rejected as disproportionately heavy.)
- **The full Travian artifact set (chosen):** 8 types × {small, large, unique}, effects **data-driven** and
  folded into the existing sim reads (oasis-bonus pattern) rather than mutating state. Each effect injects a
  multiplier at exactly one existing hook (travel time, capacity, upkeep, training time, siege damage,
  scouting power, scouting defence).
- **Fool artifact** — modeled deterministically (P6): its effect is a **seeded** pick from the other
  effects, fixed at release per artifact id (no per-tick randomness). Documented as the faithful-but-
  reproducible interpretation of Travian's "random" Fool.
- **Treasury** — a new `BuildingKind::Treasury` with config cost/time/prereqs; an artifact's scope sets the
  **required Treasury level** (small lowest, unique highest); **one artifact per village**.
- **Release is a one-time wall-clock due event** (not speed-scaled), via the generic `scheduled_events`
  queue (`EventKind::ArtifactRelease`) seeded at world creation for the configured date; idempotent.
- **Effect aggregation** — a pure `ArtifactEffects` computed from a village's own (small) + its account's
  (large/unique) holdings, exposing the per-hook multipliers; the read paths combine it with existing
  factors (speed, Smithy, oasis bonus).
- **Persistence** — `artifacts` (catalogue instances: type, scope, the Natar village/coords they release
  in) and a `holding` (which village holds which artifact, or the Natar vault). Natar villages persist in a
  `natar_villages` table (coords + seeded garrison), distinct from player `villages`.

## Open questions

- **Artifact effect magnitudes** — exact multipliers per type/scope are interim balance, tunable in
  `artifacts.toml`; chosen to be impactful but not game-breaking (e.g. Speed large ≈ +100% beyond a
  threshold; Storage ≈ ×1.5–×3; Sustenance ≈ −25%). **Proposed:** seed faithful-ish values, refine later.
- **Where the released Natar villages sit** — the reserved `Natar` tiles are sparse + seeded; **Proposed:**
  pick the first N reserved Natar tiles in deterministic ring order (like starting-village placement), N =
  the artifact count.
- **Eyes/Confuser exact mapping** — scouting power vs revealing incoming attacks. **Proposed:** map to
  scouting **power** (Eyes) and scouting **defence** (Confuser) in `resolve_scouting`, the cleanest hooks.
