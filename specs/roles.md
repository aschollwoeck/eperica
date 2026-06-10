# Eperica — User Roles & Permissions

**Status:** Standing document v1
**Governed by:** [constitution.md](./constitution.md) — esp. **P4** (server-authoritative; authorization
is enforced server-side, never trusted from the client).

This document defines the **actor roles** in Eperica. It is a standing reference: **every feature
spec must consider each applicable role in its acceptance criteria**, including the *negative* cases
(who is denied). See the rule in §4 below.

---

## 1. Two dimensions of "role"

- **A. Account / access roles** — attached to an account; govern which areas and actions are reachable
  at all.
- **B. Ownership & in-game roles** — derived from game *state* (you own this village; you hold this
  alliance rank), layered on top of an account role.

A given request is authorized by **both**: the account role must permit the kind of action, and the
ownership/in-game role must permit it on the *specific* target.

---

## 2. Account / access roles

| Role | Authenticated | Description | Can | Cannot |
|------|---------------|-------------|-----|--------|
| **Visitor** (Anonymous) | No | An unauthenticated user. | View public pages; register; log in. | Anything requiring an account (own no game state). |
| **Player** | Yes | The core actor — a registered participant in a world. | Act on **their own** game assets (villages, troops, queues, trades) and use social features per their in-game roles. | Act on others' assets except through game mechanics (attack/trade/scout); access moderation or operator functions. |
| **Moderator** | Yes (elevated) | Staff enforcing fair play (GDD §12.5). | Review reports, inspect flagged accounts, apply sanctions via moderation tools. | Configure or operate worlds; change game balance. |
| **Administrator** (Operator) | Yes (elevated) | Runs the deployment and the worlds. | Create / configure / start / archive worlds (speed, map size, schedule); full operational control. | — (superset; bounded only by audit/accountability). |

> **Elevated roles are additive to Player** where it makes sense (an Administrator can also play), but
> elevated *capabilities* are never available to a plain Player. **Default deny:** anything not
> explicitly granted to a role is denied, checked server-side (P4).

---

## 3. Ownership & in-game roles (layered on Player)

- **Owner.** A Player may act on an asset only if they **own** it. Affecting another player's assets
  happens exclusively through rule-governed game mechanics (combat, trade, scouting), never by direct
  authorization.
- **Alliance roles** — conferred within an alliance (GDD §10.1):
  - **Founder** — full alliance control.
  - **Leader** — holds granular **rights** (invite/expel, diplomacy, manage contributions, announce);
    a Leader has only the rights explicitly granted.
  - **Member** — belongs to the alliance; no management rights by default.
- **Sitter** *(deferred — GDD §12.4)* — a Player temporarily authorized to act on another account
  within limits. Not implemented yet; the account model must leave room for it.

---

## 4. The rule: roles in acceptance criteria

For every behavior a spec defines, its acceptance criteria **must address each applicable role**:

1. **Permitted (positive):** who may perform it, and on what targets.
2. **Denied (negative):** who is rejected — enforced **server-side** (P4), with a clear failure.
3. **N/A:** roles with no relevant interaction are listed once as "N/A (considered)" so it is explicit
   they were not forgotten.

The **System** actor (the scheduler/server executing due-events, weekly settlements, and NPC/Natar
behavior autonomously and authoritatively — P1/P4) is not a login role, but specs may assert
**System-performed** outcomes; mark these clearly as system-initiated, not user-initiated.

---

## Changelog

- **v1 (2026-06-10)** — Initial roles & permissions definition.
