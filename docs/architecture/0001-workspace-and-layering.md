# Workspace & layering

**Status:** Current
**Date:** 2026-06-10 · **Slice:** 001

## Context
The constitution requires a pure game core with no I/O (P3) and a stateless, scalable tier (P5).

## Design
A Cargo workspace of four crates with a strict dependency direction:

```
domain  ←  application  ←  infrastructure  ←  web
```

- **domain** — pure entities, value objects, and rules. **Zero dependencies**; the compiler forbids it
  from importing I/O crates, guaranteeing P3.
- **application** — use-cases plus **ports** (traits like `AccountRepository`, `EventStore`,
  `PasswordHasher`). Depends only on `domain`. Use-cases are tested against in-memory fakes.
- **infrastructure** — adapters implementing the ports with SQLx/Postgres, argon2, etc. All I/O here.
- **web** — Axum HTTP layer (lib + bin); `lib::router` is exposed so integration tests drive the real
  stack.

## Consequences
- Game rules are unit-testable with no DB; adapters are swappable.
- IDs are `u128` newtypes in the domain and mapped to UUIDs in infrastructure, keeping the domain
  dependency-free.
- Slightly more mapping code at the infra boundary (DTOs ↔ domain types) — an acceptable cost.

## Links
specs/constitution.md (P3, P5); crates/*.
