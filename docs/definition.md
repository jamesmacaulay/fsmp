# Authoring fsmp definitions

A **definition** is a static, human-authored workflow, kept in version control
beside the code or skill it guards. It is a rigid guardrail: the agent *drives* a
machine instantiated from it, but never authors or mutates the definition at
runtime. This document is the reference for the definition format, the patterns
that make a definition steer well, and the anti-patterns that make it steer
badly.

When a definition backs a skill, the convention is to name the file
`fsmp-definition.yaml` and ship it in the skill's folder beside `SKILL.md`. Like
`SKILL.md` itself, the enclosing folder carries the specific name; the
well-known filename is what marks a skill as fsmp-backed (and what tooling can
glob for). The definition's own `name:` field carries its identity when the
file is read out of context.

For how an agent *drives* a machine once you've authored it, see
`fsmp guide driving`.

## The model

### Definition vs. instance

A **definition** is the file you author (YAML preferred for readable prose and
comments; JSON is also accepted — the extension picks the parser). An
**instance** is one live run of it: a snapshot of the definition taken at
`fsmp new`, plus the current state, the mutable context, and a transition log.

The snapshot matters. Editing the source file — or switching branches — after
`new` does **not** change a running instance. A run is stable against edits to
its own rails.

A definition has these top-level fields:

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | The workflow's name. |
| `description` | no | One-line summary. |
| `params` | no | Set-once, read-only knobs (see below). Defaults to empty. |
| `context` | no | Initial values for the mutable run state. Defaults to empty. |
| `initial` | yes | The name of the state a new instance starts in. |
| `states` | yes | Map of state name → state. |

### State

A **state** is a node: the just-in-time prompt re-injected every time the machine
is in it, plus the edges out.

```yaml
states:
  drafting:
    guidance: |
      Write the definition. When it's on disk, fire `draft_written`.
    transitions:
      draft_written: { to: linting }
```

| Field | Required | Meaning |
|---|---|---|
| `guidance` | no (defaults to empty) | The prompt shown on arrival. **This is the interface the agent acts on** — write it as an imperative next-action instruction. Interpolated (see `{var}` below). |
| `terminal` | no (default `false`) | A terminal state ends the run. `fsmp do` refuses every move from a terminal state. |
| `transitions` | no (defaults to empty) | Map of transition name → transition. A non-terminal state with none is a dead end (the linter flags it). |

### Transition

A **transition** is an edge out of a state. Its name is what you pass to
`fsmp do <name>`.

```yaml
transitions:
  pr_opened:
    to: awaiting_review
    when: implementer returned the PR url with the gate green
    requires: [pr_url]
    blocked_reason: open the PR first, then supply its url
    guards:
      - { var: draft_ready, op: eq, value: true }
    effects:
      - { set: reviewed, to: false }
```

| Field | Required | Meaning |
|---|---|---|
| `to` | yes | Target state name. `new`/`lint` reject a target that isn't a defined state. |
| `when` | no | One-line "take this when …", shown beside the transition in the valid list. Interpolated. |
| `blocked_reason` | no | Why this move is unavailable when its guards fail — shown in the "Blocked from here" list and on a rejected `do`. Interpolated. Falls back to a generic line if omitted (a wasted prompt — see anti-patterns). |
| `guards` | no | Conditions that must ALL hold for the move to be available (implicit AND). Empty ⇒ always available. |
| `requires` | no | `--data` keys the caller must supply when firing this transition. A missing key is rejected with a hint. |
| `effects` | no | Context mutations applied when the transition fires. |

**`requires` vs. `guards` — reach for which?** `requires` is *data the caller must
attach to this move* (a url, an id); `guards` are *predicates over context/params
that already exist*. They're also checked at different times and surface
differently on a rejected `do`: `requires` is validated **first**, and a missing
key is rejected by name ("requires data: pr_url … --data pr_url=<value>"); only
then are `guards` evaluated, and a failing guard shows the interpolated
`blocked_reason`. So use `requires` to force the caller to bring a value into the
run, and `guards` to gate on values the run already holds.

### Guard grammar

A guard is a single structured comparison — `<var> <op> <rhs>`. There is **no
expression language**: a guard is plain data, and a transition's guards are
combined with an implicit AND (every guard must pass). To express "OR", author
two transitions, not a boolean.

```yaml
guards:
  - { var: clean_initial_count, op: gte, param: bar }
```

- `var` — the name whose value is the left-hand side. Resolved from **context
  first, then params** (context shadows a param of the same name).
