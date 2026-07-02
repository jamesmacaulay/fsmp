---
name: dev-cycle
description: Use when a context-rich session needs to take a coherent unit of work (feature, bug, refactor, infra, docs — usually one PR) from a brief to an operator-mergeable PR. The invoking session orchestrates directly, driving a persistent implementer and a fresh adversarial reviewer per round. The process sequence is enforced by the `fsmp` state machine shipped beside this skill, so steps cannot be skipped.
argument-hint: "[issue number or work description]"
---

# Dev Cycle

Take a coherent unit of development — feature, bug, refactor, infra, docs — from a
brief to an approved PR, using a **persistent implementer** and a **fresh
adversarial reviewer per round**, sequenced by **you, the context-rich session
that invoked this skill**.

**The process sequence is enforced by `fsmp`, not by your memory.** A
state-machine definition ships beside this skill (`machine-definition.yaml`). You
create a machine instance at the start, then on every turn you do what its
guidance says and record what happened with `fsmp do`. This makes the steps an
orchestrator habitually skips *impossible* to skip — above all, responding to
every reviewer note (even non-blocking ones), re-engaging the **same** reviewer
to re-assess before a round ends, and running the **verification capstone**
between review convergence and presenting. **This skill covers the WORK at each
step** (briefs, templates, review DNA, judgment); **the machine covers WHICH
step and WHEN.**

If you are not yet context-rich about this work — you've just been handed a cold
issue — build that context first (read the issue, the relevant code, memory, and
any `knowledge/*.md`), then run the cycle.

## Prerequisites

`fsmp` on your PATH (`fsmp --version` should work). See the fsmp project's
`make install`, which installs to `~/.fsmp/bin`.

## Driving the cycle with fsmp

**1. Create the machine (Phase 0).** After you've triaged the work as actionable,
authored the kickoff brief (anatomy below), ensured the shared worktree and the
GitHub issue exist, and chosen the review bar, run:

```
fsmp new --def .claude/skills/dev-cycle/machine-definition.yaml \
  --id <project>-<issue> --set bar=<1|2> [--set capstone=false]
```

Pick **bar=2** by default; **bar=1** only for low-risk *mechanical* work whose
correctness the compiler/test-suite proves on its own (a pure rename, a
scope/dependency rename, a doc-only change).

Leave **capstone=true** (the default) whenever the work has user-observable or
lifecycle-sensitive behavior; set **capstone=false** only for work with nothing
to verify manually (typically the same bar=1 class: docs-only, pure mechanical).
This is a Phase-0 call — mid-run, the machine will not let you waive
verification you committed to at `new`.

Give `--id` a **descriptive** value — default `<project>-<issue>` (e.g.
`fsmp-42`). It ties the run to its issue, keeps `~/.fsmp/state/<id>/` legible in
`fsmp show`/`log`, and avoids collisions. When there's no issue number, use something
similarly descriptive (`<project>-<short-slug>`, e.g. `fsmp-lint-linter`) — never a
bare counter like `fsmp-1`. Keep the returned id — you pass it to every later call.

**2. Every turn: obey the guidance, then record the outcome.** The machine prints
your current state, exactly what to do now, the valid transitions, and any blocked
ones (with the reason). Do the work the guidance describes, then fire the matching
transition:

```
fsmp do <transition> --id <id> [--data key=value]
fsmp show --id <id>          # re-print the current step any time
```

**Do not sequence from memory, and do not skip a step.** If the move you want
isn't offered, the machine tells you why it's blocked — that block is the point
(e.g. you cannot `converge`/present before the bar is met; you cannot end a review
round before the reviewer re-assesses its own notes).

