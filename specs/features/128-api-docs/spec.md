# Feature 128 — developer API reference at /docs/api (swagger-style + OpenAPI)

**Status:** Verified (reviewer APPROVE at 81bf2ee; core fact-check clean — 12+ endpoint examples diffed field-exact against the live serializers, all nine error mappers verified)
**Depends on:** 118/119 (Agent API), 125 (Spectator API), 127 (public docs pipeline precedent).
**Origin:** operator request — the API contracts live only in the repo; serve them on the site,
"swagger-like", with real per-endpoint examples.

## Goal

`/docs/api` is a **public, swagger-style API reference** for the two bearer surfaces (Agent API,
Spectator API): every endpoint grouped and expandable with method, path, auth, parameters,
request/response schemas and **concrete examples** (JSON bodies + a copyable `curl`), plus the
shared error contract and rate budgets — and the same registry exports a valid **OpenAPI 3.0
document** at `/docs/api/openapi.json` for Swagger UI/Postman/codegen users.

## Concepts

- **One registry, two renderings.** A Rust endpoint registry (compile-time, in `web`) is the
  single source: group, method, path, summary, description, auth kind, path/query parameters,
  request example, response example(s) with status codes, error codes. The HTML page and the
  OpenAPI JSON are both generated from it — they cannot drift from each other.
- **No undocumented endpoints — by test.** A coverage test compares the registry against the
  actual route sets of the agent and spectator routers; a route added without documentation
  fails CI (and vice versa for ghosts).
- **Swagger-like, native.** The page mirrors the Swagger UI mental model — sidebar groups,
  method badges (GET/POST), collapsible operations, schema/parameter tables, example panes —
  rendered by Askama in the game theme with `<details>` interactions. No vendored JS framework;
  external tooling gets the OpenAPI file instead.
- **Examples are honest.** Response examples come from the documented contracts and are pinned
  where practical against the same values the integration tests assert (e.g. the error body
  shape, a digest fragment, a spectator feed row). Bearer tokens in examples are obvious
  placeholders (`epk_key-id_secret…`).
- **The markdown contracts remain the narrative.** `docs/agent-api.md` / `docs/spectator-api.md`
  keep the prose semantics (fog rules, budget philosophy, manifest lifecycle) and gain pointers
  to `/docs/api`; the registry holds the per-endpoint reference. The site page links both ways;
  the manual's Reference section and the site footer link to `/docs/api`.

## Acceptance criteria

- **AC1 — Public routes.** `/docs/api` (the reference) and `/docs/api/openapi.json` render
  without login; both linked from the footer and the manual's Reference section.
- **AC2 — Complete coverage.** Every route registered on the `/api` and `/spectator` routers
  appears in the registry with method, auth, summary, at least one response example, and error
  codes; the coverage test enforces the equality both directions.
- **AC3 — Real examples.** Every endpoint shows a copyable `curl` (correct method, path,
  auth header, body where applicable) and at least one JSON response example; POST endpoints
  show a request-body example. The shared error contract (400/401/403/404/409/429 +
  `{error, reason}`, `retry_after_secs`) is documented once centrally and per-endpoint where
  specific.
- **AC4 — Valid OpenAPI.** `/docs/api/openapi.json` parses as JSON, declares `openapi: 3.0.x`,
  `info`, `servers`, and one path item per registry entry with operation, parameters, request
  body and response objects (schemas may be permissive `object` types where the registry has
  only examples — declared honestly via `example` fields). A test asserts structural validity
  (required OpenAPI keys, every registry path present).
- **AC5 — Swagger-like UX.** Sidebar with the two API groups (+ sections), method badges,
  collapsible operations (default collapsed), parameter/response tables, monospace example
  panes, the auth scheme explained at the top of each group. Game-theme CSS only.
- **AC6 — P11.** Everything compile-time/static per request (registry in memory; OpenAPI JSON
  serialized once at startup or per request from memory — no I/O, no DB beyond standard
  middleware).

## Roles & permissions

Per [roles.md](../../roles.md): public read (Visitor), like the manual. No mutating routes; the
documented APIs keep their own auth unchanged.

## Out of scope

- Vendoring Swagger UI / interactive "try it out" calls from the browser (CORS/auth surface —
  external tools can use the OpenAPI file).
- Documenting session (browser) routes or the admin console.
- API versioning/changelog machinery.
