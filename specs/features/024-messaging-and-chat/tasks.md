# Feature 024 — Communication: conversations — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

WhatsApp-style conversations (DMs + group channels), persisted + live. Pure-domain first; each task a
commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Sends reuse the 021/022
guards — no new enforcement path.

## Domain

- [ ] **T1 — Comms domain (`domain/comms.rs`; P3).** `ChatChannel` (+ key round-trips),
  `can_access_channel`, `valid_body` (trim + cap, no subjects). **Unit tests:** access rule, validation,
  round-trips (AC1, AC5).

## Persistence & use-cases

- [ ] **T2 — Schema + `CommsRepository` + use-cases (migration `0035`).** `direct_messages`,
  `chat_messages`, `conversation_reads`. Repo: `send_dm`/`dm_history`/`dm_threads`,
  `post_chat`/`chat_history`, `mark_read`/`unread_after`/`total_unread`. Use-cases (`send_dm`/`send_chat`
  with validation + recipient/channel access gates, `conversation_list`, `open_*` + mark-read,
  `unread_badge`); `CommsError`; `MessageView`/`ConversationSummary`. **DB tests:** DM send + two-party
  history; self/unknown/abandoned rejected; channel access (member vs non-member; global open); unread +
  mark-read; conversations list ordering (AC1–AC5).

## Realtime

- [ ] **T3 — SSE + `LISTEN/NOTIFY` live delivery.** `send_dm`/`post_chat` persist + `pg_notify('comms', …)`
  (DM notifies both viewer-relative keys); per-process `PgListener` → `ChatHub` broadcast; `GET
  /messages/stream/{key}` (SSE) backfills history then streams; `AppState` carries the hub; `main.rs` + the
  test harness start the listener. **Integration test:** an SSE client receives a posted message live + it
  persisted; an inaccessible key is rejected (AC6, AC8).

## Interface

- [ ] **T4 — Web: conversations list + conversation view + send + profile entry.** `/messages` (list,
  recency + previews + unread), `/messages/c/{key}` (history + live + send; marks read), `POST
  /messages/send`, `/messages/with/{player}` (open DM), nav unread badge, “Message” on player stats.
  **Integration tests:** DM appears for both parties; opening clears unread + advances the badge; a
  non-member alliance channel is forbidden; a non-party DM key is rejected (AC2–AC5, AC9).

## Acceptance

- [ ] **T5 — Docs + review.** rustdoc on new public items; `docs/architecture/0026-communication.md`
  (conversation model + SSE/`LISTEN/NOTIFY` design + P5 note); `docs/manual/` communication chapter;
  `CLAUDE.md` active slice → 024. Full gates + P11; `eperica-reviewer` until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests (incl. the live SSE round-trip), all gates green, both docs written, reviewer
**APPROVE**, PR merged, `spec.md` / `plan.md` **Verified**, roadmap note updated.
