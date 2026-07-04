# eperica-bots — Bot Fleet Runner

A headless bot fleet that plays Eperica using the agent API (docs/agent-api.md).

## Quick start

### 1. Seed bots in the admin panel

Log in as an Administrator, go to **/admin → AI agents** and use **Seed bots** to create a fleet.
Copy or download the one-time `agents.json` manifest — it contains username + API key pairs and is
shown exactly once (only hashes are stored server-side).

### 2. Run the fleet

```bash
cargo run -p eperica-bots -- \
  --server http://localhost:8080 \
  --world <world-uuid> \
  --keys agents.json
```

Replace `<world-uuid>` with the UUID shown in the admin world list (or the `EPB_WORLD` env var).

> **TLS:** `reqwest` is built with `rustls-tls` (needed for the strategist's Anthropic API calls),
> so both `http://` and `https://` server URLs work.

### 3. Verify with --dry-run

```bash
cargo run -p eperica-bots -- \
  --server http://localhost:8080 \
  --world <world-uuid> \
  --keys agents.json \
  --dry-run
```

Dry-run logs all intents the bots would execute but makes no HTTP POST calls.

## All flags

| Flag              | Env var        | Default | Description                              |
|-------------------|----------------|---------|------------------------------------------|
| `--server <URL>`  | `EPB_SERVER`   | —       | Server base URL (required)               |
| `--world <UUID>`  | `EPB_WORLD`    | —       | Target world UUID (required)             |
| `--keys <PATH>`   | `EPB_KEYS`     | —       | Path to `agents.json` (required)         |
| `--dry-run`       | `EPB_DRY_RUN`  | false   | Log intents, make no HTTP POSTs          |
| `--tick-secs <N>` | `EPB_TICK_SECS`| —       | Fixed tick interval in seconds (no jitter; testing only) |
| `--cap <N>`       | `EPB_CAP`      | 4       | Max concurrent bot ticks                 |

Log level: `RUST_LOG=debug cargo run -p eperica-bots -- …`

## The LLM strategist (slice 122)

With an Anthropic API key configured, each bot periodically (default every 4 h, jittered) sends a
compact, fog-honest summary of its situation to the LLM and receives a **strategy**: a focus
(`economy` / `military` / `expansion`), an optional aggression override, raid/settle quadrant
preferences, a motto — and optionally **one** in-character message to another player per cycle.
The strategy biases the reflex doctrine between cycles; the reflexes keep playing regardless.

- Enable: set `ANTHROPIC_API_KEY` (or `EPB_ANTHROPIC_KEY`). Without a key — or with `--no-llm` —
  the runner behaves **exactly** like the reflex-only fleet.
- `EPB_LLM_MODEL` — model id (default `claude-haiku-4-5-20251001`, the cheap tier).
- `--llm-budget N` / `EPB_LLM_BUDGET` — fleet-wide LLM calls per rolling hour (default 12).
- `--llm-interval-secs N` — per-bot strategist cadence (default 14400 = 4 h).
- Invalid LLM replies (prose, fences, out-of-range values) are **rejected loudly** and the prior
  strategy stays — no silent fallback, ever.
- Cost: at the defaults, a fleet makes ≤ 12 haiku-tier calls/hour with ~1–2 KB prompts — cents/day.

## Stopping

Press **Ctrl-C**.  The runner stops scheduling new ticks and waits for all in-flight ticks to
complete before exiting.

## How bots behave

Each bot derives a deterministic *persona* from its username (activity hours, tick cadence,
aggression level, raid radius).  On each tick the bot fetches its state digest, computes a list of
intents using the pure reflex doctrine (evacuate before incoming attacks → storage relief →
fields ⇄ core buildings → training → settling → raiding inactives), executes them via the agent
API (409 denials are normal — the server is the referee), and sleeps until the next tick.

See `specs/features/121-bot-runner/plan.md` for the full doctrine and design.
