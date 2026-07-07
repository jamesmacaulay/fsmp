//! On-disk layout. Definitions come from wherever the caller points (version
//! control, a skill dir); instance state lives under `~/.fsmp/state/<id>/`.

use crate::model::{Definition, Instance};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// The fsmp home directory. Holds `state/` (instance data) alongside other
/// siblings such as `bin/` (an installed binary on PATH). `FSMP_HOME` overrides
/// the default `~/.fsmp` (used by the test suite to avoid touching a real home).
pub fn home_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("FSMP_HOME") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").context("HOME is not set; cannot locate ~/.fsmp")?;
    Ok(PathBuf::from(home).join(".fsmp"))
}

/// Where per-instance state folders live: `<home>/state`.
pub fn state_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join("state"))
}

/// An instance id becomes a single directory name under `state/`, so it must
/// not be able to name anything else: no separators (`../../x` or an absolute
/// path would escape the fsmp home entirely), non-empty (`""` would collide
/// with `state/` itself), and no leading `.` (covers `.`/`..` and hidden dirs).
fn validate_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    anyhow::ensure!(
        ok,
        "invalid instance id `{id}`: use ASCII letters, digits, `-`, `_`, or `.` (no leading `.`)"
    );
    Ok(())
}

pub fn instance_dir(id: &str) -> Result<PathBuf> {
    validate_id(id)?;
    Ok(state_dir()?.join(id))
}

fn instance_path(id: &str) -> Result<PathBuf> {
    Ok(instance_dir(id)?.join("instance.json"))
}

/// Read and deserialize a definition file, selecting the parser from an extension
/// allowlist, without validating structure. `lint` uses this so it can collect
/// *all* findings from a parseable definition instead of hard-failing at the first
/// structural error the way `validate` does.
///
/// Only `.yaml`/`.yml` (YAML) and `.json` (JSON) are accepted; any other
/// extension — or none at all — is a hard error naming the accepted set, so the
/// parser is never guessed from content. The extension is matched
/// case-insensitively (macOS filesystems are commonly case-insensitive).
pub fn parse_definition(path: &Path) -> Result<Definition> {
    enum Format {
        Yaml,
        Json,
    }
    // Resolve the parser from the extension BEFORE touching the file, so an
    // unsupported extension is reported as the static contract violation it is
    // (winning over an incidental read error like a missing file) and we skip the
    // read syscall on a known-bad path. Lowercase first — macOS filesystems are
    // commonly case-insensitive; a missing or non-UTF8 extension yields None and
    // is rejected.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    let format = match ext.as_deref() {
        Some("json") => Format::Json,
        Some("yaml") | Some("yml") => Format::Yaml,
        _ => anyhow::bail!(
            "unsupported definition extension for {path:?} (expected .yaml, .yml, or .json)"
        ),
    };
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading definition {path:?}"))?;
    let def: Definition = match format {
        Format::Json => serde_json::from_str(&text)
            .with_context(|| format!("parsing JSON definition {path:?}"))?,
        Format::Yaml => serde_yaml::from_str(&text)
            .with_context(|| format!("parsing YAML definition {path:?}"))?,
    };
    Ok(def)
}

pub fn load_definition(path: &Path) -> Result<Definition> {
    let def = parse_definition(path)?;
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
    // Write-then-rename so a crash mid-write can't leave a truncated
    // instance.json behind (rename is atomic on the same filesystem).
    let tmp = dir.join("instance.json.tmp");
    std::fs::write(&tmp, json).with_context(|| format!("writing {tmp:?}"))?;
    std::fs::rename(&tmp, instance_path(&inst.id)?).context("committing instance.json")?;
    Ok(())
}

