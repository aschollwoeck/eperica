# Administering an Eperica server

Everything an operator does happens in the browser: `/admin` (Administrator role) and `/mod`
(Moderator role). Roles are granted via the `ADMINS`/`MODERATORS` env vars at startup or by an
existing admin in the console; an admin can never remove their own admin role.

## The admin console (`/admin`)

### Server & world status

Read-only panel: home-world speed/radius/seed, account & village counts, pending scheduler events,
artifact/Wonder release times, win state — plus a list of **all worlds** (name, UUID, speed,
radius, created, running / won-frozen).

### Creating worlds

`POST /admin/world` (the form):

| Field | Meaning |
|---|---|
| **Name** | Display name (lobby, nav) — max 60 chars |
| **Speed** | Multiplier (1, 3, 5, …; min 0.1). Scales production and timers (P7) — but *not* crop upkeep (faithful, slice 114) |
| **Radius** | Map radius in tiles |
| **Artifacts / Wonder (days)** | The end-game schedule for this world (defaults from env) |
| **Preset** | `classic` or `speed` — a complete balance directory (`specs/balance/presets/<name>/`: economy, construction, units, combat, culture, …). Worlds on different presets genuinely play by different rules (ADR 0035) |
| **AI visibility** | `labeled` (AI players carry an "NPC" tag on boards, stat pages and the map) or `disguised` (indistinguishable from humans). Cosmetic; moderators always see the truth |

A new world's scheduler starts **live** — no restart. Players join it from the lobby (`/worlds`),
choosing a tribe per world.

Archiving = the world freezing machinery (a won world stops accepting game actions); there is no
separate archive button — the Wonder victory or direct database intervention (no admin UI for
this) freezes a world.

### Account administration

Search accounts, toggle **Moderator**/**Administrator** per account (`POST /admin/role`). Retired
(abandoned) accounts are labeled.

### AI agents (fleet seeding)

- **Seed bots** (`POST /admin/agents`): world, count (1–50), tribe mix (even round-robin or a fixed
  tribe). Names come from a built-in medieval pool with numeric suffixes on collision. The response
  shows a **one-time key manifest** — copyable and downloadable as `agents.json` — which is exactly
  the bot runner's key file. Only hashes are stored; the manifest cannot be re-shown.
- **Single agent** (`POST /admin/agent`): username + world + tribe, one key shown once.
- **Fleet table**: every bot with tribe, world, created, enabled state; per-bot **Revoke keys** and
  per-world **Revoke all**. Revocation selects bots by world but disables the bot's **account**
  keys — a revoked bot stops playing in *every* world it inhabits, and re-enters the normal
  inactivity lifecycle (greys, then decays like a quit player).

AI accounts are full participants (rankings, medals, alliances). They are exempt from the
*detection signals* below (they'd trivially trip them) but fully subject to rate limits; players
can still report them (preserves the disguise on `disguised` worlds — the moderator sees the AI
badge and judges).

## Spectators (slice 125)

**Spectator** is an additive account role (like Moderator/Administrator) that grants a read-only,
omniscient view of **every** world — every village's internals and every movement in flight, with
compositions, unblinded by fog of war. There is no per-world flag: the grant is global and the
grant *is* the trust decision.

- **Granting/revoking the role** — the accounts table on `/admin` gets a **Spectator** column
  next to Moderator/Administrator: toggle it per account (`POST /admin/role`, `role=spectator`).
  Unlike admin, there is no self-removal restriction — an operator can freely grant or remove
  their own spectator access.
- **Minting spectator keys** — the *Spectator keys* panel mints a key by username
  (`POST /admin/spectator-key`), shown **once** as `spk_<id>_<secret>` (SHA-256 hashed at rest,
  same pattern as agent keys but a distinct table and prefix — the two credential types can never
  be confused or cross-authenticate). A holders list shows every account with an active key, with
  a per-account **Revoke keys** action (`POST /admin/spectator-key/revoke`) that revokes all of
  that account's spectator keys at once. Minting a key before the role is granted is allowed — the
  key simply won't authenticate until the role is in place (see below).

> **The fog caveat.** The Spectator role is additive to Player: an account can hold both. A
> spectator who also plays **sees through fog of war everywhere — including worlds they play on
> themselves.** Grant this role to neutral observers, casters, and tournament referees — never to
> an active competitor. This is an operator judgement call; the system enforces no per-world
> exclusion.

- **Role-revoke instantly dead-ends keys.** A spectator key only authenticates while its account
  holds the Spectator role at the moment of the request — auth re-checks the role on every
  request, not just at mint time. Removing the role from an account makes every one of its keys
  fail immediately (`401`), with nothing further to revoke by hand.
- **Keys don't cross surfaces.** A spectator key (`spk_`) is refused on the Agent API, and an
  agent key (`epk_`) is refused on the spectator surface — the two prefixes are parsed and
  verified independently, so a key minted for one purpose can never authenticate the other.

## Moderation (`/mod`)

- **Queue**: open player reports (subject, reason, note, reporter) with inline resolution —
  Dismiss / Warn / Suspend / Ban.
- **Account view** (`/mod/account/<id>`): ban/suspension state, an **AI badge** for `is_ai`
  accounts, and the advisory **detection signals** (never auto-sanction):
  - shared registration-IP association (flags at ≥ 3 accounts on one IP; AI accounts excluded from
    the count),
  - inhuman action rate (flags at ≥ 120 player actions per 60 s window).
- Sanctions are enforced server-side at the login block and action guards; suspension default is
  1 day.

## Fair-play limits (process-global, `specs/balance/fairplay.toml`)

| Limit | Value |
|---|---|
| Player mutating actions | 60 / 60 s window |
| Login attempts per IP | 10 / window |
| Agent-API requests per bot key (all methods) | 120 / window |
| Suspension default | 86 400 s |

Over-limit requests get 429; agents additionally receive `retry_after_secs` in the JSON body.
