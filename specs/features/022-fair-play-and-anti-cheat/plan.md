# Feature 022 — Fair play & anti-cheat tooling — Plan

**Spec:** ./spec.md · **Status:** Reviewed

How the spec maps onto the layered architecture (P3 dependency rule). Reuses 019 (abandoned-login block +
activity tracking), 021 (the round-freeze guard), 016 (the account/stats surface), and 001 (auth/sessions,
config, the world clock). New surface: the Moderator role, the report/review/sanction loop, DB-backed rate
limiting, and the two detection signals.

## Domain (pure, P3) — `crates/domain/src/fairplay.rs`

- `enum SanctionKind { Warn, Suspend, Ban }` and `enum ReportReason { Pushing, Botting, Abuse, Other }`
  (+ string round-trips for persistence/forms).
- `fn account_blocked(banned_at: Option<Timestamp>, suspended_until: Option<Timestamp>, now) -> bool`
  — banned ⇒ always; suspended ⇒ `now < until`. The single source of "is this account blocked now" (P1),
  used by both login and the action guard.
- Detection predicates: `fn shared_ip_flagged(association_count: u32, rules) -> bool` (≥ a config
  sensitivity), `fn inhuman_action_rate(max_window_count: u32, rules) -> bool` (≥ a config threshold).
- `struct FairPlayRules { rate_limit_per_window: u32, rate_window_secs: i64, login_limit_per_window: u32,
  suspend_default_secs: i64, ip_association_threshold: u32, inhuman_rate_threshold: u32 }` (P7).
- Unit tests: block transitions (banned/suspended/expired/none), the two detection predicates at the
  threshold boundary, reason/kind round-trips.

## Balance (P7) — `specs/balance/fairplay.toml` + `infrastructure/balance.rs`

- `fairplay.toml` carries the rate window/limits, suspension default, and detection thresholds; a
  `fair_play_rules()` loader returns `FairPlayRules`. Re-exported from the infrastructure crate.

## Persistence (migration `0034`)

- `users` += `is_moderator boolean`, `suspended_until timestamptz`, `banned_at timestamptz`,
  `registration_ip text` (the detection key; captured at register).
- `reports (id uuid pk, world_id, reporter_id → users, subject_id → users, reason text, note text,
  created_at, status text /* open|resolved */, resolved_by → users, resolved_at, resolution text)` —
  partial-unique on `(reporter_id, subject_id) WHERE status = 'open'` so duplicates collapse (AC2).
- `rate_limits (subject text, action text, window_start timestamptz, count int, pk (subject, action,
  window_start))` — fixed-window counters; the action tallies also feed the inhuman-rate signal.

## Application (use-cases + ports)

- **`ModerationRepository`** port (default no-ops so non-moderation fakes are untouched):
  `set_moderator`, `is_moderator`; `file_report`, `open_reports`, `resolve_report` (+ optional sanction in
  one tx); `apply_sanction`; `ip_association_count`, `peak_action_count`; `bump_rate` (atomic upsert +
  read). `UserRecord`/`find_user_by_*` gain the sanction fields + `is_moderator` (read-folded).
- **`crates/application/src/fairplay.rs`** use-cases:
  - `file_report` (reject self-report; collapse duplicates; persist open).
  - `review_queue` (moderator-gated list of open reports).
  - `resolve_report` (moderator-gated; resolve + optional sanction, idempotent).
  - `account_signals` (moderator-gated; `ip_association_count` + `inhuman_action_rate` via the rules).
  - `check_rate_limit` (bump the window counter; `Err(RateLimited)` when over the config limit).
  - `ModerationError` (NotAuthorized, SelfReport, NotFound, RateLimited, Backend).
- **Auth (019 pattern):** `authenticate` gains a `LoginError::Sanctioned` when `account_blocked` (banned or
  unexpired suspension), checked after the abandoned block.

## Infrastructure (web)

- **Rate-limit middleware** (`from_fn_with_state`): for mutating `POST`s, call `check_rate_limit` keyed by
  the session player (or IP for `/login`); on `Err(RateLimited)` return **429**. Auth reads pass.
- **Sanction action guard:** extend the freeze-guard chokepoint (021) so a **sanctioned** logged-in player's
  mutating `POST`s are rejected (the world-won freeze and the per-account block share one layer).
- **IP capture:** `register_submit` records `registration_ip` from `X-Forwarded-For` (proxy) falling back to
  the peer `ConnectInfo` — `serve` switches to `into_make_service_with_connect_info`.
- **Moderator pages:** `/mod` review queue (open reports, oldest first); `/mod/account/{id}` inspect
  (sanctions + signals + resolve/sanction forms); all moderator-gated. A **report** action on the public
  player-stats page (016). Sanctioned-login + 429 are surfaced.
- **Moderator bootstrap:** at startup, mark the `MODERATORS` env usernames `is_moderator` (idempotent).
- `AppState` carries `FairPlayRules`.

## Reuse / no new path

- Sanction blocking reuses the **login-block** (019) and **freeze-guard** (021) chokepoints — no parallel
  enforcement path. Detection reads existing data (`users.registration_ip`, the `rate_limits` tallies) — no
  heavy event log. The report→review→sanction loop is ordinary CRUD behind moderator gating.

## Risks / decisions

- **Rate limiting is DB-backed** (fixed window) to stay stateless/horizontally-scalable (P5) rather than a
  process-local bucket; the extra write is one indexed upsert per mutating action.
- **Detection is advisory** (P10) — signals never auto-sanction; a human moderator always decides, keeping
  false positives harmless.
- **Moderator bootstrap via env** is the minimal operator path; a full admin console is explicitly later.
