# dev-cycle — an example fsmp-backed skill

This is a worked example of a skill that delegates its **process sequencing** to
`fsmp` while keeping the **content and judgment** (briefs, agent templates, review
DNA, escalation) in the skill prose.

- `SKILL.md` — the skill. Its "Driving the cycle with fsmp" section replaces the
  process-flow logic a monolithic skill would spell out; the machine owns *which
  step and when*.
- `machine-definition.yaml` — the state machine `SKILL.md` drives. This is the
  canonical definition; the fsmp integration tests (`tests/dev_cycle.rs`) run
  against this exact file, so the shipped guardrail is verified.

## The division of labour

| Concern | Owner |
|---|---|
| What step am I on; what's valid next; did I skip anything | `fsmp` (the state machine) |
| Convergence counting ("2 clean-initial reviewers") | `fsmp` (counter gate) |
| How to write the brief; the agent prompts; the review DNA | the skill prose |
| Whether a review was truly clean; escalation judgment | the orchestrator |

`fsmp` enforces the *sequence*, not the *content* — it can't tell whether a
reported transition is truthful, only that the agent passes through the required
states in order.

## Installing into a project

Copy this directory to the consuming project's `.claude/skills/dev-cycle/`, adapt
the pre-push gate and any project-specific wording in `SKILL.md`, and ensure
`fsmp` is on PATH. The `fsmp new --def …` command in `SKILL.md` already points at
`.claude/skills/dev-cycle/machine-definition.yaml`.
