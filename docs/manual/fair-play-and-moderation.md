# Fair play & moderation

Eperica is a competitive world, and fairness is enforced by the **server** — never the client. On top of
that foundation sit the tools that keep a world clean: reporting, moderation, rate limiting, and
cheat-detection signals.

## Reporting a cheat

If you believe an account is breaking the rules — **pushing / multi-accounting**, **botting**, **abuse**,
or anything else — open that player's stats page and use **Report this account**. Pick a reason, add an
optional note, and submit. Your report goes to a moderator's review queue. (You can't report yourself, and
filing the same report twice doesn't spam the queue.)

## Sanctions

Moderators review reports and can apply a **sanction**:

- **Warn** — a recorded warning, no restriction.
- **Suspend** — a temporary block; you can't log in or act until it lifts (it expires on its own).
- **Ban** — a permanent block.

A suspended or banned account **cannot log in**, and if already logged in **cannot take game actions** —
the server rejects them. You can still see the world; you just can't play until the sanction lifts.

## Rate limiting

To stop spammers and bots, the server **rate-limits** actions: too many actions in a short window are
rejected (you'll see a "too many requests" response — just slow down). Login attempts are likewise limited
to blunt password-guessing. Normal play never hits these limits.

## For moderators

Moderators (designated by the operator) get two extra pages:

- **`/mod`** — the **review queue** of open reports, oldest first. Each can be resolved, optionally with a
  warn/suspend/ban in the same step.
- **`/mod/account/{id}`** — **inspect** an account: its current sanction status and two **detection
  signals** — how many accounts share its registration IP, and whether its action rate looks inhuman.

The signals are **advisory** — they're inputs to your judgement, never automatic punishments. You always
decide.

## Behind the scenes

Everything here is server-authoritative and reproducible: a sanction is a simple state on the account
(checked on every login and action), the detection signals are computed from stored data on demand, and
the limits and thresholds are all configurable by the operator.
