---
name: author-workflow
description: Use when you and a user need to AUTHOR a new fsmp definition (a state-machine workflow) — not drive an existing one. You interview the user about the workflow's steps and the omission failure-modes worth guarding, design a state graph together, then drive the un-skippable draft → lint → dry-run → sign-off pipeline enforced by the `fsmp` machine shipped beside this skill.
argument-hint: "[what the workflow should do]"
---

# Author Workflow

Author a new **fsmp definition** — a human-authored state machine that later
steers an agent through some workflow — *with* a user. This is the design-time
counterpart to dev-cycle: dev-cycle *drives* a machine; this skill helps you
*build* one.

Two halves, and keeping them separate is the point:

- **Elicitation and graph design are conversational and happen BEFORE the machine
  starts.** You interview the user, propose a state graph, and get sign-off on its
  shape. No tool enforces this part — it's judgment, and this skill carries that
  judgment.
- **The un-skippable tail is enforced by `fsmp`.** Once the graph is signed off,
  you drive `authoring-machine.yaml` (beside this skill): draft → lint → dry-run →
  user sign-off → done, with every quality gate able to loop you back to drafting
  but never forward past a failure. You don't sequence that tail from memory; the
  machine does.

**This skill covers the JUDGMENT** (which omissions deserve a guard, when a
counter beats a plain sequence, how to phrase guidance so it steers). **The
machine covers WHICH step and WHEN.** For the definition grammar and the
pattern/anti-pattern catalog, this skill points at `fsmp guide definition` rather
than restating it — that doc is the single source of truth.

## Prerequisites

`fsmp` on your PATH (`fsmp --version` should work). See the fsmp project's
`make install`, which installs to `~/.fsmp/bin`. Read `fsmp guide definition`
before you draft, and `fsmp guide driving` if you're new to how a machine is
driven.

## Phase A — elicit and design the graph (conversational, before the machine)

Do this *with* the user, in prose. Your goal is a state graph you can both sign
off on.

**1. Interview for the workflow AND its failure-modes.** Walk the user through
the workflow step by step, but keep asking the question this whole tool exists
for: **which step, if silently skipped, would go unnoticed until it hurt?**
fsmp enforces *sequence, not content* — it defends against **omission**. A step
that's obvious and self-correcting barely needs a machine; a step that an agent
under pressure would quietly drop (responding to every review note, re-running
the same reviewer, not presenting before a bar is met) is exactly what to model.
Draw those omissions out explicitly — they become your guards and gates.

**2. Turn the workflow into states and transitions.** A **state** is a place the
machine waits with one instruction ("the reviewer is reviewing; when it returns,
classify the verdict"). A **transition** is an event that moves it on. Sketch the
nodes, the edges, and the terminal states (a success end, and usually an
`escalated` give-up hatch off every waiting state).

**3. Decide where judgment calls go — before writing any YAML:**

- **A plain sequence, or a counter?** If "done" means one thing happened, a chain
  of states suffices. If it means *N independent things* happened ("two clean
  reviewers"), you need a **counter gate**: a context counter, an effect that
  increments it only on the qualifying event, and a guard that won't let the exit
  fire until `count >= bar`. Don't fake N-of-something with N hand-copied states.
- **A required step an agent skips → make it the only legal move.** If a step is
  mandatory, the state before it should offer *no other forward transition*, so
  skipping is impossible rather than discouraged.
- **Feedback that must be acted on and re-checked → a response-then-reassess
  loop.** Route it through a "respond" state and a "same actor re-checks" state
  with an edge looping back, so the re-check can't be skipped.
- **A knob set once vs. state that changes.** A value fixed at start (a bar, a
  ceiling, a mode) is a **param** (read-only). Anything that moves during the run
  (counters, latches, a captured url) is **context**. See the params/context
  anti-pattern in `fsmp guide definition`.

**4. Phrase guidance as the next action.** Every state's guidance is the prompt
the driving agent acts on. Write it as an imperative instruction for what to do
*now* ("Send the same reviewer back to re-assess its notes"), not a description of
where the machine is ("You are in the review phase"). Interpolate `{vars}` so the
numbers and urls are always live.

**5. Show the user the graph and get explicit sign-off on its SHAPE** — states,
transitions, and for each guard/counter the omission it defends. Changing the
shape after the prose is written is expensive; that's why the machine's first gate
is this sign-off.

## Phase B — drive the authoring machine

Once the user has signed off on the graph, create the machine and let it sequence
the rest:

```
fsmp new --def .claude/skills/author-workflow/authoring-machine.yaml \
  --id <project>-<slug>
```

Give `--id` a **descriptive** value — mirror dev-cycle's `<project>-<slug>`
convention (e.g. `fsmp-release-flow`), never a bare counter. It ties the run to
what you're authoring and keeps `fsmp show`/`log` legible. Keep the id; you pass
it to every later call.

Then, **every turn, obey the current guidance and record the outcome** with
`fsmp do`. The machine prints your state, what to do now, the valid moves, and any
blocked ones with the reason:

```
fsmp do <transition> --id <id> [--data key=value]
fsmp show --id <id>          # re-print the current step any time
```

The flow the machine enforces:

| State | What you do | Then fire |
|---|---|---|
| `graph_review` | (already done in Phase A) confirm the signed-off graph; note the file path | `graph_approved --data def_path=<path>` |
| `drafting` | Write the YAML at `def_path` per `fsmp guide definition` | `draft_written` |
| `linting` | Run `fsmp lint --def <path>`; fix any findings | `lint_clean` (or `lint_failed` → back to drafting) |
| `dry_run` | Walk the machine in a throwaway `FSMP_HOME`: happy path AND every guard-blocked move | `dryrun_passed` (or `dryrun_failed` → back to drafting) |
| `user_signoff` | Show the user the definition + dry-run trace | `accepted` (or `changes_requested` → back to drafting) |
| `done` | Scaffold a SKILL.md if a skill will drive it; commit | (terminal) |
| any waiting state | blocked on a decision only the user can make | `escalate` → `escalated` (terminal) |

**Do not sequence from memory, and do not skip a gate.** If a move you want isn't
offered, the machine tells you why — that block is the point (you can't reach the
dry-run before lint is clean; you can't reach sign-off before a dry-run). The
dry-run gate is where you catch what the linter can't: a counter that never
converges, a guidance string that narrates instead of directs, an edge you forgot.

## What the machine enforces vs. what's still on you

**Enforced by fsmp — you cannot skip these:**
- No YAML is written before the user signs off on the graph shape.
- Lint must be clean before the dry-run; the dry-run must pass before user
  sign-off; `done` is reachable only through explicit `accepted`.
- A failed gate loops back to `drafting` — you never advance past a failure.

**Still your judgment — the machine can't catch these:**
- **Which omissions deserve a guard at all** — that's the whole design, and it's
  conversational (Phase A). A machine that guards nothing worth guarding is
  ceremony.
- **Counter vs. plain sequence, param vs. context, imperative guidance** — the
  design calls in Phase A step 3–4. `fsmp guide definition` catalogs the patterns;
  applying them is on you.
- **A genuinely thorough dry-run** — actually drive the happy path *and* attempt
  each blocked move; a green lint is blind to a non-converging counter or a dead
  prompt.
- **The authoring machine is itself a worked example** of the
  pipeline-with-retry-gates shape (cited in `fsmp guide definition`). dev-cycle is
  the model for the counter-gate and response-then-reassess shapes.
