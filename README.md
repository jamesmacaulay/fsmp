# fsmp — FSM Prompter

`fsmp` runs prompt-driven workflows backed by extended finite state machines.
Its primary user is an AI coding agent. The agent instantiates a human-authored
state machine and drives it one transition at a time; each call returns the
current step's instruction, the transitions that are valid now, and the ones
that are blocked and why.

The point is to keep an agent on a long workflow it would otherwise drift from.
Returning per-step instructions is not itself novel (workflow-runner MCP servers
do that); what `fsmp` adds is a real state machine underneath — cycles, counters,
and guards — so the workflow can express things a linear step-list can't, and so
the *sequence* is enforced rather than merely suggested.

## What it does

- Drives an agent through a workflow defined as states and transitions, re-stating
  the current step's instruction on every call.
- Enforces sequencing: from a given state, only the legitimate next transitions
  are available. This targets **omission** failures — an agent skipping a required
  step (e.g. "prompt the reviewer to re-assess after the implementer responds").
- Supports cyclic workflows (loops, retry rounds) and **counter gates** — e.g. a
  transition that stays blocked until "2 clean reviews" have been recorded.
- Names blocked transitions and the reason they're blocked, so a rejected move
  redirects the agent rather than just erroring.
- Keeps a transition log per instance as an audit trail.

## What it does not do

- **It does not enforce content, only sequence.** `fsmp` can't tell whether the
  agent reported a transition truthfully; it only guarantees the agent passes
  through the required states in order.
- **It's voluntary.** As a CLI it can't force the agent to call it or to follow
  the returned instruction. It makes the correct path the available, legible one;
  it is not a sandbox. (Hard enforcement would need an MCP + hook layer — see
  Status.)
- It does not run the workflow's actual steps (spawn agents, open PRs, etc.). It
  tracks where you are and what's allowed next; the caller does the work.
- The definition is a fixed guardrail — the agent drives a machine but does not
  author or modify one at runtime.

## Model

- **Definition** — static, human-authored, kept in version control. The file
  extension selects the parser (case-insensitively): `.yaml`/`.yml` for YAML,
  `.json` for JSON; any other extension (or none) is rejected. States +
  `params` (set once at `new`, read-only) + `context` (mutable) + transitions
  with guards and effects.
- **Instance** — a live run: a snapshot of the definition plus the current state,
  context, and transition log. Stored as JSON under `~/.fsmp/state/<id>/`, never
  in version control. The definition is snapshotted at `new`, so editing the
  source file (or switching branches) doesn't mutate a running machine.
  `FSMP_HOME` overrides the `~/.fsmp` home directory (which holds `state/`
  alongside siblings like an installed `bin/`).

Guards are structured comparisons (`{var, op, value|param|ctx}`), all of which
must hold (implicit AND). Effects are `set` / `incr` / `decr` / conditional.
There is no expression language; guards and effects are plain data.

## Commands

```
fsmp new  --def <path> [--id <id>] [--set k=v ...]   # instantiate; print the entry step
fsmp show --id <id>                                   # current state + valid/blocked transitions
fsmp do   <transition> --id <id> [--data k=v ...]     # attempt a transition; print the new step
fsmp log  --id <id>                                   # transition history
fsmp lint --def <path>                                # check a definition for authoring problems
```

`fsmp lint` reports every authoring problem in a definition at once — unknown
initial state, transition to an unknown state, unreachable state, dead-end
(non-terminal with no exits), and terminal state that still declares transitions
— and exits non-zero if any are found.

Add `--json` for a machine-readable view. A rejected `fsmp do` (unknown
transition, missing required data, or a failed guard) exits non-zero and prints
the reason followed by the current step.

## Example

`.claude/skills/dev-cycle/` is a worked example that this repo also dogfoods: a
skill (`SKILL.md`) that delegates its process sequencing to `fsmp`, plus the
`machine-definition.yaml` it drives. The skill points its orchestrator agent at
the definition:

```
fsmp new --def .claude/skills/dev-cycle/machine-definition.yaml --id myproj-1234 --set bar=2
```

and then drives `fsmp do <transition>` as the cycle progresses. The agent can't
reach `presenting` until `bar` separate reviewers have each opened with a clean
initial verdict and reached SATISFIED — a count the machine tracks rather than
the agent. See `.claude/skills/dev-cycle/README.md` for the division of labour
between the skill prose and the state machine.

## Status

v1. `new` / `show` / `do` / `log` work, and the dev-cycle definition drives a full
run (see `.claude/skills/dev-cycle/machine-definition.yaml`). Tested with
`cargo test` (unit tests inline; integration tests run the binary against that
definition).

Possible next steps: `ls` / `defs` inspection commands, and an `--mcp-stdio` mode
that exposes the same engine over MCP, where PreToolUse hooks could turn the
voluntary sequencing into hard gating. (`fsmp lint`, a definition linter for
unreachable / dead-end states, has since landed.)

## Build & install

```
make build       # debug build (target/debug/fsmp)
make test        # unit + integration tests
make check       # fmt-check + clippy + test
make install     # release build, installs to ~/.fsmp/bin/fsmp
```

`make install` mirrors the runtime layout — the binary lands in `~/.fsmp/bin/`
next to `~/.fsmp/state/`. Add it to your PATH:

```
export PATH="$HOME/.fsmp/bin:$PATH"
```

`FSMP_HOME` relocates both the binary and state (`make install FSMP_HOME=...`).
Run `make help` for the full target list; plain `cargo build` / `cargo test` also
work.