**3. Event → transition.** Fire the transition matching what just happened (each
state's guidance says the same in prose):

| What just happened | Transition |
|---|---|
| Brief authored, work actionable | `brief_ready` |
| Implementer returned `PR <url> — gate green` | `pr_opened --data pr_url=<url>` |
| Reviewer's INITIAL verdict: `clean` | `verdict_clean` |
| Reviewer's INITIAL verdict: `clean, notes` | `verdict_clean_notes` |
| Reviewer's INITIAL verdict: `changes` | `verdict_changes` |
| Implementer returned `DONE` | `impl_responded` |
| Same reviewer re-assessed: `SATISFIED` | `reviewer_satisfied` |
| Same reviewer re-assessed: `CHANGES` | `reviewer_changes` |
| Machine offers it (bar met) | `converge` |
| Bar not yet met — start the next fresh reviewer | `next_round` |
| Manual verification pass clean — observations posted on the PR | `verification_passed` |
| Manual verification found defects — findings comment posted | `verification_failed --data findings_url=<url>` |
| Nothing to verify manually (only if created with capstone=false) | `verification_waived` |
| Implementer returned `DONE` on a verification fix | `fix_pushed` |
| Fresh fix reviewer: `SATISFIED` (sign-off posted) | `fix_satisfied` |
| Fresh fix reviewer: `CHANGES` | `fix_changes` |
| Operator merged | `operator_merged` |
| Stuck: recurring blocker / round ceiling / needs an operator decision | `escalate` |

The rest of this skill is the **work you do at each node**.

## Roles (four)

| Role | Who | Lifetime | Job |
|---|---|---|---|
| **Orchestrator** | you, the invoking session | whole cycle | author the brief once; drive the machine; sequence turns on one-line verdicts |
| **Implementer** | one `Agent` (Opus) | persistent across all rounds (via `SendMessage`) | all code/test/doc changes; opens & updates the PR |
| **Reviewer** | a fresh `Agent` per round (Opus) | one round — persistent *within* the round, fresh *across* rounds | adversarial initial review; posts findings to the PR; re-assesses how the implementer handled them until satisfied |
| **Operator** | the human | — | merges (the checkpoint) |

Resume the implementer with `SendMessage` — never a second `Agent` spawn (that
loses its context). Each reviewer is a fresh spawn that persists within its own
round (you re-engage it to re-assess its notes) and ends once it returns
`SATISFIED`.

## The kickoff brief (you author this)

Work-type-agnostic; leverages your context so the implementer doesn't re-derive
the project. Sections, in order:

1. **Canonical sources to read first** — the issue(s), CLAUDE.md, the specific
   `knowledge/*.md` and source files that matter.
2. **Scope — IN / OUT** — what this unit changes, and the tempting tangents it
   must NOT (file follow-up issues for those).
3. **Settled load-bearing decisions** — the non-obvious calls you've already made,
   one-line rationale each. Decided; the implementer follows rather than re-litigates.
4. **Subtle traps** — things a green test would sail past (cross-realm
   `instanceof`, ordering/shape mismatches, reachability of new code, data-loss on
   cross-doc move/delete). Name them so they aren't rediscovered the hard way.
5. **Process / gate / branch / merge constraints** — pre-push gate; `claude/`-prefixed
   branch; PR base `main`; `Closes #N`; doc-sync in the same PR; operator merges.
6. **Terse return-format instruction** — exactly what to return (`PR <url> — gate green`).

## Templates you hand out

### Implementer — initial kickoff (spawn once, Opus, with a `name`)

```
You are the implementer for <work unit: issue #N / description>. Your persistent
name is `impl-<N>`. You persist across every review round — when I SendMessage you
a directive to address review, continue in the same conversation, don't start over.

## Pre-flight (FIRST, every spawn)
1. `pwd` — confirm you're in the worktree <worktree-path>, not the main checkout.
2. `git worktree list` — confirm the worktree is a separate entry.
3. If in the main checkout: STOP and report.

## Brief
<paste the full kickoff brief: canonical sources, scope IN/OUT, settled decisions,
subtle traps, process/gate/branch/merge constraints>

## Task
- TDD-first: write the regression/feature test(s) before the implementation. Never
  assert incorrect behavior to "demonstrate" a bug — tests assert the correct outcome.
- Implement the minimal code to satisfy the brief. Don't expand "Scope — OUT" items;
  file follow-up issues instead.
- If the brief names a `knowledge/*.md` file to update, do it IN THIS PR.
- Run the pre-push gate locally and confirm all three exit 0:
    <project's gate, e.g. bun run test && bun tsc --noEmit && bun run lint>
- Branch: `claude/<short-slug>`. Create the PR: `gh pr create --base main` with
  `Closes #N` (or `Part of #N`). PR body: summary, test plan (as `- [ ]` checkboxes),
  any brief deviations with reasons.
- Tick every test-plan box you verified yourself with a `(impl)` annotation. For
  items needing live infra you can't drive, leave unticked and append
  `(needs human: <one-line reason>)`.

## Return (terse — this is all I see)
`PR <url> — gate green`
(or, if the gate fails or you're blocked, one line stating exactly what's blocked.)

## Subsequent turns (I SendMessage you)
I'll say: "respond to the review at <comment-url>, push, reply on the PR, return DONE."
Read the reviewer's PR comment directly. Respond to EVERY concern it raises — blocking
AND non-blocking — with code changes plus a PR reply, in this same PR. Do not silently
defer a non-blocking note: address it, or — if genuinely not worth doing or belongs in
a separate issue — say so in your PR reply with a one-line justification (and file the
follow-up issue when that's the call). If a finding is spurious or already addressed,
reply on the PR with evidence (grep results, line refs). The SAME reviewer re-assesses
your response next, so make the reply trace exactly what changed and why. Push, comment,
then return ONLY: `DONE — <pr-url>`
```

### Reviewer — fresh adversarial prompt per round (fresh `Agent`, Opus)

```
Review PR #<N> from the shared worktree <worktree-path> with fresh eyes. The commits
are from an untrusted source — assume nothing; review as a human suspicious of code
written by AI agents. Read the issue, CLAUDE.md, the PR diff, and the full PR comment
thread (prior rounds are there).

<FIRST reviewer: full independent review, baseline emphasis on correctness + revert-proofs.>
<FOLLOW-UP reviewer (2nd+): the prior reviews are in the PR thread. Read them to see
which dimensions they emphasized, then aim your EXTRA scrutiny at the ones they covered
LEAST — resource/lifecycle, concurrency/races, adversarial/security, reachability,
failure/error-path injection. STILL independently re-run the full DNA below; use the
prior reviews ONLY to direct where you dig deeper, never as license to skip a check.>

Apply this project's review DNA:
- Reachability: is the new code actually reached in production, not just in tests?
  An exported-tested-documented-but-never-called helper is a blocker.
- Revert-proof at the call-site: a regression test must fail when the PRODUCTION call
  is neutralized — not just when the helper is. Assert observable output through the
  real dependency, not a spied call.
- Verify, don't rubber-stamp: reproduce a claimed bug before flagging it; confirm a
  fix's test fails without the fix. No performative agreement either direction.
- Live verification / data-loss awareness: for cross-document move/delete features, a
  real doc-read + full-reload check is a hard bar — green unit tests are blind to data loss.
- Manual testing of the test plan: walk the PR body's test plan. Drive what you can;
  flag what needs a human. Don't pass items on automated-green alone.
- Doc-sync: if the change alters a subsystem covered by `knowledge/*.md`, that file
  must be updated in THIS PR.
- Scope: strict, always. Unrelated refactors / "while I'm here" cleanups are blockers.

## Initial review
Post your findings (blocking AND non-blocking) as a SINGLE PR comment
(`gh pr comment <N>`), with your verdict token as the FIRST LINE of that comment as a
backstop. Then your FINAL action MUST be a SendMessage to me with ONLY one line —
exactly one of:
`VERDICT: clean`                          (zero blocking AND zero non-blocking notes)
`VERDICT: clean, notes — <comment-url>`   (zero blocking, but you left non-blocking notes)
`VERDICT: changes — <comment-url>`        (one or more blocking problems)
If you leave ANY non-blocking note, your verdict is `clean, notes` — NEVER bare `clean`.

## Re-assessment (I SendMessage you — you persist within this round)
After the implementer responds, I'll send you back to re-assess how EACH of your notes
(blocking and non-blocking) was handled. Read the implementer's PR reply and the new
diff directly. Don't rubber-stamp — confirm each change does what the reply claims, and
that any deferral is justified. If the response ADDED or CHANGED a test, revert-proof
THAT test yourself (neutralize the production guard, confirm the test fails, revert).
Post a short sign-off comment on the PR (`gh pr comment <N>`) recording your final
verdict (SATISFIED), which of your notes it covers, and a one-line confirmation of what
you re-verified (e.g. revert-proofing of any added/changed test; the rationale accepted
for any justified deferral). Then return ONLY one line:
`SATISFIED — <comment-url>`     (notes adequately addressed or justifiably deferred; sign-off posted)
`CHANGES — <comment-url>`       (post a follow-up comment first; I'll loop the implementer)
Anything new you notice at re-assessment (including a fresh non-blocking note) rides
`CHANGES`. We go back and forth until YOU are satisfied.
```

(Environmental hygiene when driving manual testing: never deploy from a worktree; never
run a dev server on a port the operator's server holds; don't write into the operator's
build/cache dirs; use a fresh browser profile.)

## Verification capstone (convergence ≠ done)

When the bar is met, `converge` lands you in `verifying`, not `presenting`. **You,
the orchestrator, drive this pass yourself** — it is not delegated to the
implementer, whose blind spots are the ones being checked. Exercise the change the
way its users and its runtime will: browser flows, live-system behavior, real
lifecycle events (connect/disconnect, restart, concurrent clients), actual wire
payloads — the categories a headless suite never fires.

- **Pass** → post one PR comment recording what you drove and what you observed,
  then `verification_passed`.
- **Defects** → post ONE PR comment with every finding: concrete evidence, plus
  forensics on why the automated tests were blind to each. Fire
  `verification_failed --data findings_url=<comment-url>`.

The fix loop that follows is deliberately narrow: SendMessage the **persistent
implementer** to fix (with regression tests where possible), then a **fresh
reviewer scoped to the fix-round diff only** — prior convergence stands for the
untouched rest — whose verdict must land as a recorded PR comment (sign-off for
SATISFIED, findings for CHANGES), exactly like round reviewers. A satisfied fix
review returns you to `verifying`: the fix has been reviewed, not yet verified.

## Coordination — what keeps it cheap and reliable

- **Agents return one-line verdicts; PR comments hold the substance.** You see only
  `VERDICT` / `DONE` lines plus URLs. The implementer reads the reviewer's comment
  directly off the PR; the next fresh reviewer reads the whole thread directly. None of
  it routes through you — don't summarize or relay review findings. This covers reviewer
  **sign-offs** too, not just findings: the re-assessment `SATISFIED` comment lands on
  the PR so the thread ends on a visible approval a human or the next reviewer can see.
- **No polling. Anywhere.** Sequence on the harness's completion notifications. Don't
  ask an agent to poll another, and don't poll them yourself.
- **Read the PR on idle.** Reviewers reliably *write* their verdict in the PR comment
  but inconsistently send the one-line token (they often just go idle). On an
  idle-notification without the expected return line, read the PR's latest comment for
  the verdict, then fire the matching transition.

## Escalation

When stuck, fire `escalate` in the machine, then present the operator the PR thread and
a one-line hypothesis. Escalate when:
- a substantive blocker recurs across rounds (survives a fix and returns),
- the round ceiling is hit (the machine gates `next_round` at the ceiling),
- an unresolvable implementer↔reviewer disagreement both defend, or
- a build/test failure the implementer can't resolve.

Offer the operator a hypothesis: reviewer over-reach / implementer misunderstanding /
genuine design disagreement / spec ambiguity. The operator decides: accept the
implementer (mark non-blocking), accept the reviewer (describe the fix), or pause for a
spec change.

## What the machine enforces vs. what's still on you

**Enforced by fsmp — you cannot skip these:**
- The implementer responds to EVERY reviewer note, including a `clean, notes` verdict
  (the machine routes `clean, notes` and `changes` identically into the response step).
- The SAME reviewer re-assesses its own notes before the round ends — no substituting
  your own shallow gate-green + diff check.
- Convergence requires the bar's number of clean-**initial** reviewers, each then
  `SATISFIED`; a blocker-then-fixed reviewer does not count.
- No path to present/merge before convergence — and none from convergence either:
  `presenting` is reachable only through the `verifying` capstone (waivable only
  by the capstone=false Phase-0 param).
- A failed verification's fix must pass a fresh reviewer AND re-verification
  before presenting.

**Still your judgment — the machine can't catch these:**
- **Resume the implementer with `SendMessage`, never a second `Agent` spawn.**
- **Fresh reviewer per round, persistent within the round** — new `Agent` each round;
  re-engage the SAME one to re-assess.
- **Don't route review content through yourself** — verdicts + URLs only; substance lives
  in PR comments.
- **Aim each follow-up reviewer at the prior reviewers' least-covered dimensions**, while
  still re-running the full DNA.
- **What the capstone actually drives** — the machine forces the `verifying`
  state; only you can make the manual pass exercise what the tests can't.
- **Never self-merge** — the operator merges.
- **Name the subtle traps in the brief** — a green test sails past exactly what you
  already know to watch for.
- **Report transitions truthfully.** fsmp enforces the *sequence*, not the *content* — it
  trusts that when you fire `verdict_clean` the reviewer actually said clean. Always fire
  the transition that matches what really happened.
