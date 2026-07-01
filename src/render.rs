//! The prose renderer. This is the product: what the agent reads after every
//! call. It states where the machine is, re-injects the current step's
//! instruction, lists only the currently-valid moves, and — crucially — names
//! the tempting-but-blocked moves and why they're blocked.

use crate::model::{Instance, State, Transition};
use serde_json::json;

/// A short label for a transition, shown in both the valid and blocked lists.
fn transition_label(name: &str, t: &Transition, inst: &Instance) -> String {
    let mut label = format!("  {name}");
    if !t.requires.is_empty() {
        label.push_str(&format!("  (requires: {})", t.requires.join(", ")));
    }
    if let Some(when) = &t.when {
        label.push_str(&format!("  — {}", inst.interpolate(when)));
    }
    label
}

fn blocked_label(name: &str, t: &Transition, inst: &Instance) -> String {
    let reason = t
        .blocked_reason
        .as_deref()
        .map(|r| inst.interpolate(r))
        .unwrap_or_else(|| "a precondition is not yet met".to_string());
    format!("  {name} → {} — {reason}", t.to)
}

/// Render the full guidance block for the instance's current state.
///
/// `header` is the one-line banner (a transition confirmation, or a plain
/// "state" line for `show`).
pub fn render(inst: &Instance, header: &str) -> String {
    let state: &State = inst
        .definition
        .states
        .get(&inst.current)
        .expect("current state exists in snapshot");

    let mut out = String::new();
    out.push_str(header);
    out.push_str("\n\n");
    out.push_str(inst.interpolate(&state.guidance).trim_end());
    out.push('\n');

    if state.terminal {
        out.push_str("\n(terminal state — this machine is complete)\n");
        return out;
    }

    let (valid, blocked): (Vec<_>, Vec<_>) = state
        .transitions
        .iter()
        .partition(|(_, t)| inst.guards_pass(t));

    out.push_str("\nValid transitions:\n");
    if valid.is_empty() {
        out.push_str("  (none — this state has no currently-available moves)\n");
    } else {
        for (name, t) in &valid {
            out.push_str(&transition_label(name, t, inst));
            out.push('\n');
        }
    }

    if !blocked.is_empty() {
        out.push_str("\nBlocked from here (do not attempt — the precondition is not met):\n");
        for (name, t) in &blocked {
            out.push_str(&blocked_label(name, t, inst));
            out.push('\n');
        }
    }

    out
}

/// Machine-readable equivalent of `render`, for `--json` consumers.
pub fn render_json(inst: &Instance) -> serde_json::Value {
    let state = inst
        .definition
        .states
        .get(&inst.current)
        .expect("current state exists in snapshot");

    let mut valid = Vec::new();
    let mut blocked = Vec::new();
    for (name, t) in &state.transitions {
        if inst.guards_pass(t) {
            valid.push(json!({
                "name": name,
                "to": t.to,
                "when": t.when.as_ref().map(|w| inst.interpolate(w)),
                "requires": t.requires,
            }));
        } else {
            blocked.push(json!({
                "name": name,
                "to": t.to,
                "reason": t.blocked_reason.as_ref()
                    .map(|r| inst.interpolate(r))
                    .unwrap_or_else(|| "a precondition is not yet met".to_string()),
            }));
        }
    }

    json!({
        "id": inst.id,
        "state": inst.current,
        "guidance": inst.interpolate(&state.guidance),
        "terminal": state.terminal,
        "valid": valid,
        "blocked": blocked,
        "context": inst.context,
        "params": inst.params,
    })
}