pub fn load_instance(id: &str) -> Result<Instance> {
    let path = instance_path(id)?;
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("no instance `{id}` at {path:?} (did you run `fsmp new`?)"))?;
    let inst: Instance = serde_json::from_str(&text).context("parsing instance.json")?;
    // A hand-edited/corrupt snapshot must fail here with a clear error, not
    // panic later in rendering, which assumes this invariant.
    anyhow::ensure!(
        inst.definition.states.contains_key(&inst.current),
        "instance `{id}` is corrupt: current state `{}` is not in its definition snapshot",
        inst.current
    );
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

    #[test]
    fn accepts_yml_extension() {
        let p = temp_def("yml", "yml", VALID);
        let def = load_definition(&p).expect("valid .yml should load");
        assert_eq!(def.initial, "a");
    }

    #[test]
    fn accepts_uppercase_extensions_case_insensitively() {
        let py = temp_def("upperyaml", "YAML", VALID);
        assert_eq!(
            load_definition(&py).expect("`.YAML` should load").initial,
            "a"
        );

        let json = r#"{"name":"t","initial":"a","states":{"a":{"transitions":{}}}}"#;
        let pj = temp_def("upperjson", "JSON", json);
        assert_eq!(
            load_definition(&pj).expect("`.JSON` should load").initial,
            "a"
        );
    }

    #[test]
    fn rejects_unsupported_extension_naming_the_accepted_set() {
        let p = temp_def("txt", "txt", VALID);
        let err = load_definition(&p).unwrap_err().to_string();
        assert!(err.contains(".yaml"), "should name .yaml: {err}");
        assert!(err.contains(".yml"), "should name .yml: {err}");
        assert!(err.contains(".json"), "should name .json: {err}");
    }

    #[test]
    fn unsupported_extension_wins_over_a_missing_file() {
        // The extension is the static contract, checked before the file is read,
        // so a nonexistent `.txt` reports the extension error rather than a read
        // error — and no read syscall is issued on a known-bad extension.
        let missing = std::env::temp_dir().join("fsmp-store-does-not-exist.txt");
        let _ = std::fs::remove_file(&missing); // ensure absent
        let err = load_definition(&missing).unwrap_err().to_string();
        assert!(
            err.contains("unsupported definition extension"),
            "extension error should win over the read error: {err}"
        );
        assert!(
            !err.contains("reading definition"),
            "should not have read: {err}"
        );
    }

    #[test]
    fn rejects_a_file_with_no_extension() {
        // `unwrap_or(false)` previously routed extensionless files to YAML; the
        // allowlist now rejects them.
        let path = std::env::temp_dir().join("fsmp-store-noext");
        std::fs::write(&path, VALID).unwrap();
        let err = load_definition(&path).unwrap_err().to_string();
        assert!(
            err.contains(".yaml"),
            "should name accepted extensions: {err}"
        );
    }

    /// A definition whose single guard carries the given rhs key(s).
    fn def_with_guard_rhs(rhs: &str) -> String {
        format!(
            "\
name: t
initial: a
states:
  a:
    transitions:
      go: {{ to: b, guards: [ {{ var: x, op: eq, {rhs} }} ] }}
  b:
    terminal: true
"
        )
    }

    #[test]
    fn guard_requires_exactly_one_rhs_at_parse_time() {
        // More than one rhs key: previously value silently won; now rejected.
        let p = temp_def(
            "tworhs",
            "yaml",
            &def_with_guard_rhs("value: 1, param: bar"),
        );
        let err = format!("{:#}", load_definition(&p).unwrap_err());
        assert!(
            err.contains("exactly one") && err.contains("more than one"),
            "unexpected error: {err}"
        );

        // No rhs key at all: previously evaluated against an absent rhs.
        let p = temp_def(
            "norhs",
            "yaml",
            &def_with_guard_rhs("value: 1").replace(", value: 1", ""),
        );
        let err = format!("{:#}", load_definition(&p).unwrap_err());
        assert!(
            err.contains("exactly one") && err.contains("found none"),
            "unexpected error: {err}"
        );

        // Each single-key form still parses.
        for (name, rhs) in [("v", "value: 1"), ("p", "param: bar"), ("c", "ctx: other")] {
            let p = temp_def(&format!("onerhs{name}"), "yaml", &def_with_guard_rhs(rhs));
            load_definition(&p).unwrap_or_else(|e| panic!("`{rhs}` should load: {e:#}"));
        }
    }

    #[test]
    fn guard_serializes_back_to_its_single_wire_key() {
        // Round-trip: the enum serializes as just its own key — no null noise
        // for the two absent alternatives (as the old triple-Option shape did).
        let p = temp_def("roundtrip", "yaml", &def_with_guard_rhs("param: bar"));
        let def = load_definition(&p).unwrap();
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("\"param\":\"bar\""), "missing param: {json}");
        assert!(
            !json.contains("\"value\":null") && !json.contains("\"ctx\":null"),
            "null noise in serialized guard: {json}"
        );
        // And the old on-disk shape (explicit nulls) still deserializes.
        let old = r#"{"var":"x","op":"eq","value":1,"param":null,"ctx":null}"#;
        let g: crate::model::Guard = serde_json::from_str(old).unwrap();
        assert_eq!(g.rhs, crate::model::Rhs::Value(crate::model::Value::Int(1)));
    }
}
