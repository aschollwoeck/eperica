# eperica-bots — Bot Fleet Runner

A headless bot fleet that plays Eperica using the agent API (docs/agent-api.md).

## Quick start

### 1. Seed bots in the admin panel

Log in as an operator, go to **Admin → Bot fleet** and use **Seed bots** to create a set of
bot accounts.  Download the resulting `agents.json` file — it contains username + API key pairs.

### 2. Run the fleet

```bash
cargo run -p eperica-bots -- \
  --server http://localhost:8080 \
  --world <world-uuid> \
  --keys agents.json
```

Replace `<world-uuid>` with the UUID shown in the admin world list (or the `EPB_WORLD` env var).

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

## Stopping

Press **Ctrl-C**.  The runner stops scheduling new ticks and waits for all in-flight ticks to
complete before exiting.

## How bots behave

Each bot derives a deterministic *persona* from its username (activity hours, tick cadence,
aggression level, raid radius).  On each tick the bot fetches its state digest, computes a list of
intents using the pure reflex doctrine (field upgrades → core buildings → training → settling →
raiding), executes them via the agent API, and sleeps until the next tick.

See `specs/features/121-bot-runner/plan.md` for the full doctrine and design.
