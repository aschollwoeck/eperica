# Eperica — Documentation

This folder holds the project's **produced documentation**. (Design/behavior lives in `specs/` and is
the source of truth; this folder does not duplicate it.)

```
docs/
├── README.md            ← this file: documentation conventions + templates
├── architecture/        ← technical notes (created when first needed)
├── manual/              ← end-user (player) manual (created when first needed)
└── eperica_concept.docx ← original concept (historical; superseded by specs/)
```

Documentation is part of "done" for every slice — see
[implementation-workflow.md](../specs/implementation-workflow.md).

---

## 1. Technical documentation (intentionally light)

The primary technical documentation is **rustdoc in the code** — doc comments on public items of the
`domain` and `application` crates especially. We do **not** maintain a separate styled tech-docs site.

`docs/architecture/` holds **short narrative notes** only when a slice introduces cross-cutting
architecture worth explaining (e.g. the due-event scheduler, the workspace/layering, auth/sessions).
Keep them brief; they explain *why*, not *what* (the code and specs cover *what*).

**Architecture note template** (`docs/architecture/NNN-topic.md`):

```markdown
# <Topic>

**Status:** Current | Superseded by <link>
**Date:** YYYY-MM-DD · **Slice:** NNN

## Context
What problem/force made this necessary (link the relevant constitution principle).

## Design
The approach taken, and the key alternatives rejected (one line each).

## Consequences
What this makes easy, what it makes harder, and any follow-ups.

## Links
spec/plan, code modules, related notes.
```

---

## 2. End-user (player) documentation

Lives in `docs/manual/`. It is the **player-facing manual** — how to *play*, not how the code works.

### 2.1 Structure
- `docs/manual/README.md` — the manual **index** (table of contents, links to each area).
- One file per area/feature, named by topic: `getting-started.md`, `resources.md`, `buildings.md`,
  `combat.md`, … Order them in the index along the player's learning path.

### 2.2 Style guide
- **Audience:** a new player who has never played Travian. Assume no jargon until defined.
- **Voice:** second person, present tense, friendly and direct. "You build a warehouse to store more
  resources." Not "The player may construct…".
- **Task-oriented:** lead with what the player wants to do. Use **numbered steps** for procedures and
  short sections with clear `##` headings.
- **Terminology:** use the exact terms from the GDD (village, resource fields, warehouse, rally point,
  …) consistently — never synonyms. **Bold** a term on first use.
- **Numbers:** describe mechanics qualitatively; avoid hardcoding balance values that may change (link
  to in-game figures instead of pasting tables).
- **Brevity:** short paragraphs, scannable. Prefer a list or table over a wall of text.
- **Media:** screenshots/diagrams are welcome once the UI exists; store under `docs/manual/img/`. Every
  image needs descriptive alt text. Don't block a doc on missing screenshots.
- **Cross-links:** link related pages ("see [Resources](resources.md)") rather than repeating content.

### 2.3 Page template (`docs/manual/<topic>.md`)

```markdown
# <Player-facing title>

> One-sentence summary of what this page helps you do.

## What is <topic>?
A short, plain-language explanation, defining any **new term** on first use.

## How to <do the main task>
1. Step one.
2. Step two.

## Tips
- Practical advice / common pitfalls.

## See also
- [Related page](other.md)
```

---

## 3. What goes where (quick reference)

| You're documenting… | Put it in |
|---------------------|-----------|
| How a function/type/module works | rustdoc (in the code) |
| Why a cross-cutting design choice was made | `docs/architecture/` |
| What the game's rules/behavior are | `specs/` (source of truth — don't copy here) |
| How a player uses a feature | `docs/manual/` |
| How the UI should look/behave | `specs/ui-style-guide.md` |
