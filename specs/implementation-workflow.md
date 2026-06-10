# Eperica — Implementation Workflow

**Status:** Standing document v1
**Governed by:** [constitution.md](./constitution.md) · **Complements:** [README.md](./README.md)

`README.md` defines the *artifact* flow (spec → plan → tasks). **This document defines what happens
once a slice's `spec.md` (Reviewed), `plan.md` (Reviewed), and `tasks.md` exist** — the repeatable
build loop that is **always** followed before a slice can be called done. It is the expanded form of
README §6 steps 4–5.

The mandatory activities, every slice: **implement · write tests · run & verify all tests · review &
accept · write technical documentation · write end-user documentation · ship (PR + merge).**

---

## Phase 0 — Slice setup (once per slice)

1. Ensure `main` is current; create branch **`feature/NNN-slug`**.
2. Mirror `tasks.md` into the live task tracker; mark the slice in progress.

---

## Phase 1 — Per-task loop (repeat for each task, in order)

For each task `Tn`:

1. **Mark `Tn` in progress.**
2. **Write tests first.** Derive them from the acceptance criteria — exact values where possible. For
   the pure domain (P3) this is strict TDD; for infra/web, write tests alongside the code.
3. **Implement** the minimal code to satisfy `Tn` and its tests; match surrounding style.
4. **Technical docs (inline).** rustdoc on new public items. If the design must deviate from
   `plan.md`, update `plan.md`. **If behavior must differ from `spec.md`, STOP and update `spec.md`
   first (P8)** — the spec never silently diverges.
5. **Run & verify ALL gates** (not just the new tests — no regressions):
   - `cargo fmt --check`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test` (whole workspace)
   - hot-path latency check if the task touches one (P11)
6. **Commit** the task (one commit per task): `feat|fix|test|docs|refactor|chore(NNN): summary`,
   imperative mood, ending with the Co-Authored-By trailer.
7. **Mark `Tn` done.**

> A task is not done until its tests pass and **every** gate is green. Never advance past a red gate.

---

## Phase 2 — Slice completion (once all tasks are done)

1. **Run & verify the full suite** + the **P11 latency budget**. Confirm **every** acceptance
   criterion in `spec.md` is implemented *and* covered by a passing test; record the AC → test mapping.
2. **Technical documentation (consolidation):**
   - rustdoc complete on public `domain`/`application` APIs.
   - Update **`CLAUDE.md`** commands (build/test/run/migrate) now that they exist and work.
   - If the slice introduced architecture worth narrating (e.g. the scheduler, the workspace, auth),
     add a short note under **`docs/architecture/`**.
   - Confirm `spec.md`/`plan.md` reflect the final behavior (P8).
3. **End-user documentation:**
   - For any **player-visible** behavior, write/update the player manual under **`docs/manual/`**
     (e.g. `docs/manual/getting-started.md` for register/login). Plain, task-oriented ("How to …").
   - A slice with no player-visible behavior records "no end-user docs (internal slice)" explicitly.
4. **Review & accept (the gate):** run the **`eperica-reviewer`** agent on the full slice diff
   (`git diff main...HEAD`). Address every **MUST-FIX**, re-run gates, re-review. **Loop until the
   verdict is `APPROVE` with no MUST-FIX outstanding.** The human does not review (per operating model).
5. **Pull request:** push the branch; open a GitHub PR (`gh pr create`) summarizing the slice, linking
   `spec.md`/`plan.md`, and pasting the reviewer's final verdict. The PR is the record.
6. **Merge & finalize:** merge to `main` once reviewer-APPROVED and green. Then:
   - Set `spec.md` and `plan.md` **Status → Verified**.
   - Mark the slice **done** in `roadmap.md`.
   - Delete the feature branch.

---

## Definition of Done (checklist — applies to every slice)

- [ ] All tasks in `tasks.md` checked.
- [ ] Every acceptance criterion implemented **and** covered by a passing test.
- [ ] Full `cargo test` green; `cargo fmt` clean; `clippy -D warnings` clean.
- [ ] P11 latency budget verified on hot paths.
- [ ] **Technical docs**: rustdoc on public items; `CLAUDE.md` commands current; architecture note if
      warranted; `spec.md`/`plan.md` in sync.
- [ ] **End-user docs**: `docs/manual/` updated for any player-visible behavior (or "internal slice").
- [ ] `eperica-reviewer` verdict = **APPROVE**, no MUST-FIX outstanding.
- [ ] PR opened, green, merged to `main`.
- [ ] `spec.md`/`plan.md` = **Verified**; `roadmap.md` updated; feature branch deleted.

---

## Documentation locations (convention)

| Kind | Lives in | Form |
|------|----------|------|
| **Technical (API)** | in code | rustdoc on public items |
| **Technical (cross-cutting)** | `docs/architecture/` | short markdown narratives, only when a slice adds architecture worth explaining |
| **Behavior (authoritative)** | `specs/` | the spec/plan/GDD — the source of truth, not duplicated into prose docs |
| **End-user** | `docs/manual/` | player-facing, task-oriented guides, one area per file |

The existing `docs/eperica_concept.docx` is the original concept and is superseded by `specs/` for
design; it is kept for history.

---

## Conventions

- **Commits:** conventional-commits style — `feat|fix|test|docs|refactor|chore(NNN): summary`,
  imperative, with the Co-Authored-By trailer. One commit per task.
- **Test-first** is strictest in the pure domain (P3), where exact-number ACs make it natural.
- **Never** advance past a red gate; **never** silently diverge from a spec (P8).
