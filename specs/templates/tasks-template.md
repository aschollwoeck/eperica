# Feature NNN — <Title> — Tasks

**Plan:** ./plan.md

Ordered, checkable units. Each should be small enough to complete and verify in one sitting.

- [ ] T1 — …
- [ ] T2 — …
- [ ] T3 — …
- [ ] T… — **Technical docs**: rustdoc on public items; update `CLAUDE.md` commands; architecture note if warranted.
- [ ] T… — **End-user docs**: update `docs/manual/` for player-visible behavior (or record "internal slice").
- [ ] T… — **Review & accept**: run `eperica-reviewer` on the slice diff; fix until verdict = APPROVE.

## Done when

The [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice)
is satisfied: all acceptance criteria in `spec.md` pass with tests, all gates green, both docs written,
reviewer APPROVED, PR merged, statuses set to Verified.
