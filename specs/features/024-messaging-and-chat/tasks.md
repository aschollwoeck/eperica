# Feature 024 — Communication: messaging & chat — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered pure-domain-first; each task is a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass
before advancing. Mail lands first (durable CRUD), then the chat persistence + access rules, then the
realtime layer, then UI. Reuses the 021/022 send-time guards — no new enforcement path.

## Domain

- [ ] **T1 — Comms domain (`domain/comms.rs`; P3).** `ChatChannel` (+ string round-trips),
  `can_access_channel(channel, membership)`, `valid_message`/`valid_chat` (trim + length caps).
  **Unit tests:** access rule (global vs alliance member/non-member), validation bounds, round-trips (AC1, AC5).

## Mail (persistent)

- [ ] **T2 — `messages` schema + `MessageRepository` + use-cases (migration `0035`).** Send (validate
  recipient exists/not-abandoned/not-self), inbox/sent (per-side, newest first), `message_by_id` (party
  guard), `mark_read`, `delete_for` (per-side), `unread_count`. `MailError`. **DB tests:** send + inbox/sent;
  self/unknown/abandoned rejected; read sets read_at + unread count; per-side delete; non-party access
  rejected (AC1–AC4).

- [ ] **T3 — Mail web UI.** `/messages` (inbox), `/messages/sent`, compose (GET+POST), `/messages/{id}`
  (view, marks read), `/messages/{id}/delete`; unread badge in nav; “message this player” from player
  stats. **Integration tests:** send → appears in recipient inbox + sender sent; opening marks read;
  delete hides from one side only; a non-party 404s (AC2–AC4, AC9).

## Chat (persistent + access)

- [ ] **T4 — `chat_messages` schema + `ChatRepository` + access use-case (migration `0035`).** `post_chat`
  (access-gated via `can_access_channel` + the sender's alliance), `chat_history(channel, limit)`.
  `CommsError`. **DB tests:** post + history; a non-member alliance post/read rejected; global open to all
  (AC5, AC6 persistence half).

## Realtime

- [ ] **T5 — SSE + `LISTEN/NOTIFY` live delivery.** `post_chat` persists + `pg_notify`; a per-process
  `PgListener` task → `ChatHub` broadcast; `GET /chat/stream` (SSE) backfills history then streams the
  player's accessible channels; `POST /chat/send`. `AppState` carries the hub; `main.rs` + the test harness
  start the listener. **Integration test:** an SSE client receives a posted chat message live + the row
  persisted; an inaccessible channel is rejected (AC6, AC8).

## Acceptance

- [ ] **T6 — Docs + review.** rustdoc on new public items; `docs/architecture/0026-communication.md`
  (mail + the SSE/`LISTEN/NOTIFY` design + P5 note); `docs/manual/` communication chapter; `CLAUDE.md`
  active slice → 024. Full gates + P11; `eperica-reviewer` on the slice diff; fix until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests (incl. the live SSE round-trip), all gates green, both docs written, reviewer
**APPROVE**, PR merged, `spec.md` / `plan.md` **Verified**, roadmap note updated.
