# Driving an fsmp machine

This is a short primer for the agent *driving* a machine. To author a definition,
see `fsmp guide definition`.

## The idea

You do not sequence the workflow from memory. You drive a state machine one
transition at a time, and on every call it re-injects the current step's
instruction. The returned text — the guidance for the state you're in, the moves
that are valid now, and the moves that are blocked and why — **is the interface
you act on**. Read it every step; don't run ahead of it.

## The loop

1. **Read the guidance.** `fsmp new` (first step) and every `fsmp do` print the
   current state's guidance: an imperative instruction for what to do *now*. Do
   that.
2. **Choose among the valid transitions.** Under "Valid transitions" are the only
   legitimate next moves. Pick the one matching what actually happened.
3. **Fire it.** `fsmp do <transition> --id <id>`. If the transition `requires`
   data, pass it: `fsmp do <transition> --id <id> --data key=value`.
4. **Repeat** from the new state's guidance, until you reach a terminal state
   ("this machine is complete").

```
fsmp new  --def <path> --id <id> [--set k=v ...]   # start a run; print the entry step
fsmp show --id <id>                                 # re-print the current step any time
fsmp do   <transition> --id <id> [--data k=v ...]   # attempt a move; print the new step
fsmp log  --id <id>                                 # the transition history so far
```

Give `--id` a **descriptive** value (e.g. `<project>-<issue>`), not a bare
counter — it ties the run to its work and keeps `fsmp show`/`log` legible. Keep
it; you pass it to every later call.

## Reading a "Blocked from here"

Below the valid moves, the machine lists transitions that are **blocked from
here** with the reason each is blocked. This is deliberate: it shows you the
tempting-but-not-yet-legal move and what would unlock it (e.g. "needs 2
clean-initial reviewers … currently 1"). Do **not** attempt a blocked move.
Instead, read the reason as an instruction for what to do first.

## A rejected `do` is itself a prompt

If you fire a move that isn't valid — unknown transition, missing required data,
or a failed guard — `fsmp do` exits non-zero and prints *why*, followed by the
same current-state view you'd get from `show`. That rejection is not just an
error; it re-orients you. Read the reason, then pick a move that is actually
offered. You never need to guess: the valid list is right there.

Because it exits non-zero, a programmatic driver can branch on the rejection
directly.

## Programmatic driving with `--json`

Add the global `--json` flag to any command to get the machine-readable view
instead of prose: the current `state`, `guidance`, the `valid` and `blocked`
transition lists (with reasons), and the run's `context` and `params`. Use it
when a script or tool is driving the machine rather than a human reading along;
the content is the same, shaped for a parser.

## What driving does *not* do

The machine tracks where you are and enforces which moves are legal next — it
does **not** do the work of a step (spawn an agent, open a PR, run a build). You
do that, then report it by firing the matching transition. And it enforces
**sequence, not content**: it can't tell whether you reported a transition
truthfully, so always fire the transition that matches what really happened —
the guarantee is only as good as your honesty about each step.
