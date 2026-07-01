//! `fsmp` — FSM Prompter.
//!
//! A CLI whose primary user is an AI coding agent. The agent instantiates a
//! human-authored state machine and drives one transition at a time; every call
//! returns prose that re-injects the current step's instruction, lists the
//! valid moves, and names the blocked ones and why. See README for the model.

mod engine;
mod model;
mod render;
mod store;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use indexmap::IndexMap;
use model::{Instance, LogEntry, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "fsmp",
    version,
    about = "FSM Prompter — steer agents through workflows by re-prompting at each transition"
)]
struct Cli {
    /// Emit the machine-readable JSON view instead of prose.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Instantiate a machine from a definition file and print its entry prompt.
    New {
        /// Path to the definition (.yaml/.yml/.json).
        #[arg(long)]
        def: PathBuf,
        /// Instance id (e.g. `myproj-1234`). A UUID is minted if omitted.
        #[arg(long)]
        id: Option<String>,
        /// Override a param: `--set key=value` (repeatable).
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
    },
    /// Print the current state and valid/blocked transitions.
    Show {
        #[arg(long)]
        id: String,
    },
    /// Attempt a transition and print the resulting state's prompt.
    Do {
        /// Transition name.
        transition: String,
        #[arg(long)]
        id: String,
        /// Attach data: `--data key=value` (repeatable).
        #[arg(long = "data", value_name = "KEY=VALUE")]
        data: Vec<String>,
    },
    /// Print the transition history for an instance.
    Log {
        #[arg(long)]
        id: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<ExitCode> {
    match &cli.cmd {
        Cmd::New { def, id, set } => cmd_new(cli.json, def, id.as_deref(), set),
        Cmd::Show { id } => cmd_show(cli.json, id),
        Cmd::Do {
            transition,
            id,
            data,
        } => cmd_do(cli.json, transition, id, data),
        Cmd::Log { id } => cmd_log(cli.json, id),
    }
}

/// Parse repeated `key=value` args into an ordered map of coerced scalars.
fn parse_kv(pairs: &[String]) -> Result<IndexMap<String, Value>> {
    let mut map = IndexMap::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .with_context(|| format!("expected key=value, got `{p}`"))?;
        map.insert(k.to_string(), Value::parse_scalar(v));
    }
    Ok(map)
}

fn cmd_new(json: bool, def_path: &Path, id: Option<&str>, set: &[String]) -> Result<ExitCode> {
    let definition = store::load_definition(def_path)?;
    let id = id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if store::instance_exists(&id)? {
        bail!(
            "instance `{id}` already exists at {:?}",
            store::instance_dir(&id)?
        );
    }

    // Params: definition defaults, then --set overrides.
    let mut params = definition.params.clone();
    for (k, v) in parse_kv(set)? {
        params.insert(k, v);
    }

    let initial = definition.initial.clone();
    let inst = Instance {
        id: id.clone(),
        context: definition.context.clone(),
        params,
        current: initial.clone(),
        log: vec![LogEntry {
            seq: 0,
            transition: "new".to_string(),
            from: String::new(),
            to: initial,
            data: IndexMap::new(),
            at: Utc::now().to_rfc3339(),
        }],
        definition,
    };
    // Snapshot is now owned by the instance; persist it.
    store::save_instance(&inst)?;

    if json {
        print_json(&render::render_json(&inst));
    } else {
        let header = format!(
            "● created machine `{}`  →  state: {}",
            inst.id, inst.current
        );
        print!("{}", render::render(&inst, &header));
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_show(json: bool, id: &str) -> Result<ExitCode> {
    let inst = store::load_instance(id)?;
    if json {
        print_json(&render::render_json(&inst));
    } else {
        let header = format!("● state: {}", inst.current);
        print!("{}", render::render(&inst, &header));
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_do(json: bool, name: &str, id: &str, data: &[String]) -> Result<ExitCode> {
    let mut inst = store::load_instance(id)?;

    // Clone the current state so we can hold onto the transition while mutating
    // `inst` (the transition otherwise borrows `inst.definition`).
    let state = inst
        .definition
        .states
        .get(&inst.current)
        .cloned()
        .with_context(|| format!("current state `{}` missing from snapshot", inst.current))?;

    if state.terminal {
        return reject(
            json,
            &inst,
            &format!(
                "`{}` is a terminal state — the machine is complete; no transitions remain.",
                inst.current
            ),
        );
    }

    let t = match state.transitions.get(name) {
        Some(t) => t.clone(),
        None => {
            return reject(
                json,
                &inst,
                &format!(
                    "`{name}` is not a valid transition from `{}`. Pick one of the valid transitions below.",
                    inst.current
                ),
            );
        }
    };

    let provided = parse_kv(data)?;
    let missing: Vec<&String> = t
        .requires
        .iter()
        .filter(|r| !provided.contains_key(*r))
        .collect();
    if !missing.is_empty() {
        return reject(
            json,
            &inst,
            &format!(
                "transition `{name}` requires data: {}. Re-run with {}.",
                t.requires.join(", "),
                missing
                    .iter()
                    .map(|r| format!("--data {r}=<value>"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        );
    }

    if !inst.guards_pass(&t) {
        let reason = t
            .blocked_reason
            .as_deref()
            .map(|r| inst.interpolate(r))
            .unwrap_or_else(|| "a precondition is not yet met".to_string());
        return reject(
            json,
            &inst,
            &format!("transition `{name}` is blocked: {reason}"),
        );
    }

    // Commit: data merges into context, then effects apply.
    let from = inst.current.clone();
    for (k, v) in &provided {
        inst.context.insert(k.clone(), v.clone());
    }
    for e in &t.effects {
        inst.apply_effect(e);
    }
    inst.current = t.to.clone();
    inst.log.push(LogEntry {
        seq: inst.log.len(),
        transition: name.to_string(),
        from,
        to: t.to.clone(),
        data: provided,
        at: Utc::now().to_rfc3339(),
    });
    store::save_instance(&inst)?;

    if json {
        print_json(&render::render_json(&inst));
    } else {
        let header = format!("✔ {name}  →  state: {}", inst.current);
        print!("{}", render::render(&inst, &header));
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_log(json: bool, id: &str) -> Result<ExitCode> {
    let inst = store::load_instance(id)?;
    if json {
        print_json(&serde_json::to_value(&inst.log)?);
    } else {
        println!("history for `{}`:", inst.id);
        for e in &inst.log {
            let data = if e.data.is_empty() {
                String::new()
            } else {
                let kvs: Vec<String> = e.data.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!("  [{}]", kvs.join(", "))
            };
            let arrow = if e.from.is_empty() {
                e.to.clone()
            } else {
                format!("{} → {}", e.from, e.to)
            };
            println!(
                "  {:>3}. {:<20} {arrow}{data}  ({})",
                e.seq, e.transition, e.at
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A rejected `do`: same guidance view as `show`, prefixed with why the move
/// failed, exiting non-zero so a caller can branch on it. The rejection itself
/// is a prompt — it re-orients the agent rather than just erroring.
fn reject(json: bool, inst: &Instance, why: &str) -> Result<ExitCode> {
    if json {
        let mut v = render::render_json(inst);
        v.as_object_mut().unwrap().insert(
            "rejected".into(),
            serde_json::Value::String(why.to_string()),
        );
        print_json(&v);
    } else {
        let header = format!("✗ rejected: {why}\n\n● state: {}", inst.current);
        print!("{}", render::render(inst, &header));
    }
    Ok(ExitCode::FAILURE)
}

fn print_json(v: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::parse_kv;
    use crate::model::Value;

    #[test]
    fn parses_and_coerces_pairs_preserving_order() {
        let m = parse_kv(&[
            "bar=2".into(),
            "pr_url=https://x/1".into(),
            "on=true".into(),
        ])
        .unwrap();
        assert_eq!(m["bar"], Value::Int(2));
        assert_eq!(m["pr_url"], Value::Str("https://x/1".into()));
        assert_eq!(m["on"], Value::Bool(true));
        assert_eq!(m.keys().collect::<Vec<_>>(), vec!["bar", "pr_url", "on"]);
    }

    #[test]
    fn splits_only_on_the_first_equals() {
        // A value containing '=' (e.g. a query string) must survive intact.
        let m = parse_kv(&["url=https://x/1?a=b".into()]).unwrap();
        assert_eq!(m["url"], Value::Str("https://x/1?a=b".into()));
    }

    #[test]
    fn rejects_a_fragment_without_equals() {
        assert!(parse_kv(&["novalue".into()]).is_err());
    }
}
