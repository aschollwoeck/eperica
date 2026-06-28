# Feature 101 — settlers train without Academy research (faithful)

## Why

013 says settlers "train in the Residence/Palace"; faithful Travian trains them there directly, with **no
Academy research**. But the balance data gave the settler a `research` block (gated on Main Building 10), so a
player had to research it at the Academy first — and the domain's "exactly one research-free unit per tribe"
rule forced that. A player with a Residence/Palace couldn't build settlers until a multi-step research gate
was met. Make settlers trainable directly, as 013/Travian intend.

## Acceptance criteria

- **AC1 — No research gate.** The settler is research-free: it appears on the Residence/Palace training page
  and is trainable as soon as a Residence or Palace is built — no Academy research. (The Residence/Palace +
  culture/expansion-slot gates from 013 still apply to *settling*.)
- **AC2 — Domain allows research-free Expansion units.** `UnitRules::new` requires exactly one research-free
  *combat* unit (the tier-1 starter); **Expansion** units (settlers/administrators) may also be research-free.
- **AC3 — Administrators unchanged.** The conquest administrator (senator/chief/chieftain) keeps its research
  requirement (out of scope; its gate stays as-is).

## Out of scope
- Administrators' gating; a Residence/Palace *level* gate for settler count (013 uses CP + expansion slots).