- `op` — one of `eq`, `ne`, `lt`, `lte`, `gt`, `gte` (lowercase).
- The right-hand side is exactly one of:
  - `value:` — a literal (`true`, `2`, `"draft"`).
  - `param:` — the name of a read-only param.
  - `ctx:` — the name of another context variable (resolved context-then-param).

Operator semantics, exactly as the engine evaluates them:

- `eq` / `ne` compare values **by exact type and content**. `Int(3)` does not
  equal `Str("3")`, and `Int(1)` does not equal `Bool(true)`. A literal you write
  as `value: true` is a boolean; `value: "true"` is a string.
- `lt` / `lte` / `gt` / `gte` coerce both sides to integers (ints, or numeric
  strings). An ordered comparison against a **non-numeric** operand is simply
  **false** — never an error. So `count >= bar` is false while `count` is unset or
  non-numeric, which is usually what you want for a not-yet-started counter.
- If `var` resolves to nothing (unset), `eq`/`ne` compare against "nothing"
  (an absent value equals neither a literal nor a set variable), and ordered
  comparisons are false.

### Effect grammar

An **effect** mutates context when a transition fires. Four shapes:

```yaml
effects:
  - { set: initial_was_clean, to: true }   # set a variable to a literal
  - { incr: clean_initial_count }          # +1 (an unset var counts as 0 → 1)
  - { decr: budget }                        # -1 (an unset var counts as 0 → -1)
  - if: { var: initial_was_clean, op: eq, value: true }   # conditional:
    then: { incr: clean_initial_count }                   # apply `then` only if the guard holds
```

- `set` / `to` — assign a literal value.
- `incr` / `decr` — add or subtract 1; an unset variable is treated as 0.
- `if` / `then` — a guard (same grammar as above) and a nested effect applied
  only when the guard holds. `then` can itself be any effect, including another
  conditional.

Effects run in the order listed, **after** any `--data` supplied on the `do` has
been merged into context (so an effect's guard can read data the caller just
provided).

### Params vs. context

Two kinds of variable, and confusing them is a classic bug (see anti-patterns):

- **params** — set once at `fsmp new` (definition defaults, overridable with
  `--set k=v`), then **read-only** for the life of the run. Use them for the
  knobs a run is configured with: a review bar, a ceiling, a mode flag. No effect
  can change a param.
- **context** — the **mutable** run state. Seeded from the definition's `context`
  block, then changed by effects and by `--data` on transitions. Counters,
  latches, and captured data (a PR url) live here.

Both are readable by guards and `{var}` interpolation; only context is writable.
When a context var and a param share a name, context wins on lookup.

**Values are scalars.** Everywhere a value appears — a `params`/`context` seed, a
guard `value:`, an effect `to:`, `--data` on a `do` — it is a single scalar:
boolean, integer, or string (an unquoted `true`/`false` is a boolean, a bare
integer is an int, everything else is a string). Lists and maps are **not**
supported; a `context:` or `params:` entry written as a YAML list or map is
rejected at parse time (at `new`/`lint`), not silently coerced. Model
"multiple things" with multiple named variables, not a collection in one.

### `{var}` interpolation

`guidance`, `when`, and `blocked_reason` are interpolated: each `{name}` token is
replaced with the resolved value of that variable (context-then-param). An
unknown name — or anything that isn't a bare identifier — is left **verbatim**,
braces and all, so a stray `{` never silently swallows text. Interpolate every
number and url you mention so the prompt states the *current* truth rather than a
frozen literal (see the "un-interpolated guidance that lies" anti-pattern).

### Terminal states

Mark an end state `terminal: true`. It should carry `guidance` (a final
"you're done, here's the propagate/cleanup step") but **no transitions** — `do`
refuses to move from a terminal state, so any edges out are dead code the linter
flags. Give a machine at least one success terminal and, usually, an `escalated`
terminal for the give-up hatch.

## Positive patterns

These are the shapes that make a machine actually hold an agent on the rails.

### Counter-gate convergence

When "done" means *N independent somethings*, not one, gate the exit on a counter
and increment it only on the qualifying event. dev-cycle's convergence is the
canonical case: `converge` is guarded on `clean_initial_count >= bar`, and the
count only rises when a reviewer's **initial** verdict was clean *and* it later
reached satisfied. One clean reviewer cannot satisfy a bar of two; the agent
can't present early by miscounting, because it isn't doing the counting.

```yaml
converge:
  to: verifying
  blocked_reason: >-
    needs {bar} clean-initial reviewers, each subsequently SATISFIED;
    currently {clean_initial_count}
  guards:
    - { var: clean_initial_count, op: gte, param: bar }
```

