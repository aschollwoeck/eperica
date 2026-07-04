# Feature 124 — graceful shutdown on SIGTERM

**Status:** Draft
**Depends on:** 001 (server bootstrap), 037 (world registry drain).
**Origin:** documentation accuracy review of PR #141 — the binary handles only Ctrl-C (SIGINT),
so the DEFAULT stop signal of systemd and Docker (SIGTERM) kills the process without draining
in-flight scheduler work, contradicting the intended deployment story.

## Goal

`SIGTERM` triggers the same graceful shutdown as Ctrl-C: signal the watch channel, drain the
per-world schedulers, exit. Operators no longer need `KillSignal=SIGINT`/`STOPSIGNAL SIGINT`.

## Acceptance criteria

- **AC1 — Both signals drain.** On unix, SIGINT **or** SIGTERM resolves the shutdown future
  (first one wins); the existing drain path (`registry.join_all`) runs unchanged. Non-unix
  builds keep ctrl-c-only (conditional compilation).
- **AC2 — No behaviour change otherwise.** Startup, serving and the drain itself are untouched.
- **AC3 — Docs follow.** The installation manual's signal note is updated by the in-flight docs
  PR once this merges (KillSignal workaround no longer required, kept as historical note only
  for older builds).

## Roles & permissions

Per [roles.md](../../roles.md): n/a — process lifecycle only.

## Out of scope

Windows service semantics; a configurable drain timeout (the drain is already bounded by design).
