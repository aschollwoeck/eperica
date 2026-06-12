# Attacking & defending

When troops arrive at an enemy village, a **battle** is fought instantly. You launch attacks from the
**Rally Point**; the server resolves them and writes a **battle report** to both sides.

## Attack vs. raid

Pick an **Order** on the Rally Point:

- **Attack** — fight to destroy. The **loser loses everything** that took part; the winner loses a
  share that shrinks the bigger their edge.
- **Raid** — fight to plunder. **Both sides** take losses (the stronger side loses less) and survivors
  remain. Raiding is the low-commitment way to grind an enemy down.

(Carrying off loot arrives in a later update — for now a raid is the fight and the casualties only.)

To launch: choose the order, enter the target tile (`x`/`y`), set how many of each unit to send, and
**Send**. The troops leave your garrison and travel; the battle happens the moment they arrive. Once
sent, an attack **can't be recalled**.

## How a battle is decided

Your army's **attack** is weighed against the defender's **defence**. A few things matter:

- **Unit types.** Each unit has separate **defence vs. infantry** and **vs. cavalry**; the defender's
  troops defend best against the unit class you bring most of. Mixing forces is a real choice.
- **The Wall.** A defender's **Wall** multiplies their whole defence. **Rams** you bring smash the
  Wall down — enough ram power levels it completely before the defence is even counted.
- **Morale.** If you're a much bigger player attacking a much smaller one, **morale** weakens your
  attack — newer players get some protection.
- **Luck.** Every battle rolls a bounded **luck** factor (±25%). It's decided by the world's seed and
  your army, so the same battle always plays out the same way — no take-backs, no online advantage.

Whoever has more effective power wins; casualties follow a **power-law**, so a modest power edge yields
a lopsided result.

## Defending

You don't have to do anything to defend: your **garrison** and any **reinforcements** other players
have stationed with you (from the Rally Point) all fight automatically. Build a **Wall** to multiply
your defence, and keep troops — or ask allies to reinforce — if you expect to be hit.

## Battle reports

After every battle, both sides get a **report** (see **Reports** from your village). It lays out each
side's forces and losses, how far the Wall was knocked down, and the **luck and morale** that applied —
so the outcome is always explainable. Only the two parties to a battle can read its report.

> See also **[Scouting](scouting.md)** — reveal an enemy's defenses or resources before you commit.
> Coming next: siege & loot (catapults wreck buildings; raids carry resources home, minus what a
> Cranny hides).