### Response-then-reassess loop

When feedback must be *acted on and then re-checked by the same actor*, don't let
one state both receive feedback and exit. Route it: `awaiting_impl_response`
(implementer addresses every note) → `awaiting_reassessment` (the **same**
reviewer re-checks its own notes) → only then `round_complete`. The loop back to
the implementer (`reviewer_changes`) is an edge, so "the reviewer still isn't
satisfied" can't be skipped. This defends the omission the whole tool exists for.

### An `escalate` hatch on every waiting state

Every state where the machine is waiting on an agent or human should offer an
`escalate` edge to a terminal `escalated` state. Without it, a stuck run has no
legal move and the agent either wedges or fabricates a transition. The hatch
makes "I'm blocked, hand it to the operator" a first-class, recorded outcome
rather than an off-ramp the agent has to invent.

### A Phase-0 param gating a shortcut edge

When a workflow has a step that is mandatory for most runs but genuinely
meaningless for some, don't leave the skip to mid-run judgment — make it a
set-once param and guard the shortcut on it. dev-cycle's verification capstone
is the case: `converge` always lands in `verifying`, and the
`verification_waived` edge to `presenting` is guarded on
`{ var: capstone, op: eq, value: false }`. Guard lookup falls back from context
to params, so the guard reads the knob fixed at `new`. A run created with
`capstone=true` shows the waive as blocked-with-reason; the agent cannot talk
itself into skipping the step *now* because that decision was only available at
instantiation.

### Pipeline with retry gates

A linear pipeline whose stages can *fail and loop back* is the shape of the
authoring machine (`.claude/skills/author-fsmp-workflow/fsmp-definition.yaml`):
`drafting → linting → dry_run → user_signoff → done`, where `lint_failed`,
`dryrun_failed`, and `changes_requested` each route back to `drafting`. Each gate
enforces "you may not advance until this stage actually passed": you can't reach
`dry_run` without a clean lint, can't reach `user_signoff` without a passing
dry-run, can't reach `done` without explicit acceptance. The forward edges are the
only way on, so a stage can't be skipped — only retried.

### `blocked_reason` as an active prompt

A blocked transition is still shown to the agent, with its `blocked_reason`.
Treat that string as a prompt, not an error message: say what's missing and what
to do about it, interpolating the live numbers. dev-cycle's `next_round` block
reads "bar of {bar} clean-initial reviewers already met (have
{clean_initial_count})…" — the agent reads that and re-orients. A blocked reason
is a free instruction; omitting it wastes one.

## Anti-patterns

Named, why they hurt, and the fix.

### 1. Guidance that narrates state instead of directing action

**Why:** `guidance` is the interface the agent acts on. "You are now in the
review phase" tells it where it is, not what to do, and a drifting agent needs the
next action. **Fix:** write every `guidance` as an imperative instruction —
"Send the SAME reviewer back to re-assess its own notes; on SATISFIED fire
`reviewer_satisfied`." Lead with the verb.

### 2. Expression-language creep in guards

