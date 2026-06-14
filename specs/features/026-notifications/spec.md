# Feature 026 — Notifications & alerts

**Status:** Verified
**Depends on:** 009 (combat — incoming attacks + battle reports), 024 (the SSE + `LISTEN/NOTIFY` live bus, the nav-badge poll pattern), 016/025 (the profile/identity the bell sits beside), 001 (auth/sessions)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Communication → "Notifications / alerts") — incoming-attack warnings, report arrival, and message arrival, delivered live with a nav bell + read/unread.

## Goal

Tell a player, promptly and durably, when something that needs their attention happens: an **incoming
attack** on one of their villages, a new **battle report**, and a new **direct message**. Notifications
are **persisted** (a feed the player can revisit, P2/P6), **server-authoritative** (P4), and delivered
**live** over the existing 024 stream so a bell badge updates without a reload — while a missed live
nudge never loses a notification (it's a row; the next page load shows it).

## Concepts

- **Notification.** A persisted, per-player record that an event occurred: `kind`, an optional reference
  (the village / report / sender it points at), a `created_at`, and a per-recipient `read_at` (null =
  unread). It is a **log of outcomes already committed** — created at the exact point its triggering event
  is persisted, never recomputed or invented (P1: discrete outcomes, recorded once).

- **Kinds (this slice).**
  - **IncomingAttack** — recorded for the **defender** when an attack/raid is launched at one of their
    villages (knowable immediately, the moment the movement is persisted). Points at the target village +
    carries the arrival time.
  - **BattleReport** — recorded for the **attacker** and **each defender** when a battle resolves and its
    report is written (the scheduler-driven combat resolution). Points at the report.
  - **NewMessage** — recorded for the **recipient** when a direct message is sent. Points at the
    conversation.

- **Delivery.** Persist-then-notify in one statement (the row is truth; `pg_notify` is the live nudge),
  reusing the 024 architecture: a per-process `PgListener` on a `notifications` channel republishes to an
  in-process broadcast hub; an SSE endpoint streams a **single player's** notifications, routed by a
  **per-player key** (`notif:<player-uuid>`) so one player can never receive another's (P4). A nav **bell**
  shows the unread count, refreshed by a poll (as the 024 message badge does) and nudged live.

- **Read state.** Opening the notifications feed marks its entries read (advancing `read_at`); the unread
  count is the player's notifications with `read_at IS NULL`. A player may also mark all read explicitly.

## Acceptance criteria

> All generation + read state is server-authoritative (P4) and reproducible from persisted rows (P2/P6).
> A notification is created exactly once, in the same transaction that commits its triggering event.

- **AC1 — Incoming-attack notification.** When a player launches an attack or raid at a village, the
  **defending owner** gets an `IncomingAttack` notification referencing the target village and the arrival
  time. The attacker does not notify themselves. (A player attacking their **own** village — e.g. between
  their villages — produces no alarming alert.)

- **AC2 — Battle-report notification.** When a battle resolves, the **attacker** and **each distinct
  defending participant** (owner + reinforcers) get a `BattleReport` notification pointing at their report.
  Created in the same transaction as the report (so it is never orphaned).

- **AC3 — New-message notification.** When a direct message is sent, the **recipient** gets a `NewMessage`
  notification pointing at the conversation. The sender does not notify themselves.

- **AC4 — Persisted feed + unread count.** A logged-in player has a notifications feed (most-recent first)
  and an unread count. The count is the player's `read_at IS NULL` notifications. Both are reads over
  persisted rows (P1).

- **AC5 — Read state.** Viewing the feed marks the shown notifications read; the unread count drops
  accordingly. Marking is **owner-scoped** — a player only ever reads/clears their own notifications (P4).

- **AC6 — Live delivery.** A new notification reaches an open session's bell **live** (within ~a second)
  over SSE, routed by a per-player key so no player receives another's. A dropped live nudge loses nothing:
  the notification is persisted and appears on the next load (P5 — DB is truth).

- **AC7 — Roles.** Notifications are **private to their recipient**: only the logged-in owner can list,
  count, or clear them; a Visitor has none. No client action creates a notification for another player or
  reads another's (P4).

- **AC8 — Reproducibility & config.** Notifications are persisted and recomputed on read (P1/P2). The feed
  page size is bounded (P11). Any retention/poll cadence is config, not hardcoded wall-clock gameplay (P7).

## Roles & permissions

Per [roles.md](../../roles.md). Notifications are strictly per-recipient private reads.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — (no notifications; redirected to login). | List/clear any notifications. |
| **Player** | List / count / mark-read **their own** notifications; receive their own live. | Read, clear, or stream another player's notifications; create one for another player. |
| **Moderator/Administrator** | (as Player) for their own. | — |

## Out of scope

- **Completion alerts** (build / training / movement-arrival / trade-delivery): higher-volume,
  per-scheduler-tick events — a deferred follow-up; this slice covers the attention-critical trio
  (incoming attack, report, message).
- Per-kind **notification preferences** / muting / email digests — handled by the later Settings slice.
- Scout-report and trade-specific notifications, alliance/diplomacy events, grouping/threading of the feed,
  and desktop/push (web-push) notifications — future work.
