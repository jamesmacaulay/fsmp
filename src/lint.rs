//! Definition linter. Unlike `store::validate` (which hard-fails at the first
//! structural error so a broken guardrail can't be instantiated), the linter
//! collects *all* authoring problems in a parseable definition at once and adds
//! reachability analysis that `new` does not do. It operates on an already-parsed
//! `Definition` and never touches disk, so `lint` is a pure, unit-testable
//! function; the CLI feeds it `store::parse_definition` output.

use crate::model::Definition;
use serde::Serialize;
use std::collections::{HashSet, VecDeque};

/// A single authoring problem found in a definition. Each variant carries enough
/// location to render a precise, actionable message and to serialize under
/// `--json`. `snake_case` tagging yields stable `kind` strings for consumers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Finding {
    /// `initial` names a state that isn't defined.
    UnknownInitial { initial: String },
    /// A transition `to:` targets a state that isn't defined.
    UnknownTarget {
        state: String,
        transition: String,
        target: String,
    },
    /// A state with no path from `initial` along transition targets.
    UnreachableState { state: String },
    /// A non-terminal state with no outgoing transitions — the agent arrives
    /// with no valid move and gets stuck.
    DeadEnd { state: String },
    /// A `terminal: true` state that also declares transitions; `do` refuses
    /// moves from a terminal state, so those edges are dead code.
    TerminalWithTransitions { state: String },
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::UnknownInitial { initial } => {
                write!(f, "initial: `{initial}` is not a defined state")
            }
            Finding::UnknownTarget {
                state,
                transition,
                target,
            } => write!(
                f,
                "{state}.{transition}: targets unknown state `{target}` — point it at a defined state"
            ),
            Finding::UnreachableState { state } => write!(
                f,
                "{state}: unreachable — no path to it from the initial state"
            ),
            Finding::DeadEnd { state } => write!(
                f,
                "{state}: dead end — non-terminal with no outgoing transitions (mark it `terminal: true` or add a transition)"
            ),
            Finding::TerminalWithTransitions { state } => write!(
                f,
                "{state}: terminal state declares transitions — dead code, since `do` refuses moves from a terminal state"
            ),
        }
    }
}

/// Analyze a parsed definition and return every problem found, in a
/// deterministic order: checks are emitted in a fixed sequence, and within each
/// check states/transitions are visited in definition (`IndexMap`) order.
pub fn lint(def: &Definition) -> Vec<Finding> {
    let mut findings = Vec::new();
    let initial_valid = def.states.contains_key(&def.initial);

    // 1. Unknown initial state.
    if !initial_valid {
        findings.push(Finding::UnknownInitial {
            initial: def.initial.clone(),
        });
    }

    // 2. Transitions targeting an undefined state.
    for (sname, state) in &def.states {
        for (tname, t) in &state.transitions {
            if !def.states.contains_key(&t.to) {
                findings.push(Finding::UnknownTarget {
                    state: sname.clone(),
                    transition: tname.clone(),
                    target: t.to.clone(),
                });
            }
        }
    }

    // 3. Unreachable states. Only meaningful with a valid entry point; when
    //    `initial` is unknown we skip this rather than flag every state (the
    //    UnknownInitial finding above is the actionable problem to fix first).
    if initial_valid {
        let reachable = reachable_from_initial(def);
        for sname in def.states.keys() {
            if !reachable.contains(sname) {
                findings.push(Finding::UnreachableState {
                    state: sname.clone(),
                });
            }
        }
    }

    // 4. Dead ends: a non-terminal state with no way out.
    for (sname, state) in &def.states {
        if !state.terminal && state.transitions.is_empty() {
            findings.push(Finding::DeadEnd {
                state: sname.clone(),
            });
        }
    }

    // 5. Terminal states carrying (unreachable) transitions.
    for (sname, state) in &def.states {
        if state.terminal && !state.transitions.is_empty() {
            findings.push(Finding::TerminalWithTransitions {
                state: sname.clone(),
            });
        }
    }

    findings
}

/// BFS the set of state names reachable from `initial` by following transition
/// `to:` edges (guards ignored — a guarded edge still makes its target reachable
/// in principle). Only traverses edges whose target is a defined state, so a
/// bogus `to:` never pollutes the reachable set. A terminal state's out-edges are
/// NOT traversed: `do` refuses moves from a terminal state, so those edges are
/// dead code (and separately flagged `TerminalWithTransitions`), and a state
/// reachable *only* through one is genuinely unreachable at runtime. Caller
/// ensures `initial` exists.
fn reachable_from_initial(def: &Definition) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(def.initial.clone());
    queue.push_back(def.initial.clone());
    while let Some(name) = queue.pop_front() {
        if let Some(state) = def.states.get(&name) {
            if state.terminal {
                continue;
            }
            for t in state.transitions.values() {
                if def.states.contains_key(&t.to) && visited.insert(t.to.clone()) {
                    queue.push_back(t.to.clone());
                }
            }
        }
    }
    visited
}

/// Render findings as prose: one finding per line, then a summary line.
pub fn render_prose(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "clean — no problems found\n".to_string();
    }
    let mut out = String::new();
    for f in findings {
        out.push_str(&format!("{f}\n"));
    }
    let n = findings.len();
    out.push_str(&format!(
        "\n{n} problem{} found\n",
        if n == 1 { "" } else { "s" }
    ));
    out
}

