//! Transition evaluation: guard checks, effect application, and value
//! resolution/interpolation against an instance's params + context.

use crate::model::{Effect, Guard, Instance, Op, Transition, Value};

impl Instance {
    /// Look up a name in context first, then params. Context shadows params
    /// (a run can't mutate params, but a context var may share a name).
    pub fn resolve(&self, name: &str) -> Option<Value> {
        self.context
            .get(name)
            .or_else(|| self.params.get(name))
            .cloned()
    }

    fn rhs(&self, g: &Guard) -> Option<Value> {
        if let Some(v) = &g.value {
            Some(v.clone())
        } else if let Some(p) = &g.param {
            self.params.get(p).cloned()
        } else if let Some(c) = &g.ctx {
            self.resolve(c)
        } else {
            None
        }
    }

    pub fn eval_guard(&self, g: &Guard) -> bool {
        let lhs = self.resolve(&g.var);
        let rhs = self.rhs(g);
        match g.op {
            Op::Eq => lhs == rhs,
            Op::Ne => lhs != rhs,
            Op::Lt | Op::Lte | Op::Gt | Op::Gte => {
                let (a, b) = match (lhs.and_then(|v| v.as_int()), rhs.and_then(|v| v.as_int())) {
                    (Some(a), Some(b)) => (a, b),
                    // Ordered comparison against a non-numeric operand is never satisfied.
                    _ => return false,
                };
                match g.op {
                    Op::Lt => a < b,
                    Op::Lte => a <= b,
                    Op::Gt => a > b,
                    Op::Gte => a >= b,
                    _ => unreachable!(),
                }
            }
        }
    }

    /// A transition is available only when every guard passes (implicit AND).
    pub fn guards_pass(&self, t: &Transition) -> bool {
        t.guards.iter().all(|g| self.eval_guard(g))
    }

    pub fn apply_effect(&mut self, e: &Effect) {
        match e {
            Effect::Set { set, to } => {
                self.context.insert(set.clone(), to.clone());
            }
            Effect::Incr { incr } => {
                let n = self.resolve(incr).and_then(|v| v.as_int()).unwrap_or(0) + 1;
                self.context.insert(incr.clone(), Value::Int(n));
            }
            Effect::Decr { decr } => {
                let n = self.resolve(decr).and_then(|v| v.as_int()).unwrap_or(0) - 1;
                self.context.insert(decr.clone(), Value::Int(n));
            }
            Effect::Cond { cond, then } => {
                if self.eval_guard(cond) {
                    self.apply_effect(then);
                }
            }
        }
    }

