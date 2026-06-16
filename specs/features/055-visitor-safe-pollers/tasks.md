# Tasks — 055 visitor-safe pollers

- [x] **T1** — `sitting_status` `RealUser`→`MaybeRealUser` (empty for visitor); `messages_unread` /
  `notifications_unread` `AuthUser`→`MaybeAuthUser` (`"0"` for visitor); `notifications_stream`
  `AuthUser`→`MaybeAuthUser` (`204` for visitor).
- [x] **T2** — Integration test `visitor_background_pollers_do_not_leak_login_html` (each poller returns a
  short, non-HTML body to a visitor; logged-in unchanged); update the 026 test
  `notifications_feed_bell_and_privacy` to the new visitor-safe contract. Full gate + reviewer → APPROVE.

Gates: `fmt --check`, `clippy -D warnings`, `cargo test --workspace`, P11.
