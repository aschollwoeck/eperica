# Plan — 128 the developer API reference

**Status:** Draft (spec approved)

## Constitution check

- **P4:** documentation only; the documented surfaces keep their auth. No client input beyond
  the public GET.
- **P11:** the registry is a compile-time constant; the OpenAPI document is built from it in
  memory (serde_json) — zero I/O per request beyond standard middleware.

## Module changes

| Layer | Change |
|---|---|
| `web/src/apidocs.rs` (new) | the endpoint registry: `ApiGroup { name, auth_blurb, endpoints }`, `Endpoint { method, path, summary, description, auth, params: [(name, kind, desc)], request_example: Option<&str>, responses: [(status, desc, example)], errors: [(status, code, when)] }` — Agent API (from the `/api` router: me, w/{world}/state, map, build, train, research, smithy, attack, scout, reinforce, return, trade, settle, message) + Spectator API (me, feed, players, village). Examples as inline JSON string literals (pretty-printed); `curl` assembled by the template. `openapi_json() -> serde_json::Value` generated from the same structs |
| `web/src/handlers.rs` | `docs_api` (HTML) + `docs_api_openapi` (JSON) — public router |
| `web/templates/docs_api.html` | swagger-style: sidebar (groups → operations), method badges, `<details>` per operation, parameter/response tables, example panes with the curl line; auth explainer per group; error-contract section |
| `web/static/base.css` | `.apidoc__*` styles (method badges GET/POST colors, panes) — theme tokens |
| docs | agent-api.md/spectator-api.md gain a "Served interactively at /docs/api" pointer; manual Reference section + site footer gain the link |

## Key decisions

- **Native swagger-style + OpenAPI export** (no vendored Swagger UI): the house is JS-light and
  CSP-tight; external tooling consumes `/docs/api/openapi.json`.
- **Coverage test via route-list comparison**: the agent/spectator routers' path sets are
  asserted equal to the registry's (maintained constants beside the routers if introspection is
  impractical — with a comment binding them; the test is the drift alarm either way).
- **Examples pinned to tested truth where cheap**: the error body, `/api/me`, a state-digest
  fragment, and a spectator feed row reuse the exact shapes integration tests assert.

## Test strategy

- Unit (`apidocs.rs`): registry invariants (unique method+path, every endpoint ≥1 response,
  POSTs have request examples, all example strings parse as JSON); `openapi_json()` structural
  validity (AC4 keys, every path present).
- Integration: AC1 (public 200 ×2 + links present); AC2 coverage test; AC3 spot-checks (curl
  line for attack; the 429 contract block; a known response example fragment).

## Risks

- **Registry↔handler drift in examples**: mitigated by pinning the high-traffic shapes to the
  same fragments integration tests assert; the reviewer diffs the rest against serializers.
- **OpenAPI strictness**: we target structural 3.0 validity (spec-required keys), not full
  schema modeling — declared in the doc itself ("schemas are example-driven v1").
