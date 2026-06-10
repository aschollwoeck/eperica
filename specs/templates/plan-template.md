# Feature NNN — <Title> — Technical Plan

**Spec:** ./spec.md

## Constitution check

Note how this plan satisfies the relevant principles (esp. P1 lazy time, P3 pure domain,
P4 server-authoritative, P7 configurable speed). Flag any tension.

## Domain model

Entities, value objects, and the pure rules involved. No I/O here.

## Persistence

Tables / EF entities, what is stored vs. computed-on-read (P1/P2).

## Services / application layer

Commands and how due-events are produced and processed.

## Interface (UI / API)

What the client sees and posts. Server stays authoritative (P4).

## Test strategy

How the acceptance criteria map to tests (domain unit tests first).
