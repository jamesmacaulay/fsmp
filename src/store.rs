//! On-disk layout. Definitions come from wherever the caller points (version
//! control, a skill dir); instance state lives under `~/.asm/<id>/`.

use crate::model::{Definition, Instance};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Root under which all instance state lives. `FSMP_HOME` overrides the default
/// `~/.fsmp` (used by the test suite to avoid touching a real home dir).
pub fn state_root() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("FSMP_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").context("HOME is not set; cannot locate ~/.fsmp")?;
    Ok(PathBuf::from(home).join(".fsmp"))
}

pub fn instance_dir(id: &str) -> Result<PathBuf> {
    Ok(state_root()?.join(id))
}

fn instance_path(id: &str) -> Result<PathBuf> {
    Ok(instance_dir(id)?.join("instance.json"))
}

pub fn load_definition(path: &Path) -> Result<Definition> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading definition {path:?}"))?;
    let is_json = path.extension().map(|e| e == "json").unwrap_or(false);
    let def: Definition = if is_json {
        serde_json::from_str(&text).with_context(|| format!("parsing JSON definition {path:?}"))?
    } else {
        serde_yaml::from_str(&text).with_context(|| format!("parsing YAML definition {path:?}"))?
    };
    validate(&def).with_context(|| format!("invalid definition {path:?}"))?;
    Ok(def)
}

/// Structural checks so a broken guardrail fails at `new`, not mid-run.
fn validate(def: &Definition) -> Result<()> {
    anyhow::ensure!(
        def.states.contains_key(&def.initial),
        "initial state `{}` is not defined",
        def.initial
    );
    for (sname, state) in &def.states {
        for (tname, t) in &state.transitions {
            anyhow::ensure!(
                def.states.contains_key(&t.to),
                "state `{sname}` transition `{tname}` targets unknown state `{}`",
                t.to
            );
        }
    }
    Ok(())
}

pub fn instance_exists(id: &str) -> Result<bool> {
    Ok(instance_path(id)?.exists())
}

pub fn save_instance(inst: &Instance) -> Result<()> {
    let dir = instance_dir(&inst.id)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {dir:?}"))?;
    let json = serde_json::to_string_pretty(inst)?;
    std::fs::write(instance_path(&inst.id)?, json).context("writing instance.json")?;
    Ok(())
}

pub fn load_instance(id: &str) -> Result<Instance> {
    let path = instance_path(id)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("no instance `{id}` at {path:?} (did you run `fsmp new`?)"))?;
    let inst: Instance = serde_json::from_str(&text).context("parsing instance.json")?;
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::load_definition;
    use std::path::PathBuf;

    /// Write `contents` to a uniquely-named temp file with the given extension
    /// and return its path. Distinct names per test keep parallel runs isolated.
    fn temp_def(name: &str, ext: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("fsmp-store-{name}.{ext}"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    const VALID: &str = "\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: b }
  b:
    terminal: true
";

    #[test]
    fn loads_valid_yaml_and_defaults_empty_params_and_context() {
        let p = temp_def("valid", "yaml", VALID);
        let def = load_definition(&p).expect("valid yaml should load");
        assert_eq!(def.initial, "a");
        assert!(def.params.is_empty());
        assert!(def.context.is_empty());
        assert!(def.states["b"].terminal);
    }

    #[test]
    fn loads_json_by_extension() {
        let json = r#"{"name":"t","initial":"a","states":{"a":{"transitions":{}}}}"#;
        let p = temp_def("json", "json", json);
        let def = load_definition(&p).expect("valid json should load");
        assert_eq!(def.initial, "a");
    }

    #[test]
    fn rejects_unknown_initial_state() {
        let bad = VALID.replace("initial: a", "initial: nope");
        let p = temp_def("badinitial", "yaml", &bad);
        let err = load_definition(&p).unwrap_err().to_string();
        assert!(err.contains("initial"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_transition_to_unknown_state() {
        let bad = VALID.replace("go: { to: b }", "go: { to: ghost }");
        let p = temp_def("badtarget", "yaml", &bad);
        let err = format!("{:#}", load_definition(&p).unwrap_err());
        assert!(err.contains("ghost"), "unexpected error: {err}");
    }

    #[test]
    fn surfaces_a_parse_error_for_malformed_yaml() {
        let p = temp_def("malformed", "yaml", "name: [unterminated\n");
        assert!(load_definition(&p).is_err());
    }
}
