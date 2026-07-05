# Tasks — 128 the developer API reference

**Status:** Draft. Gates per task: fmt, clippy -D warnings, cargo test --workspace.

- [x] **T1 — Registry + OpenAPI.** apidocs.rs (both surfaces fully entered, examples as JSON
  literals), openapi_json(); unit invariants + AC4 structural tests. (AC2 data, AC4)
- [x] **T2 — The page.** /docs/api + /docs/api/openapi.json handlers/routes; swagger-style
  template + CSS; footer/manual/markdown-contract links; AC1/AC3/AC5 integration tests +
  the AC2 route-coverage test. (AC1, AC2, AC3, AC5, AC6)
- [ ] **T3 — Review & accept.** Reviewer APPROVE (incl. example-vs-serializer diffing);
  statuses flipped; merged when Verified.
