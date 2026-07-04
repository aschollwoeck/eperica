# The bot system — how it works & how to run it

Eperica's AI players are **true clients**: they play through the same server-authoritative Agent
API any key-holder could use, driven by the `eperica-bots` runner. Nothing bot-related lives in the
game rules — a bot can never see or do more than a human player (ADR 0036).

```
/admin (seed fleet) ──▶ agents.json manifest ──▶ eperica-bots runner
                                                      │  one digest poll per bot tick
                                                      ▼
                     Agent API (/api/…) ◀── bearer key: epk_<id>_<secret>
                          │ same use-cases, same fog of war, same rate limits
                          ▼
                       the game
```

## Server side (slices 118–120)

- **Keys**: `Authorization: Bearer epk_<id>_<secret>` — minted in `/admin` (single or bulk), bound
  to `is_ai` accounts only, SHA-256-hashed at rest, shown exactly once, revocable. The full wire
  contract (digest shape, all actions, error codes) is [docs/agent-api.md](../agent-api.md).
- **The digest** (`GET /api/w/{world}/state`) is the bot's entire perception: own villages, queues,
  garrison, culture, incoming attacks (arrival-only — fog holds), own movements/reinforcements,
  report heads, research state. Everything a page shows the player, nothing more.
- **Rate budget**: 120 API requests/minute per key (429 + `retry_after_secs` beyond).
- **Carve-outs** (so fleets don't break the meta): detection signals skip AI accounts (moderators
  see the AI badge instead; humans are never flagged by IP-association with bots); the inactivity
  sweep spares **enabled** bots (revoke keys ⇒ the bot decays like a quit player); beginner
  protection applies normally; bots are full ranking/medal participants.
- **Visibility** is per world: `labeled` (NPC tags) or `disguised` (indistinguishable).

## Runner side (`crates/bots`, slices 121–122)

A separate binary; keeps **no state** the server doesn't have (restart-safe — every tick starts
from a fresh digest). Modules: `client` (HTTP), `digest` (defensive DTOs), `persona`, `policy`
(pure decisions), `executor`, `runner` (fleet loop), `strategy` + `strategist` (LLM layer).

### Personas — the difficulty knob

Derived deterministically from the bot's username (FNV-1a): activity window (start hour, 8–16 h
length), tick interval band (180–900 s, jittered), aggression 0–3, raid range 5–10 tiles. No bot
plays 24/7 or reacts instantly.

### The reflex doctrine (every tick, pure, in priority order)

1. **Evacuate/recall** — imminent attack + a second village ⇒ move the garrison out, recall later.
2. **Storage** — any store ≥ 90 % capacity ⇒ Warehouse/Granary.
3. **Fields** — lowest level first, crop-biased when crop net < 25/h, up to level 10.
4. **Core buildings** — MB→3, Barracks→3, Warehouse→3, Granary→3, MB→5, Academy→1, Residence→10
   (prereq-consistent with the classic preset), once fields average level 2.
5. **Training** — tier-1 infantry up to the floor `10 + 10·aggression`.
6. **Settling** — Residence 10 + culture allows ⇒ train 3 settlers, settle the nearest free valley.
7. **Raiding** — aggression ≥ 1 and garrison above `15 + 5·aggression` ⇒ raid up to `aggression`
   `(inactive)`-labeled villages in range, never drawing below the floor.

The server is the referee: a 409 denial is a normal outcome, logged and skipped — never retried in
the same tick. 429 backs the bot off (never below its own minimum interval); 401 retires it.

### The LLM strategist (optional)

With an Anthropic key, each bot periodically (default 4 h, budget-capped fleet-wide) sends a
compact fog-honest summary and receives a **validated strategy**: focus (economy/military/
expansion), aggression override, raid/settle quadrant preferences, a motto — and at most one
in-character DM per cycle. The strategy *biases* the reflexes; invalid replies are rejected loudly
(the prior strategy persists — no silent fallbacks). Without a key the runner is byte-identical to
reflex-only. Cost at defaults: ≤ 12 haiku-tier calls/hour ≈ cents/day.

## Run book

1. `/admin` → **AI agents** → *Seed bots* (world, count ≤ 50, tribe mix) → copy/download the
   one-time `agents.json`.
2. ```bash
   cargo run --release -p eperica-bots -- \
     --server https://your-host \
     --world <world-uuid> \
     --keys agents.json
   ```
3. Optional strategist: `ANTHROPIC_API_KEY=… ` (or `EPB_ANTHROPIC_KEY`), tune `--llm-budget`,
   `--llm-interval-secs`, `EPB_LLM_MODEL`.
4. Watch: the world's leaderboard shows the fleet (NPC-tagged on labeled worlds); the runner logs
   one line per tick per bot. Stop with ctrl-c (drains in-flight ticks).

| Flag | Env | Default | |
|---|---|---|---|
| `--server` | `EPB_SERVER` | – | required |
| `--world` | `EPB_WORLD` | – | required (UUID) |
| `--keys` | `EPB_KEYS` | – | required (manifest path) |
| `--dry-run` | `EPB_DRY_RUN` | off | log intents, no writes |
| `--open-window` | `EPB_OPEN_WINDOW` | off | ignore activity windows (demo/ops) |
| `--tick-secs N` | `EPB_TICK_SECS` | persona | fixed interval (demo/tests) |
| `--cap N` | `EPB_CAP` | 4 | max concurrent bot ticks |
| `--no-llm` | `EPB_NO_LLM` | off | force strategist off |
| `--llm-budget N` | `EPB_LLM_BUDGET` | 12 | LLM calls per rolling hour, fleet-wide |
| `--llm-interval-secs N` | `EPB_LLM_INTERVAL_SECS` | 14400 | per-bot strategist cadence |
| – | `EPB_ANTHROPIC_KEY` / `ANTHROPIC_API_KEY` | – | enables the strategist |
| – | `EPB_LLM_MODEL` | `claude-haiku-4-5-20251001` | model id |

### Troubleshooting

- **Bot dropped at startup** — dead/revoked key or no player in the target world; the fleet
  continues without it (check the startup log).
- **Lots of 409s in the log** — normal; the doctrine asks and the server refuses (insufficient
  resources, busy lanes). Bots don't duplicate balance math by design.
- **429s** — the fleet is out-pacing the per-key budget; raise tick intervals or lower `--cap`.
- **`strategist disabled (no API key or --no-llm)`** — expected without a key; reflexes run fully.
- **Nothing happens at night** — personas have activity windows; `--open-window` overrides for
  demos.

Deeper reading: ADR 0036 (architecture), `specs/features/118–122` (spec/plan per slice),
`crates/bots/README.md` (the short operator card).
