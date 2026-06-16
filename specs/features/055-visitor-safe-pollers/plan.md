# Plan — 055 visitor-safe pollers

A one-commit bug fix; the spec's **Design** section is the plan. Two tasks:

- **T1 — handlers.** Swap the four base-template pollers off redirecting auth extractors:
  `sitting_status` (`RealUser`→`MaybeRealUser`, empty for a visitor), `messages_unread` /
  `notifications_unread` (`AuthUser`→`MaybeAuthUser`, `"0"`), `notifications_stream`
  (`AuthUser`→`MaybeAuthUser`, `204 No Content`). Logged-in branches unchanged.
- **T2 — tests.** Add the visitor regression test; update the one 026 test that asserted the old
  anon-redirect contract. Full gate + reviewer.

## Gates

`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace`, P11. Plus a headless render to confirm the landing page no longer dumps markup.
