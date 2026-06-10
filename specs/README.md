# Eperica — Spec-Driven Development (How it works)

This file is the **process reference**. It defines exactly which documents exist, where they live, what
each contains, and the step-by-step lifecycle by which a feature goes from idea to verified code.

**The one rule everything else serves:** the spec is the source of truth; **code conforms to the
spec, never the reverse.** If behavior must change, the spec changes *first*.

---

## 1. Two kinds of documents

| Kind | Lives in | Lifespan | Examples |
|------|----------|----------|----------|
| **Standing documents** | `specs/` (top level) | Long-lived, change rarely | constitution, game design, roadmap |
| **Per-feature artifacts** | `specs/features/NNN-slug/` | One set per build slice | spec, plan, tasks |

Standing docs answer *"what is this project and how must it behave overall."* Per-feature artifacts
answer *"what exactly are we building right now, how, and in what steps."*

---

## 2. Directory layout (the whole picture)

```
eperica/
├── specs/                              ← all design & process docs
│   ├── README.md                       ← THIS FILE — the process
│   ├── constitution.md                 ← non-negotiable principles (P1…P10). Read first.
│   ├── game-design.md                  ← Game Design Document: simulation mechanics ("what")
│   ├── social-and-meta-features.md     ← app-layer features (chat, profiles, UX) — not sim rules
│   ├── roles.md                        ← user roles & permissions (referenced by every spec)
│   ├── roadmap.md                      ← dependency-ordered build order (slices 001 → end-game)
│   ├── implementation-workflow.md      ← the build loop followed after spec/plan/tasks are approved
│   ├── balance/                        ← numeric balance data (created when first needed)
│   ├── templates/                      ← starting points — COPY these, don't edit in place
│   │   ├── spec-template.md
│   │   ├── plan-template.md
│   │   └── tasks-template.md
│   └── features/                       ← one folder per slice
│       └── NNN-slug/
│           ├── spec.md                 ← WHAT & WHY  (written 1st)
│           ├── plan.md                 ← HOW         (written 2nd)
│           └── tasks.md                ← STEPS       (written 3rd)
│
├── Cargo.toml                          ← Rust workspace (stack chosen in 001's plan)
├── crates/                             ← domain / application / infrastructure / web (P3 layering)
├── migrations/                         ← SQLx migrations
├── templates/                          ← Askama HTML templates
└── CLAUDE.md, README.md, LICENSE, ...
```

