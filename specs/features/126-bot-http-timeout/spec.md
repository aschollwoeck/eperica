# Feature 126 — bot runner: HTTP timeouts (no hung fleet)

**Status:** Verified (reviewer APPROVE at 4034cf9 — both must-fixes closed: stray blob rebuilt out of history, strategist client bounded too)
**Depends on:** 121 (ApiClient, fleet semaphore).
**Origin:** live incident (2026-07-04): a running fleet went silent for 20 h (the runner process
died without a trace — cause unattributed). The post-mortem found a genuine freeze hole
regardless: `reqwest::Client::new()` has NO default timeout, so any hung request (e.g. a server
restart under the fleet) would hold a semaphore permit forever — with `--cap` such requests the
whole fleet freezes silently. This slice closes that hole.

## Goal

A bot HTTP call can never hang the fleet: every request has a bounded lifetime; a timed-out
request is a normal Transient outcome (retry next tick per the existing error doctrine).

## Acceptance criteria

- **AC1** — the ApiClient is built with explicit timeouts (connect ~5 s, total ~30 s); builder
  failure is a loud startup error, not a fallback.
- **AC2** — a timeout classifies as `Transient` (existing `classify` path: logged, bot reschedules,
  permit returned). Unit test: `ApiFailure::Http` from a timeout error maps to Transient.
- **AC3** — no behavioural change otherwise (dry-run byte-identical; e2e suite green).

## Roles & permissions

n/a — runner-internal robustness (no server change).

## Out of scope

Retry-with-backoff inside a single tick; watchdog liveness metrics (log-only runner stays).
