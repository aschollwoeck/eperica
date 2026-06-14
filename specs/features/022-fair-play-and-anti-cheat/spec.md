# Feature 022 — Fair play & anti-cheat tooling

**Status:** Reviewed
**Depends on:** 016 (ranking/stats — the account surface signals attach to), 019 (the abandoned-login block + activity throttle — the enforcement + tracking patterns), 021 (the round-freeze guard — the action-rejection pattern), 001 (auth/sessions, world clock, config)
**Roadmap:** M8 · slice 022 · GDD §12.5 — the **enforcement surface** for fair play: a **Moderator** role, player **reporting → review → sanction**, server-side **rate limiting**, and reproducible **multi-account / bot detection signals**.

## Goal

The constitution already makes the game *fair* (P4 server authority, P2/P6 reproducibility); this slice
adds the **policy/enforcement surface** on top (GDD §12.5, built progressively — P10). It introduces a
**Moderator** role and the tooling staff need to keep a competitive world clean:

- **Players report** suspected cheats; reports land in a **moderator review queue**.
- **Moderators sanction** accounts — **warn / suspend / ban** — and the server **enforces** sanctions
  (a banned/suspended account cannot log in or act).
- **Rate limiting** guards action frequency server-side (anti-spam / anti-bot / brute-force).
- **Detection signals** — **shared registration-IP association** and an **inhuman action-rate flag** —
  are computed deterministically from persisted state and surfaced to moderators.

Everything is **server-authoritative** (P4) and **reproducible** from persisted state (P2/P6). Thresholds
and limits are **config** (P7). The fair-play *rules* (sanction state, detection predicates) are **pure
domain** (P3); sanction state is **computed on read** (P1 — a suspension simply expires).

## Concepts

- **Moderator role (elevated, additive to Player).** A new account capability (`users.is_moderator`) per
  [roles.md](../../roles.md). Moderator-only actions (review queue, inspect, sanction) are **server-gated** —
  a Player is denied. The first moderators are designated by the **operator** via config (a `MODERATORS`
  list applied idempotently at startup, P7) — an Administrator concern; a full admin console is later work.

- **Report (player → subject).** A player files a report against another account with a **reason** (a fixed
  set: pushing/multi-account, botting, abuse, other) and an optional note. Self-reports are rejected; a
  duplicate **open** report by the same reporter against the same subject is collapsed (no queue spam).

- **Review queue.** Open reports, **oldest first**, visible only to moderators. A moderator **resolves** a
  report — recording who/when and an outcome — optionally **applying a sanction** to the subject.

- **Sanction (warn / suspend / ban).** A moderator action on an account: **warn** (a recorded note, no
  block), **suspend** (a block until a timestamp — speed-independent wall-clock; the operator sets the
  default window), or **ban** (a permanent block). Sanction state lives on the account; whether an account
  is **blocked now** is a pure read of `banned_at` + `suspended_until` vs. now (P1).

- **Sanction enforcement (server-side).** A **blocked** account (banned, or suspended and not yet expired)
  **cannot log in** (mirrors the 019 abandoned block) and **cannot perform mutating actions** within a live
  session (mirrors the 021 freeze guard). Reads/login surface the reason. Enforcement is the server's, never
  the client's (P4).

- **Rate limiting (server-side, DB-backed).** Each mutating action and each login attempt is counted in a
  **fixed time window** per subject (player id, or IP for pre-auth login); exceeding the configured limit is
  **rejected** (HTTP 429). DB-backed counters keep the web tier stateless + horizontally scalable (P5). The
  windows/limits are config (P7).

- **Detection signals (reproducible, moderator-facing).** Computed on demand from persisted state, never
  stored as verdicts: **(a) shared registration-IP association** — how many accounts registered from the
  same IP (`users.registration_ip`); **(b) inhuman action-rate flag** — derived from the rate-limit action
  tallies, raised when a window's action count exceeds a (higher) **inhuman** threshold. Signals are
  **advisory inputs to a human**, not automatic sanctions.