**Specs and code are separate trees.** Design lives in `specs/`; implementation lives in the Rust
workspace (`crates/`, with tests via `cargo test`). They are linked by the acceptance criteria (a
spec's criteria become the tests).

---

## 3. The standing documents — what each is for

| File | Contains | Changes when |
|------|----------|--------------|
| `constitution.md` | The fixed vision + 10 non-negotiable principles every other doc and all code must obey. | Rarely — only by a deliberate amendment recorded in its changelog. |
| `game-design.md` | The **simulation mechanics**: resources, buildings, units, map, movement, combat, ranking, end-game. Models & formulas, **not** raw number tables. | When a *game rule* changes. |
| `roles.md` | The **user roles & permissions** (Visitor, Player, Moderator, Administrator + ownership/in-game roles). Every spec's acceptance criteria must account for each applicable role. | When a role or permission is added/changed. |
| `social-and-meta-features.md` | **App-layer** features that surround the game but aren't simulation rules (messaging, forum, profiles, notifications, admin). | When a meta/UX feature is added or reshaped. |
| `roadmap.md` | The dependency-ordered list of slices (001 → end-game), grouped into milestones. The master build order. | When slices are added, split, merged, or reordered. |
| `implementation-workflow.md` | The repeatable build loop (per-task + per-slice), the Definition of Done, and doc locations — followed once spec/plan/tasks are approved. | When the build/review/doc process changes. |
| `balance/` | The actual **numbers** (production per level, costs, build times, unit stats) referenced by specs. | Whenever balance is tuned — without touching prose docs. |

---

## 4. The per-feature artifacts — what each contains

Every slice gets its own folder `specs/features/NNN-slug/` with **three files, written in order**.
Start each by copying the matching file from `templates/`.

### 4.1 `spec.md` — the WHAT & WHY *(tech-agnostic)*
- **Goal** — the player-facing capability this slice delivers, and why.
- **User stories** — `As a <role>, I want <capability>, so that <benefit>.`
- **Acceptance criteria** — the heart of the spec. Written so they become tests: exact numbers,
  Given/When/Then. *e.g. "Given a village producing 30 wood/h, when 2 h pass, then it holds 60 more
  wood, capped at warehouse capacity."*
- **Roles & permissions** — for each applicable role from [roles.md](./roles.md), the **permitted**
  (positive) and **denied** (negative, server-enforced) criteria; roles with no interaction listed as
  "N/A (considered)". This is mandatory, not optional.
- **Out of scope** — what this slice deliberately does *not* cover (keeps it small).
- **Open questions** — anything unresolved, with a proposed answer.

> `spec.md` never mentions frameworks, classes, or tables. It describes behavior only.

### 4.2 `plan.md` — the HOW *(technical)*
- **Constitution check** — how the plan satisfies the relevant principles (esp. P1 lazy time, P3 pure
  domain, P4 server-authoritative, P7 configurable speed); flag any tension.
- **Domain model** — entities, value objects, and the pure rules involved.
- **Persistence** — tables/EF entities; what is stored vs. computed-on-read (P1/P2).
- **Services / application layer** — commands and how due-events are produced and processed.
- **Interface (UI/API)** — what the client sees and posts; server stays authoritative.
- **Test strategy** — how each acceptance criterion maps to tests.

### 4.3 `tasks.md` — the STEPS
- An **ordered, checkable** list (`- [ ] T1 …`), each small enough to finish and verify in one sitting.
- A **"Done when"** line: all acceptance criteria pass and all tasks are checked.

---

## 5. Naming & numbering

- Folder name = `NNN-slug` where **NNN** is the slice's number from `roadmap.md` (e.g. `001`) and
  **slug** is a short kebab-case name (e.g. `foundation`, `resource-production`).
- Numbers are **stable handles** — once assigned, don't renumber. A new slice inserted between two
  existing ones gets a fresh number (or a `NNNa` suffix); we don't shuffle the others.

---

## 6. The lifecycle of one slice (step by step)

Each step names the file touched and the **gate** that must pass before the next step.

| Step | Action | File(s) | Gate before proceeding |
|------|--------|---------|------------------------|
| 0 | Pick the next slice whose dependencies are all **Verified**. | `roadmap.md` | Prereqs done. |
| 1 | Create `features/NNN-slug/`, copy `spec-template.md` → `spec.md`, fill it. | `spec.md` | **Spec reviewed & approved.** |
| 2 | Copy `plan-template.md` → `plan.md`, design against the approved spec. | `plan.md` | **Plan reviewed; constitution check passes.** (Stack decided here for 001.) |
| 3 | Copy `tasks-template.md` → `tasks.md`, break the plan into ordered tasks. | `tasks.md` | Tasks agreed. |
| 4 | Implement, writing tests that assert the spec's acceptance criteria. | `src/`, `tests/` | All acceptance criteria pass; tasks checked. |
| 5 | Verify behavior against the spec; update statuses. | all | Slice marked **Verified**. |

**Behavior-change rule:** if implementation reveals the behavior must differ from the spec, **stop and
update `spec.md` first** (and `plan.md` if needed), then continue. The spec never silently diverges.

---

## 7. Status tracking

Each per-feature file carries a `**Status:**` line at the top that advances through:

`Draft` → `Reviewed` → `Built` → `Verified`

The roadmap's milestone table is the high-level view; the per-file status is the detail.

---

## 8. How code relates to specs

- A slice's **acceptance criteria** (spec.md) are realized as **automated tests** (`cargo test` —
  domain unit tests + integration tests). A slice is not done until those tests pass.
