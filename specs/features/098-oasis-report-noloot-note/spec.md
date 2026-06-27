# Feature 098 — oasis raid report: explain the empty haul

## Why

A raid on an oasis yields no loot by design (012: "the reward is the bonus, not loot; survivors return empty
from an oasis"). But the battle report just showed no loot line, which reads as a bug ("how much did I
raid?"). Add a clear note so the zero loot is understood, not mistaken for missing data.

## Acceptance criteria

- **AC1 — Note on won oasis raids.** When the viewer is the attacker of a won oasis attack/raid
  (`kind == OasisAttack`, `attacker_won`), the report shows: "Oases hold no resources to plunder — your
  troops returned empty-handed." Village raids are unaffected (their real loot still shows).

## Out of scope
- Any change to oasis combat/loot rules (oases remain loot-free, 012).
