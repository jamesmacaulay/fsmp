# CLAUDE.md

Guidance for AI agents working in this repository.

## What this is

`fsmp` (FSM Prompter) is a Rust CLI that runs prompt-driven workflows backed by
extended finite state machines. Its **primary user is an AI coding agent**: the
agent instantiates a human-authored state machine and drives one transition at a
time; each call returns the current step's instruction, the transitions valid
now, and the ones blocked and why.

The goal is to keep a drifting agent on a long workflow. Returning per-step
instructions is not itself novel (workflow-runner MCP servers do it); what `fsmp`
adds is a real state machine underneath — cycles, counters, guards — so the
sequence is enforced, not merely suggested, and the workflow can express things a
linear step-list can't.

The failure mode being defended against is **omission** — an agent skipping a
required step. `fsmp` enforces *sequencing, not content*: from a given state the
only valid transitions are the legitimate next steps, including counter gates
like "2 clean reviews before you may present." It cannot tell whether the agent
reported a transition truthfully, and as a CLI it can't force the agent to call
it — it makes the correct path the available one.

## Core design principles (do not erode these)

- **The returned text is the interface the agent acts on.** `src/render.rs`
  produces it. When changing behavior, ask first "what does the agent now read,
  and does it steer correctly?" Guidance strings are authored in the definition's
  YAML block scalars — keep them concrete (interpolate `{vars}`) and imperative.
- **Definitions are rigid guardrails.** They are static, human-authored, and in
  version control. The agent drives a machine; it does **not** author or mutate
  the definition. Don't add features that let a running agent rewrite its own
  rails.
- **Snapshot at `new`.** The definition is copied into the instance so editing
  the source file (or switching branches) can't mutate a running machine.
  Preserve this.
- **Structured guards/effects, no expression language.** Guards are plain-data
  comparisons (`{var, op, value|param|ctx}`), all-must-pass (implicit AND).
  Effects are `set`/`incr`/`decr`/conditional. Resist adding an eval mini-language
  — the small structured set covers these workflows and stays safe/parseable.
- **Sequencing, not content.** `fsmp` cannot tell if an agent lies in a
  transition; it guarantees the agent passes through the required *states*.
  Don't claim or design for content enforcement.

## Layout

| Path | Role |
|---|---|
| `src/model.rs` | Types: `Definition`, `Instance`, `State`, `Transition`, `Guard`, `Effect`, `Value`. |
| `src/engine.rs` | Guard evaluation, effect application, `{var}` interpolation (impls on `Instance`). |
| `src/render.rs` | Renders the step text the agent reads (`render`, `render_json`). |
| `src/lint.rs` | Definition linter: pure `lint(&Definition) -> Vec<Finding>` plus prose/JSON rendering. |
| `src/store.rs` | On-disk layout, load/save, definition parse (`parse_definition`) + validation. |
| `src/main.rs` | clap CLI: `new` / `show` / `do` / `log` / `lint` / `guide` (+ global `--json`). |
| `src/guide.rs` | `fsmp guide [topic]`: topic→text map over the `include_str!`'d `docs/`. |
| `docs/definition.md`, `docs/driving.md` | Single-source reference docs, compiled into the binary by `src/guide.rs`. `definition.md` = the format + patterns/anti-patterns; `driving.md` = the driving primer. |
| `.claude/skills/dev-cycle/machine-definition.yaml` | The reference workflow definition (implement-and-review). Canonical; the integration tests run against it. |
| `.claude/skills/dev-cycle/SKILL.md` | This repo's own dev-cycle skill (dogfooded); delegates process sequencing to `fsmp` and keeps content/judgment in prose. |
| `.claude/skills/author-workflow/` | The authoring skill + `authoring-machine.yaml` (a pipeline-with-retry-gates exemplar). Helps an agent + user AUTHOR a definition; also lint/dry-run-tested. |
| `tests/` | Integration tests that run the built binary against the example definitions (`dev_cycle.rs`, `authoring.rs`, `lint.rs`, `guide.rs`). |

## Model