- The **pure domain layer** (P3) is where game rules live and where most tests point — it has no I/O,
  so acceptance criteria about production/combat/etc. can be asserted with exact numbers.
- **Balance numbers** are *not* hardcoded in prose specs; they live in `specs/balance/` (introduced by
  the first slice that needs them) and are loaded by the code, so tuning never edits the game-design
  doc.

---

## 9. Discipline (the short list)

- **Spec just-in-time.** Standing docs are written once and kept stable; feature specs are written one
  slice at a time, not all upfront (that's waterfall and it rots).
- **One slice at a time**, strictly in dependency order from the roadmap.
- **The spec is the source of truth.** Update it before changing behavior.
- **Every spec accounts for every applicable role** ([roles.md](./roles.md)) — permitted *and* denied,
  enforced server-side (P4). Don't ship a behavior without saying who *can't* do it.
- **Everything is in git.** Commit each artifact (spec, plan, tasks) as it's approved, and the code
  against its tasks — so history shows design preceding implementation.

---

## 10. Worked example — slice 001

```
specs/features/001-foundation/
├── spec.md     # Goal: a visitor can register, log in, and see one starting village in a
│               #       speed-configured world.
│               # AC1: Given valid registration details, when I submit, then an account is created
│               #      and I can log in.
│               # AC2: Given a logged-in new player, then exactly one starting village exists,
│               #      owned by me, at a valid map coordinate.
│               # AC3: Given the world is configured at speed S, then S is readable by the domain
│               #      and no duration in code is a hardcoded wall-clock value.
│               # Out of scope: resource accrual (002), construction (003), map UI.
│
├── plan.md     # Constitution check: P3 layering, P5 stateless web + DB truth, P7 speed config,
│               #   P1 due-event scheduler skeleton.
│               # >>> The application stack (web framework, DB engine, project layout) is CHOSEN
│               #     HERE — this is the first slice that writes code. <<<
│
└── tasks.md    # - [ ] T1 Create solution + Domain/Application/Infrastructure/Web projects
                # - [ ] T2 Configure DB + migrations + auth (register/login)
                # - [ ] T3 World + speed config readable by the domain (P7)
                # - [ ] T4 Due-event scheduler skeleton (P1) with one trivial event + test
                # - [ ] T5 Create a starting village on registration; show it
                # Done when: AC1–AC3 pass and T1–T5 checked.
```

When we spec 001 for real, this folder is created from the templates and filled in detail — but the
shape above is exactly what every slice looks like.

---

## 11. Execution & review workflow (how slices get built)

> **Full procedure:** [implementation-workflow.md](./implementation-workflow.md) — the per-task and
> per-slice loop, the Definition of Done, and documentation locations. The summary below is the gist.

How the *code* (step 4 of §6) is produced, checked, and accepted:

- **Execution:** implemented **serially in the main thread**, task-by-task from `tasks.md`, test-first
  for the domain. Parallel subagents/workflows are reserved for later high-volume independent work and
  proposed explicitly — never spun up by default.
- **Automated gates (every task):** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
  green, plus the latency budget on hot paths (P11). Work that fails a gate is not advanced.
- **Acceptance gate = the review agent, not the human.** The dedicated **`eperica-reviewer`** agent
  (`.claude/agents/eperica-reviewer.md`) reviews the change against the constitution, the slice's
  acceptance criteria + roles, and Rust quality/security. Findings are fixed and re-reviewed until the
  verdict is **APPROVE** with no MUST-FIX remaining. The human does **not** manually review code.
- **Status flips** to `Verified` only after the reviewer approves and all ACs pass.
- **Version control:** the spec foundation is committed as a baseline; then **one branch per slice**
  (`feature/NNN-slug`), **a commit per task**, and a **GitHub PR per slice**. The PR is the record and
  the home for the review; it is merged to `main` once the slice is Verified (reviewer-approved + green).

This workflow is operational guidance; the *artifact* rules (§1–§10) still govern.

