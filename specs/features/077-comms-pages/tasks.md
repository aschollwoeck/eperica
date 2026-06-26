# Tasks — 077 the communication pages redesign

Gated by fmt/clippy/`cargo test --workspace`/P11. Presentation only; branch `feature/077-comms-pages`.

- [x] **T1 — CSS.** Restyle `.notifications` as cards w/ unread accent; add `.conversations` cards, `.messages`
  chat bubbles (`.mine`/`.theirs`/`.sender`), the `.send`(/`--inline`) form, `.preview`. Reuse `.phead`/`.page`.
- [x] **T2–T6 — Templates.** `.phead` + the new components on notifications, messages, conversation (SSE JS
  kept + matched to the bubble markup), forum, forum_thread; all routes/POSTs/links preserved.
- [x] **T7 — Tests.** Assert `phead` + the comms classes (`conversations`/`messages`) on the inbox, chat,
  forum, thread, and notifications, keeping the existing content/flow assertions.
- [ ] **T8 — Gate + reviewer + PR.** fmt/clippy/`cargo test --workspace` green; Playwright; reviewer APPROVE; PR.
