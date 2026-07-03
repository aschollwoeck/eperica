# Plan — 119 Agent API actions complete

**Status:** Draft (spec approved)

## Constitution check

- **P1:** nothing scheduled; all new reads are compute-on-read via existing page read models; all
  responses carry absolute-ms times. The AC8 test drives the existing due-combat processor directly
  (the test-suite precedent), not a new tick.
- **P3:** zero domain change. Every endpoint is a JSON adapter over an existing application
  use-case; error enums map to codes in the web layer.
- **P4:** the use-cases keep full authority (garrison coverage, tribe scoping, protection, party
  scoping on reports); the adapters add only strict village addressing (118) and username→account
  resolution for DMs. No new authorization logic.
- **P7:** untouched — arrival/completion times come from the speed-aware use-cases.
- **P11:** every endpoint is O(1)-ish over existing queries; the digest additions reuse the exact
  village-page reads (`active_movements`, `reinforcements_at/of`, `researched_units`,
  `unit_levels`, `active_unit_orders`). All under the 118 agent budget.

## Decisions

1. **Wire format for bundles.** JSON maps, not form-field prefixes: units as
   `"units": { "<unit_id>": count, … }` (zero/absent filtered like the rally handler), resources as
   `"give": { "wood": n, "clay": n, "iron": n, "crop": n }`. First JSON bodies were introduced in
   118; this extends the same style.
2. **Scout takes a units map** (spec table said `{count}`): the scout unit is tribe-specific, so the
   agent names it like every other send — `order_scout`'s `NotAllScouts` stays the authority. Spec
   patched to match.
3. **Recall by host village id.** `order_return` identifies a stationed group by its **host**
   village; the digest's `reinforcements_abroad` entries carry `host_village`, and
   `POST …/return { "host": "<village-uuid>" }` feeds it straight through. (The path `{village}` is
   the agent's own home village per strict addressing; the use-case validates ownership of the
   group.)
4. **Reports.** `GET /api/w/{w}/report/{id}` → the existing party-scoped `report(id, player)`
   (`None` → 404, P4). Scout reports are their own model: `GET /api/w/{w}/scout-report/{id}` →
   `scout_report(id, player)` (the target's view arrives pre-redacted — 010's rule). The digest's
   battle-report heads gain `kind`; a `scout_reports` head list (id, occurred-at, viewer_is_scouter,
   detected) is added.
5. **Messages key by ACCOUNT id** (024/045: comms are cross-world account-level; the browser passes
   `ctx.account`). The adapter does the same: `POST /api/w/{w}/message { "to": "<username>", "body" }`
   resolves the recipient via `find_user_by_username` → `send_dm(account, recipient_account, …)`;
   unknown username → 404. Reads reuse the page models: `GET /api/w/{w}/messages` →
   `conversation_list` summaries (key, title, last, unread), `GET /api/w/{w}/messages/{account}` →
   `open_dm` history (marks read, the page's own semantics — no new "since" read exists, spec Open
   Question resolved).
6. **Error codes.** Same contract as 118 — each new enum maps to snake_case codes with
   `e.to_string()` reasons: Combat (`insufficient`, `empty_composition`, `no_target`, `same_tile`,
   `target_protected`, `invalid_catapult_target`, `not_found`), Scout (+`not_all_scouts`), Movement
   (+`nothing_stationed`), Trade (`no_marketplace`, `empty_bundle`, `not_enough_merchants`, …),
   Settle (`not_settler_group`, `no_slot`, `not_free_valley`, …), Research (`in_progress`,
   `already_researched`, `requirements_unmet`, …), Smithy (+`not_researched`, `no_smithy`,
   `max_level`, `smithy_level_too_low`), Comms (`invalid`, `self_send`, `recipient_unavailable`,
   `forbidden`). Rule denials 409; unknown ids/parties 404; malformed 400; `Backend`/`Conflict` per
   118 (500 / 409 `conflict`).
7. **Oasis flows stay out** (spec §Out of scope): the attack adapter passes coordinates to
   `order_attack` only — an oasis tile yields the use-case's own denial; the rally page's oasis
   branch is not adapted in 119.

## Interface (all under the 118 bearer auth + agent budget + strict addressing)

| Endpoint | Body | → |
|---|---|---|
| `POST …/village/{v}/attack` | `{x, y, units, mode: "attack"\|"raid", catapult_target?}` | `order_attack` |
| `POST …/village/{v}/scout` | `{x, y, units, target: "resources"\|"defenses"}` | `order_scout` |
| `POST …/village/{v}/reinforce` | `{x, y, units}` | `order_reinforcement` |
| `POST …/village/{v}/return` | `{host: "<village-uuid>"}` | `order_return` |
| `POST …/village/{v}/trade` | `{x, y, give}` | `order_trade` |
| `POST …/village/{v}/settle` | `{x, y}` | `order_settle` |
| `POST …/village/{v}/research` | `{unit}` | `order_research` |
| `POST …/village/{v}/smithy` | `{unit}` | `order_smithy_upgrade` |
| `GET  …/report/{id}` · `GET …/scout-report/{id}` | — | party-scoped views |
| `POST …/message` · `GET …/messages` · `GET …/messages/{account}` | `{to, body}` | `send_dm` / `conversation_list` / `open_dm` |

Send successes return the created movement read back through `active_movements` (kind, destination,
arrival); research/smithy return the `ActiveUnitOrder`; trade returns the shipment arrival.

**Digest additions (page truth):** per player — `movements` (`active_movements`),
`reinforcements_abroad` (`reinforcements_of`: host village/coord/owner + troops),
`scout_reports` heads; per village — `reinforcements_here` (`reinforcements_at`), `research`
(`researched_units` + `unit_levels` + `active_unit_orders`); battle-report heads gain `kind`.

## Test strategy

- **Unit:** error-code mapping per enum variant (table-driven).
- **Integration:** per-endpoint success + the distinctive denial classes (empty bundle,
  not-all-scouts, protection, no marketplace, not-settler-group, already-researched, self-send,
  unknown recipient); report/scout-report party scoping (non-party → 404); digest additions equal
  direct read-model calls (the 118 M4 pattern).
- **AC8 (flagship):** two agents; A raids B (A's digest: movement + garrison drop; B's: arrival-only
  incoming); drive `process_due_combat`/`process_due_movements` (suite precedent); both read their
  party views of the same report id; A reinforces B (B's `reinforcements_here` shows it, A's
  `reinforcements_abroad` too); A recalls by host id; groups clear after the due return. Pure JSON.

## Tasks

See [tasks.md](tasks.md). Gates every task: `cargo fmt --all -- --check`,
`clippy --all-targets -- -D warnings`, `cargo test --workspace`, P11.

## Key risks

- **AC8 timing:** movements need due-processing in-test; mitigated by the suite's existing
  `process_due_*` direct-call precedent (no sleeps, deterministic).
- **DM identity confusion (multi-world):** comms key by account id while game actions key by player
  id — the adapter must pass `ctx.account` to comms and `ctx.player` everywhere else; called out
  in code comments + tested via the two-agent DM exchange.
- **Unit-map deserialization:** counts must reject negatives/overflow cleanly (u32 via serde) and
  filter zeros like the rally handler.