    /// Replace `{name}` tokens with the resolved value; unknown names are left
    /// verbatim so a stray brace never silently vanishes. UTF-8 safe.
    pub fn interpolate(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            match after.find('}') {
                Some(close) => {
                    let key = &after[..close];
                    let is_ident =
                        !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_');
                    match (is_ident, self.resolve(key)) {
                        (true, Some(v)) => out.push_str(&v.to_string()),
                        // Not an identifier, or no such var: emit the token verbatim.
                        _ => {
                            out.push('{');
                            out.push_str(key);
                            out.push('}');
                        }
                    }
                    rest = &after[close + 1..];
                }
                // Unbalanced brace: emit the rest as-is and stop.
                None => {
                    out.push('{');
                    out.push_str(after);
                    return out;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::model::*;
    use indexmap::IndexMap;

    fn inst(ctx: &[(&str, Value)], params: &[(&str, Value)]) -> Instance {
        let map = |pairs: &[(&str, Value)]| {
            let mut m = IndexMap::new();
            for (k, v) in pairs {
                m.insert(k.to_string(), v.clone());
            }
            m
        };
        Instance {
            id: "t".into(),
            definition: Definition {
                name: "t".into(),
                description: None,
                params: IndexMap::new(),
                context: IndexMap::new(),
                initial: "s".into(),
                states: IndexMap::new(),
            },
            params: map(params),
            context: map(ctx),
            current: "s".into(),
            log: vec![],
        }
    }

    fn guard(
        var: &str,
        op: Op,
        value: Option<Value>,
        param: Option<&str>,
        ctx: Option<&str>,
    ) -> Guard {
        Guard {
            var: var.into(),
            op,
            value,
            param: param.map(str::to_string),
            ctx: ctx.map(str::to_string),
        }
    }

    #[test]
    fn eq_and_ne_against_literal() {
        let i = inst(&[("x", Value::Int(3))], &[]);
        assert!(i.eval_guard(&guard("x", Op::Eq, Some(Value::Int(3)), None, None)));
        assert!(!i.eval_guard(&guard("x", Op::Eq, Some(Value::Int(4)), None, None)));
        assert!(i.eval_guard(&guard("x", Op::Ne, Some(Value::Int(4)), None, None)));
    }

    #[test]
    fn ordered_compare_against_param() {
        let i = inst(&[("count", Value::Int(1))], &[("bar", Value::Int(2))]);
        assert!(i.eval_guard(&guard("count", Op::Lt, None, Some("bar"), None)));
        assert!(!i.eval_guard(&guard("count", Op::Gte, None, Some("bar"), None)));
    }

    #[test]
    fn ordered_compare_against_nonnumeric_is_false() {
        let i = inst(&[("x", Value::Str("hi".into()))], &[]);
        assert!(!i.eval_guard(&guard("x", Op::Gt, Some(Value::Int(0)), None, None)));
    }

    #[test]
    fn compare_against_other_context_var() {
        let i = inst(&[("a", Value::Bool(true)), ("b", Value::Bool(true))], &[]);
        assert!(i.eval_guard(&guard("a", Op::Eq, None, None, Some("b"))));
    }

    #[test]
    fn guards_pass_is_implicit_and() {
        let i = inst(&[("x", Value::Int(5))], &[]);
        let t = Transition {
            to: "s".into(),
            when: None,
            blocked_reason: None,
            requires: vec![],
            effects: vec![],
            guards: vec![
                guard("x", Op::Gt, Some(Value::Int(0)), None, None),
                guard("x", Op::Lt, Some(Value::Int(3)), None, None), // fails
            ],
        };
        assert!(!i.guards_pass(&t));
    }

    #[test]
    fn incr_from_absent_defaults_to_one() {
        let mut i = inst(&[], &[]);
        i.apply_effect(&Effect::Incr { incr: "n".into() });
        assert_eq!(i.resolve("n"), Some(Value::Int(1)));
        i.apply_effect(&Effect::Decr { decr: "n".into() });
        assert_eq!(i.resolve("n"), Some(Value::Int(0)));
    }

    #[test]
    fn conditional_effect_respects_guard() {
        // Mirrors the dev-cycle "count only clean-initial reviewers" rule.
        let mut clean = inst(
            &[
                ("initial_was_clean", Value::Bool(true)),
                ("c", Value::Int(0)),
            ],
            &[],
        );
        let mut dirty = inst(
            &[
                ("initial_was_clean", Value::Bool(false)),
                ("c", Value::Int(0)),
            ],
            &[],
        );
        let eff = Effect::Cond {
            cond: guard(
                "initial_was_clean",
                Op::Eq,
                Some(Value::Bool(true)),
                None,
                None,
            ),
            then: Box::new(Effect::Incr { incr: "c".into() }),
        };
        clean.apply_effect(&eff);
        dirty.apply_effect(&eff);
        assert_eq!(clean.resolve("c"), Some(Value::Int(1)));
        assert_eq!(dirty.resolve("c"), Some(Value::Int(0)));
    }

    #[test]
    fn interpolation_known_unknown_unicode_and_unbalanced() {
        let i = inst(
            &[
                ("pr_url", Value::Str("https://x/1".into())),
                ("n", Value::Int(2)),
            ],
            &[],
        );
        assert_eq!(
            i.interpolate("PR {pr_url} — {n} of {bar}"),
            "PR https://x/1 — 2 of {bar}"
        );
        // Unicode in the surrounding text survives; a lone brace is emitted verbatim.
        assert_eq!(i.interpolate("✔ done {n"), "✔ done {n");
        assert_eq!(i.interpolate("{ not_an_ident }"), "{ not_an_ident }");
    }
}
