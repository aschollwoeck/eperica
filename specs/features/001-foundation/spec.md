# Feature 001 — Foundation & Skeleton

**Status:** Reviewed
**Depends on:** none (this is the first slice)
**Roadmap:** M1 · slice 001 · GDD §3, §13

## Goal

A visitor can **register an account, log in, and see their single starting village** in a running,
**speed-configured** world. This slice exists to stand up the project's architecture on the smallest
possible behavioral surface: the layered domain (P3), database-as-source-of-truth with a stateless
web tier (P5), server-authoritative actions (P4), **configurable game speed** (P7), and the
**due-event scheduler** skeleton (P1). Everything later in the roadmap rides on what is proven here.

## User stories

- As a **visitor**, I want to register an account, so that I can start playing.
- As a **registered user**, I want to log in and log out securely, so that only I can act on my account.
- As a **new player**, I want a starting village created for me automatically, so that I have a
  foothold in the world the moment I join.
- As the **operator**, I want each world to run at a configurable speed, so that we can offer worlds at
  different paces without changing code.

## Acceptance criteria

> Written to become tests. "The domain" = the pure game-logic layer (P3). "Server-side" = not
> performed or trusted from the client (P4).

- **AC1 — Registration.** Given valid, unique registration details, when the visitor submits them,
  then a new account is created and persisted. Given details that are invalid or duplicate an existing
  account, then registration is **rejected with a clear error** and **no** account is created. When
  **email confirmation is enabled** (production default), the account must be confirmed before login;
  when **disabled** (dev default), the account can log in immediately. (See Decisions.)

- **AC2 — Login / logout.** Given an existing account, when correct credentials are submitted, then a
  session is established and the user is authenticated. Given incorrect credentials, then login is
  rejected. When the user logs out, the session ends.

- **AC3 — Exactly one starting village.** Given a player who has just registered, then **exactly one**
  village exists that is **owned by that player**, and it sits at a **unique coordinate within the
  configured world bounds**. No two villages ever share a coordinate.

- **AC4 — Starting village baseline.** The starting village is created with the defined initial state:
  the standard set of **18 resource-field slots** and the baseline center buildings (at minimum a Main
  Building and a Rally Point) at their starting levels. Exact starting levels/quantities come from
  `specs/balance/` data, not hardcoded in logic.

- **AC5 — Configurable speed (P7).** Given a world configured **by the Administrator** at speed `S`,
  then the domain can read `S`, and any time-dependent computation derives from base values × `S`.
  Test: with a representative base duration `D`, the effective duration equals `D / S` (or `D × S` for
  rates) — changing `S` changes the result proportionally; **no wall-clock duration is a hardcoded
  constant**. A Player cannot change `S`.

- **AC6 — Due-event scheduler (P1/P2).** Given a due-event scheduled for time `T`, then it is processed
  **at or after `T`** (never before), **exactly once**, and the schedule is **persisted** so a pending
  event still fires after a process restart. Test with one trivial event type.

- **AC7 — Server authority (P4).** Village creation and ownership assignment happen **server-side**. A
  crafted client request cannot create a village, assign one to a different owner, or place one at an
  out-of-bounds or already-occupied coordinate.

- **AC8 — State survives restart (P2/P5).** Given a registered player with their village, when the
  application process is restarted, then logging back in shows the same account and the same village
  (no correctness-critical state was held only in memory).

## Roles & permissions

Per [roles.md](../../roles.md). System-initiated outcomes are marked.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | View public pages; register (AC1); log in (AC2). | View any village or game state; create/own a village directly (AC7). |
| **Player** | Log out (AC2); view **their own** starting village (AC3, AC4); their account & village persist across restart (AC8). | View or act on **another** player's village; configure world speed/bounds (AC5, Administrator-only); access moderation or operator functions. |
| **Moderator** | N/A (considered) — no moderation features exist in this slice. | — |
| **Administrator** | Configure the world's **speed** and **bounds** via world configuration (AC5, AC3). | — (superset). |
| **System** | *(system-initiated)* Create the starting village server-side upon registration (AC3, AC7); process the due-event scheduler (AC6). | — |

## Out of scope

Explicitly **not** in this slice (each has its own later slice):

- **Resource accrual over time** → slice 002.
- **Construction / build queue** → slice 003.
- **Tribe selection and its effects on units** → slice 004 (the starting village is tribe-agnostic
  for now; see Decisions).
- **The generated world map** (tile field-distributions, oases, Natar tiles) → slice 006. This slice
  only assigns a unique coordinate within world bounds; it does not generate the rich map.
- Any military, movement, combat, alliances, ranking, or UI beyond the minimal screens needed to
  register, log in, and view the one village.

## Decisions

- **Email confirmation** is **configurable**: default **OFF in dev**, **ON in production** (AC1).
- **Tribe selection** is **deferred to slice 004**. In 001 the starting village is created
  tribe-agnostic (placeholder/default); 004 introduces the selection step. (Deliberate deviation from
  GDD §12.1's "choose at registration," sequenced for build order.)
- **Starting coordinate** is assigned by a **simple unique, in-bounds placeholder strategy** in 001,
  to be replaced by proper map-aware placement in slice 006.
