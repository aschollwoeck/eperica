# Feature 047 — Per-world end-game schedule (operator-set)

**Status:** Draft
**Depends on:** 041 (world lifecycle admin: create-world form + `AdminRepository::create_world`).
**Roadmap:** First slice of the **per-world configuration** program — see
[ADR 0035](../../../docs/architecture/0035-per-world-configuration.md).
**Program note:** Today an admin sets only a new world's **speed** and **radius**; the artifact/Wonder
release schedule is fixed (admin-created worlds use hardcoded `90d`/`120d` constants, ignoring the env
offsets the boot world uses). This slice makes the end-game schedule operator-set on the form and unifies
the two sources of truth onto one code path. Standalone; no rule-preset work (048+).

## Problem

- The release dates are stored per-world (`worlds.artifact_release_at`/`wonder_release_at`) and threaded
  through `AdminRepository::create_world(speed, radius, artifact_offset, wonder_offset)`, but the
  create-world form exposes neither, so every admin world gets the same fixed schedule.
- Two sources of truth: the **boot** world uses the env offsets (`ARTIFACT/WONDER_RELEASE_DELAY_SECS`); the
  **admin** path uses hardcoded constants in `admin.rs` (`ARTIFACT/WONDER_RELEASE_DEFAULT_SECS`). They
  coincide today, so a changed env offset would silently not apply to admin worlds.

## Goal

- **AC1 — Schedule on the form.** The `/admin` create-world form gains **artifact-release** and
  **Wonder-release** offsets, entered in **days**, prefilled with the operator's configured defaults. The
  new world's `artifact_release_at`/`wonder_release_at` are set from them.
- **AC2 — Server-authoritative validation (P4).** The use case validates `0 < artifact_days` and
  `artifact_days < wonder_days` (the Wonder phase must follow the artifact phase, GDD §13.2/§11.3),
  alongside the existing speed/radius checks; an invalid schedule is rejected with a flash, no world created.
- **AC3 — One source of truth.** The env `ARTIFACT/WONDER_RELEASE_DELAY_SECS` become the **defaults**
  surfaced in the form and used when a field is omitted; the admin path no longer uses its own hardcoded
  constants. The boot world and admin-created worlds derive their schedule the same way.
- **AC4 — Behaviour preserved.** With the defaults unchanged (90d / 120d), an admin who doesn't touch the
  new fields gets exactly today's schedule; the existing suite passes.

## Design

- **`AppState`** gains `artifact_release_offset_secs` / `wonder_release_offset_secs` (from `AppConfig`, the
  env values) so the admin handler can prefill the form defaults (as **days** = secs / 86400). `AdminTemplate`
  gains the two default-days fields.
- **`CreateWorldForm`** gains `artifact_days: Option<i64>` + `wonder_days: Option<i64>` (optional — omitted ⇒
  the env default). The handler resolves each to seconds (`days * 86400`), passing them to the use case.
- **`create_world` use case** takes `artifact_offset_secs` + `wonder_offset_secs` as parameters (no more
  `ARTIFACT/WONDER_RELEASE_DEFAULT_SECS` constants); validates `0 < artifact < wonder` and returns
  `AdminError::InvalidWorld` otherwise. The two `*_DEFAULT_SECS` consts are removed.
- No schema/migration change (`worlds` already stores the dates; `create_world` already accepts offsets).

## Out of scope

- Per-world **rule presets** (the rest of ADR 0035: 048–053). Editing the schedule of an **existing**
  world (creation-time only; mid-round end-game changes would risk the 021 freeze invariants). Surfacing the
  release dates beyond the existing read-only admin status panel (036 AC4).
