//! Data model for machine definitions and running instances.
//!
//! A **definition** is static, human-authored, and lives in version control
//! alongside the workflow it guards. An **instance** is a live run: a snapshot
//! of the definition plus the current state, mutable context, and a transition
//! log. Instances live under `~/.fsmp/state/<id>/` and are never in version
//! control.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// A scalar value used for params, context variables, and transition data.
///
/// `untagged` deserialization tries the variants in order, so an unquoted YAML
/// `true` becomes `Bool`, `2` becomes `Int`, and everything else `Str`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Str(s) => write!(f, "{s}"),
        }
    }
}

impl Value {
    /// Interpret this value as an integer where possible (ints, or numeric strings).
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Str(s) => s.parse().ok(),
            Value::Bool(_) => None,
        }
    }

    /// Coerce a raw `key=value` string fragment into the most specific scalar type.
    pub fn parse_scalar(s: &str) -> Value {
        match s {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => match s.parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => Value::Str(s.to_string()),
            },
        }
    }
}

/// Comparison operator for a guard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

/// A single structured comparison. No expression language — just
/// `<var> <op> <rhs>`, where the right-hand side is a literal `value`, a
/// read-only `param`, or another context variable `ctx`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guard {
    pub var: String,
    pub op: Op,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub ctx: Option<String>,
}

/// A mutation applied to context when a transition fires. `untagged`, so the
/// shape in the definition selects the variant; `Cond` is tried first because
/// it is the only one carrying `if`/`then`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Effect {
    /// Apply `then` only when the guard holds (e.g. count a reviewer only if
    /// its initial verdict was clean).
    Cond {
        #[serde(rename = "if")]
        cond: Guard,
        then: Box<Effect>,
    },
    Set {
        set: String,
        to: Value,
    },
    Incr {
        incr: String,
    },
    Decr {
        decr: String,
    },
}

/// An edge out of a state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition {
    /// Target state name.
    pub to: String,
    /// One-line "take this when …" shown in the valid-transition list.
    #[serde(default)]
    pub when: Option<String>,
    /// Why this move is blocked when its guards fail — shown in the
    /// blocked-from-here list. Interpolated. Falls back to a generic line.
    #[serde(default)]
    pub blocked_reason: Option<String>,
    /// All guards must pass for the transition to be available (implicit AND).
    #[serde(default)]
    pub guards: Vec<Guard>,
    /// `--data` keys that must be supplied when firing this transition.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Context mutations applied when the transition fires.
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// A node: the prose re-injected on arrival plus the edges out.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    /// The just-in-time prompt for this state. Interpolated with `{var}`.
    #[serde(default)]
    pub guidance: String,
    #[serde(default)]
    pub terminal: bool,
    #[serde(default)]
    pub transitions: IndexMap<String, Transition>,
}

/// A static, human-authored workflow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Definition {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Read-only after `new`; set from defaults + `--set` overrides.
    #[serde(default)]
    pub params: IndexMap<String, Value>,
    /// Initial values for the mutable run context.
    #[serde(default)]
    pub context: IndexMap<String, Value>,
    pub initial: String,
    pub states: IndexMap<String, State>,
}

/// One recorded step in an instance's history.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: usize,
    pub transition: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub data: IndexMap<String, Value>,
    pub at: String,
}

/// A live run of a definition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    /// Snapshot taken at `new` — stable against later edits to the source file.
    pub definition: Definition,
    pub params: IndexMap<String, Value>,
    pub context: IndexMap<String, Value>,
    pub current: String,
    pub log: Vec<LogEntry>,
}

#[cfg(test)]
mod tests {
    use super::Value;

    #[test]
    fn parse_scalar_picks_the_most_specific_type() {
        assert_eq!(Value::parse_scalar("true"), Value::Bool(true));
        assert_eq!(Value::parse_scalar("false"), Value::Bool(false));
        assert_eq!(Value::parse_scalar("42"), Value::Int(42));
        assert_eq!(Value::parse_scalar("-3"), Value::Int(-3));
        assert_eq!(Value::parse_scalar("hello"), Value::Str("hello".into()));
        // A URL is not an int and must stay a string.
        assert_eq!(
            Value::parse_scalar("https://x/1"),
            Value::Str("https://x/1".into())
        );
    }

    #[test]
    fn as_int_coerces_numeric_strings_only() {
        assert_eq!(Value::Int(7).as_int(), Some(7));
        assert_eq!(Value::Str("7".into()).as_int(), Some(7));
        assert_eq!(Value::Str("seven".into()).as_int(), None);
        assert_eq!(Value::Bool(true).as_int(), None);
    }
}