/// Machine-readable equivalent of `render_prose`, for `--json` consumers.
pub fn to_json(findings: &[Finding]) -> serde_json::Value {
    serde_json::json!({
        "ok": findings.is_empty(),
        "findings": findings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a definition from YAML in-memory (no disk), mirroring how the
    /// real parser deserializes a definition file.
    fn def(yaml: &str) -> Definition {
        serde_yaml::from_str(yaml).expect("test definition should parse")
    }

    #[test]
    fn a_clean_definition_produces_no_findings() {
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: b }
  b:
    terminal: true
");
        assert_eq!(lint(&d), vec![]);
        assert!(to_json(&lint(&d))["ok"].as_bool().unwrap());
        assert!(render_prose(&lint(&d)).contains("clean"));
    }

    #[test]
    fn flags_an_unknown_initial_state() {
        let d = def("\
name: t
initial: nope
states:
  a:
    terminal: true
");
        assert_eq!(
            lint(&d),
            vec![Finding::UnknownInitial {
                initial: "nope".into()
            }]
        );
    }

    #[test]
    fn flags_a_transition_to_an_unknown_state() {
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: ghost }
");
        assert_eq!(
            lint(&d),
            vec![Finding::UnknownTarget {
                state: "a".into(),
                transition: "go".into(),
                target: "ghost".into()
            }]
        );
    }

    #[test]
    fn flags_an_unreachable_state() {
        // `b` is defined but nothing reaches it. `a` has a self-loop, so it is
        // not a dead end — keeping this case to exactly the one unreachable
        // finding.
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      loop: { to: a }
  b:
    terminal: true
");
        assert_eq!(
            lint(&d),
            vec![Finding::UnreachableState { state: "b".into() }]
        );
    }

    #[test]
    fn flags_a_dead_end_but_not_a_terminal_with_no_transitions() {
        // `dead` is non-terminal with no exits → DeadEnd. `done` is terminal with
        // no exits → correct, no finding.
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      stop: { to: done }
      stuck: { to: dead }
  dead: {}
  done:
    terminal: true
");
        assert_eq!(
            lint(&d),
            vec![Finding::DeadEnd {
                state: "dead".into()
            }]
        );
    }

    #[test]
    fn flags_a_terminal_state_that_declares_transitions() {
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      done: { to: end }
  end:
    terminal: true
    transitions:
      back: { to: a }
");
        assert_eq!(
            lint(&d),
            vec![Finding::TerminalWithTransitions {
                state: "end".into()
            }]
        );
    }

    #[test]
    fn reports_an_unknown_target_and_an_unreachable_state_together() {
        // Proves lint collects rather than dying on the first structural error:
        // a's edge points nowhere (UnknownTarget) AND b is unreachable.
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: ghost }
  b:
    terminal: true
");
        let f = lint(&d);
        assert!(f.contains(&Finding::UnknownTarget {
            state: "a".into(),
            transition: "go".into(),
            target: "ghost".into()
        }));
        assert!(f.contains(&Finding::UnreachableState { state: "b".into() }));
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn unknown_initial_does_not_cascade_into_unreachable_for_every_state() {
        // With no valid entry point, reachability is undefined; we emit the one
        // actionable UnknownInitial finding and do not flood every state.
        let d = def("\
name: t
initial: nope
states:
  a:
    transitions:
      go: { to: b }
  b:
    terminal: true
");
        let f = lint(&d);
        assert_eq!(
            f,
            vec![Finding::UnknownInitial {
                initial: "nope".into()
            }]
        );
    }

    #[test]
    fn a_state_reachable_only_through_a_broken_edge_is_still_unreachable() {
        // a → ghost (broken); c is only "reachable" via ghost, which isn't a real
        // node, so c stays unreachable. UnknownTarget and UnreachableState are
        // distinct findings, not a cascade.
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: ghost }
  c:
    terminal: true
");
        let f = lint(&d);
        assert_eq!(f.len(), 2);
        assert!(f.contains(&Finding::UnknownTarget {
            state: "a".into(),
            transition: "go".into(),
            target: "ghost".into()
        }));
        assert!(f.contains(&Finding::UnreachableState { state: "c".into() }));
    }

    #[test]
    fn a_state_reachable_only_through_a_terminal_states_dead_edge_is_unreachable() {
        // `do` never fires a transition out of a terminal state, so `t`'s edge to
        // `x` is dead code. Reachability must not traverse it: `x` is reachable
        // ONLY through that dead edge, so it is genuinely unreachable at runtime.
        // `t` is independently flagged TerminalWithTransitions.
        let d = def("\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: t }
  t:
    terminal: true
    transitions:
      dead: { to: x }
  x:
    terminal: true
");
        let f = lint(&d);
        assert!(
            f.contains(&Finding::UnreachableState { state: "x".into() }),
            "x should be flagged unreachable, got: {f:?}"
        );
        assert!(f.contains(&Finding::TerminalWithTransitions { state: "t".into() }));
        assert_eq!(f.len(), 2);
    }

    #[test]
    fn prose_summary_is_singular_for_one_and_plural_for_many() {
        let one = vec![Finding::DeadEnd { state: "x".into() }];
        assert!(render_prose(&one).contains("1 problem found"));
        let two = vec![
            Finding::DeadEnd { state: "x".into() },
            Finding::DeadEnd { state: "y".into() },
        ];
        assert!(render_prose(&two).contains("2 problems found"));
    }
}