- **Definition** — static, versioned. The loader keys the parser on a
  case-insensitive extension allowlist: `.yaml`/`.yml` → YAML (preferred, for
  readable prose + comments), `.json` → JSON; any other extension (or none) is a
  hard error naming the accepted set — the parser is never guessed from content.
  States + `params` (set once at `new`, read-only) + `context` (mutable) +
  guarded transitions with effects.
- **Instance** — a live run: snapshot of the definition + current state + context
  + transition log. Stored as JSON at `~/.fsmp/state/<id>/instance.json`.
  `FSMP_HOME` overrides the `~/.fsmp` home dir (which holds `state/` next to
  siblings like an installed `bin/`); the test suite sets it to a temp dir.

## Commands

```
fsmp new  --def <path> [--id <id>] [--set k=v ...]
fsmp show --id <id>
fsmp do   <transition> --id <id> [--data k=v ...]
fsmp log  --id <id>
fsmp lint --def <path>
fsmp guide [topic]
```

`fsmp lint` parses a definition (without instantiating it) and reports every
authoring problem at once — unknown initial state, transition to an unknown
state, unreachable state, dead-end (non-terminal with no exits), and terminal
state that still declares transitions — exiting non-zero if any are found.

`fsmp guide [topic]` prints the embedded reference docs to stdout (`definition`
and `driving`; no topic lists them, an unknown topic errors non-zero naming the
valid set). The docs live in `docs/*.md` and are `include_str!`'d — that markdown
is the single source of truth; the authoring skill cites `fsmp guide definition`
rather than restating the grammar. `--json` does not apply to `guide` (it's prose
to stdout).

A rejected `fsmp do` (unknown transition, missing required data, or a failed
guard) prints the reason followed by the current guidance and exits non-zero —
the rejection is itself a prompt.

## Working here

- Build: `cargo build`
- Test: `cargo test` (unit tests live inline under `#[cfg(test)]`; integration
  tests in `tests/` run the real binary via `CARGO_BIN_EXE_fsmp` with a temp
  `FSMP_HOME`, so they never touch a real `~/.fsmp`).
- Run: `cargo run -- <args>`, e.g.
  `cargo run -- new --def .claude/skills/dev-cycle/machine-definition.yaml --id demo --set bar=2`
- Lint: `cargo clippy`; format: `cargo fmt`.

When you change the engine or a shipped definition
(`.claude/skills/dev-cycle/machine-definition.yaml`,
`.claude/skills/author-workflow/authoring-machine.yaml`), **add/adjust an
integration test that drives it** — the behaviors these machines exist to
guarantee must stay covered: for dev-cycle, can't skip the
reviewer-response/re-assessment steps and can't `converge` before the counter bar
is met (`tests/dev_cycle.rs`); for author-workflow, can't skip the
lint/dry-run/sign-off gates and can't reach `done` without `accepted`
(`tests/authoring.rs`).

The reference docs are single-source: `docs/definition.md` and `docs/driving.md`
are the ONLY copy, compiled into the binary via `src/guide.rs`. When behavior
changes, edit the markdown (not a duplicate) and keep the authoring skill's
citations of `fsmp guide definition` accurate rather than restating the grammar.

## Status / not-yet-done

v1 skeleton. `fsmp lint` (definition linter: unreachable / dead-end states, plus
the structural checks) is done, as is `fsmp guide` (embedded authoring/driving
docs) with the author-workflow skill + machine for authoring definitions with a
user. Candidates: real man pages (`fsmp guide` is prose-to-stdout because fsmp
installs to a non-standard `~/.fsmp/bin` prefix — man generation/install is a
separate follow-up, issue #9), unit-test coverage growth, `fsmp ls`/`defs`
inspect commands, an `--mcp-stdio` mode exposing the engine over MCP for hard
hook-enforced gating (the CLI is voluntary by design). A linter
follow-up worth noting: detecting states whose transitions are *all* permanently
guard-blocked would need runtime guard evaluation and is deliberately out of the
current linter's scope. `serde_yaml` 0.9 is deprecated but functional — consider
`serde_yaml_ng` if it becomes a problem.