**Why:** guards are structured, single comparisons combined with an implicit AND.
There is no `or`, no `not`, no arithmetic, no nesting — and deliberately so
(safe, parseable, no eval). Reaching for boolean algebra means you're fighting
the model. **Fix:** express alternatives as **separate transitions** (two edges
with different guards is an OR); express a multi-condition gate as multiple guards
on one transition (that's the AND); introduce a context latch set by an effect if
you need to remember something.

### 3. Counters that can't converge

**Why:** a `count >= bar` gate with no effect that ever increments `count` — or an
increment buried inside an `if` whose condition can never be true — is a gate that
never opens. The run wedges at the gate forever. The linter can't catch this
(it's a data-flow property, not a graph one). **Fix:** trace it by hand, then
**dry-run it** — walk the happy path in a throwaway home and confirm the gate
actually opens. For every `>=`/`gt` counter gate, find the `incr` that feeds it
and confirm nothing gates that `incr` shut.

### 4. Unreachable or dead-end states

**Why:** a state no transition targets is dead prose; a non-terminal state with no
exits strands the agent with no legal move. **Fix:** the linter **does** catch
both — `fsmp lint --def <path>` reports unreachable states, dead ends, unknown
targets, and terminals that still declare transitions. Run it; treat a non-clean
lint as a build break.

### 5. Rewriting the rails at runtime

**Why:** the whole point is that the definition is a fixed guardrail. A design
that expects the running agent to edit its own definition, add a state
mid-run, or otherwise mutate the machine defeats the guarantee — and the snapshot
at `new` makes it impossible anyway (edits don't reach a live instance). **Fix:**
put every branch the workflow might need into the definition **up front**. If a
run genuinely needs a shape the definition doesn't have, that's an `escalate` to a
human and a definition edit for the *next* run — not a live rewrite.

### 6. Optional-looking required steps

**Why:** "you may want to re-run the reviewer" for a step that is actually
mandatory invites the agent to skip it — exactly the omission the tool defends
against. **Fix:** make the required step the **only** valid transition out of its
state (so there is no other legal move), and phrase its guidance imperatively:
"Send the reviewer back in" — not "consider sending the reviewer back in."

### 7. Missing `blocked_reason`

**Why:** a guarded transition with no `blocked_reason` falls back to a generic
"a precondition is not yet met" — the agent sees the move is blocked but not what
would unblock it, and a prompt is wasted. **Fix:** give every guarded transition a
`blocked_reason` that names the missing condition and interpolates the live
numbers.

### 8. Confusing params and context

**Why:** params are set-once read-only knobs; context is mutable run state. Trying
to `incr` a param does nothing useful (effects only write context — and a
same-named context var would silently shadow the param in every later read),
while putting a genuine knob in context lets a stray `--data` or effect
mutate it mid-run. **Fix:** knobs the run is configured with at `new` → `params`;
anything that changes as the run progresses (counters, latches, captured urls) →
`context`.

### 9. Un-interpolated guidance that lies

**Why:** hard-coding "2 clean reviewers" in guidance means the text is wrong the
moment `bar` is set to 1 (or the count moves). Frozen numbers drift out of sync
with the machine's real state and mislead the agent. **Fix:** interpolate —
"{bar} clean reviewers", "{clean_initial_count} of {bar}", "PR {pr_url}" — so the
prompt always states the current truth.

## Worked example: dev-cycle

`.claude/skills/dev-cycle/fsmp-definition.yaml` is a complete, dogfooded
machine. Reading it top to bottom, here is where each pattern lives:

- **params vs. context.** `bar`, `round_ceiling`, and `capstone` are `params`
  (set at `new`, read-only). `clean_initial_count`, `round_count`,
  `initial_was_clean`, `pr_url`, and `findings_url` are `context` — everything
  that moves during the run.
- **`requires` + captured data.** `pr_opened` declares `requires: [pr_url]`; the
  url the caller supplies with `--data pr_url=…` merges into context and is then
  interpolated into later guidance ("reviewing PR {pr_url}").
- **The conditional increment.** In `awaiting_reassessment`, `reviewer_satisfied`
  carries an `if`/`then` effect: it increments `clean_initial_count` **only** when
  `initial_was_clean` is true. That's what makes a blocker-then-fixed reviewer not
  count toward the bar — the exact rule a naive counter would get wrong (see
  anti-pattern 3, inverted: here the guard on the increment is deliberate and
  reachable).
- **The counter gate.** In `round_complete`, `converge` is guarded on
  `clean_initial_count >= bar` with an interpolated `blocked_reason`; `next_round`
  is guarded on both `clean_initial_count < bar` and `round_count <
  round_ceiling`, and increments `round_count` as its effect. Convergence is
  enforced, not suggested.
- **The response-then-reassess loop.** `awaiting_review` routes both
  `verdict_clean_notes` and `verdict_changes` into `awaiting_impl_response` (a
  non-blocking note still owes a response) → `awaiting_reassessment` → back to the
  implementer on `reviewer_changes`, forward on `reviewer_satisfied`. There is
  deliberately no edge from `awaiting_review` straight to `presenting`.
- **The verification capstone.** `converge` lands in `verifying`, not
  `presenting` — the manual verification pass is a state, so it can't be
  omitted. `verification_failed` (which `requires: [findings_url]` and counts
  against `round_ceiling`) opens a narrow fix loop — `fixing` →
  `awaiting_fix_review` — whose only exit forward is back into `verifying`;
  `clean_initial_count` is untouched throughout, so prior convergence stands.
  The `verification_waived` shortcut is the param-guarded edge described above.
- **The escalate hatch.** Every waiting state offers `escalate → escalated`
  (a terminal state), so a stuck run always has a legal, recorded way out.
- **Terminal states.** `merged` and `escalated` are `terminal: true` with final
  guidance and no transitions.

For the authoring machine as a second worked example of the pipeline-with-retry
shape, see `.claude/skills/author-fsmp-workflow/fsmp-definition.yaml`.