## User stories

- As a **player**, I want to **report** an account I believe is cheating, so staff can act.
- As a **moderator**, I want a **review queue** of open reports and the ability to **inspect** an account
  (its sanctions + detection signals) and **sanction** it.
- As a **player**, I want **rate limiting** and server validation so spammers/bots can't flood the world,
  and I want to know if my own account is **sanctioned** (and why).
- As an **administrator**, I want **moderators**, the **rate limits**, the **suspension default**, and the
  **detection thresholds** to be **config** (P7).

## Acceptance criteria

> Reporting, review, sanction, enforcement, rate limiting, and detection are **server-authoritative** (P4)
> and **reproducible** from persisted state (P2/P6). Roles, limits, and thresholds are **config** (P7).

- **AC1 — Moderator role.** A new `is_moderator` capability, additive to Player. Moderator-only actions
  (review queue, account inspect, sanction) are **rejected** server-side for a non-moderator. Operators
  designate moderators via config, applied idempotently at startup.

- **AC2 — Reporting.** A player can file a report against **another** account with a reason (+ optional
  note); the report is persisted as **open**. A **self-report** is rejected, and a duplicate **open** report
  by the same reporter against the same subject does not create a second row.

- **AC3 — Review queue.** A moderator can list **open** reports, **oldest first**. A non-moderator listing
  is rejected.

- **AC4 — Sanctioning.** A moderator can **resolve** a report and, in the same action, apply a sanction —
  **warn**, **suspend(until)**, or **ban** — recording the **acting moderator**, **when**, and the outcome.
  Resolving an already-resolved report is a no-op (idempotent).

- **AC5 — Sanction enforcement.** A **banned** account, or a **suspended** account whose suspension has not
  expired, **cannot log in** and **cannot perform mutating game actions**; the block lifts automatically
  when a suspension expires (computed on read). A **warn** does not block. Enforcement is server-side.

- **AC6 — Rate limiting.** Mutating actions and login attempts exceeding the configured **per-window limit**
  for a subject are **rejected** (429); within-limit requests pass. The window/limit are config.

- **AC7 — Detection signals.** For an account, a moderator can see **(a)** the count of accounts sharing its
  **registration IP** and **(b)** an **inhuman-action-rate flag**, both computed **deterministically** from
  persisted state. Signals are advisory and never auto-sanction.

- **AC8 — Reproducibility & config.** Sanction state, the review-queue order, and both detection signals are
  deterministic from persisted state (P2/P6). The moderator list, rate-limit window/limit, suspension
  default, and detection thresholds are config (P7). All enforcement is server-authoritative (P4).

- **AC9 — Interface.** A moderator **review-queue** page and **account-inspect** page (sanctions + signals +
  resolve/sanction actions); a player **report** action; sanctioned-login and rate-limit rejections are
  surfaced to the user. No client action sanctions, resolves, or bypasses a limit (P4).

## Roles & permissions

Per [roles.md](../../roles.md). Reporting is **Player**; review/inspect/sanction are **Moderator**;
moderator designation + limits/thresholds are **Administrator/operator** config.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — | Everything (must be a logged-in player to report). |
| **Player** | Report another account; see **their own** sanction status + the reason. | Review queue; inspect others' sanctions/signals; sanction; self-report; bypass rate limits. |
| **Moderator** | Review queue; inspect any account (sanctions + signals); resolve reports; apply warn/suspend/ban. | Configure/operate worlds; change balance. |
| **Administrator** (Operator) | Designate moderators; set rate limits, suspension default, detection thresholds (config). | — (superset; bounded by audit). |

## Out of scope

- A full **Administrator console** (world create/configure/start/archive) — operators use config + the DB.
- **Sitting limits / vacation mode** (GDD §12.4) — a later social/meta feature.
- Automatic sanctions from detection signals — signals are **advisory** only; a human always acts.
- Appeals workflow, audit-log UI, IP geolocation, and ML-based bot classification — future hardening.
