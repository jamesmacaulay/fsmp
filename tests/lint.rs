//! Integration tests for `fsmp lint`. They run the real binary against
//! temp-file definitions (and the shipped dev-cycle definition), asserting the
//! prose/`--json` output shapes and the exit codes a caller branches on.
//!
//! `lint` reads only a definition file — it never opens an instance — so these
//! do not need an `FSMP_HOME`.

use std::path::PathBuf;
use std::process::Command;

fn run(args: &[&str]) -> Out {
    let out = Command::new(env!("CARGO_BIN_EXE_fsmp"))
        .args(args)
        .output()
        .expect("failed to run fsmp");
    Out {
        code: out.status.code().unwrap_or(-1),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

struct Out {
    code: i32,
    text: String,
}

impl Out {
    fn ok(self) -> Out {
        assert_eq!(
            self.code, 0,
            "expected success, got {}:\n{}",
            self.code, self.text
        );
        self
    }
    fn fail(self) -> Out {
        assert_ne!(
            self.code, 0,
            "expected failure, got success:\n{}",
            self.text
        );
        self
    }
    fn has(self, needle: &str) -> Out {
        assert!(
            self.text.contains(needle),
            "missing {needle:?} in:\n{}",
            self.text
        );
        self
    }
    fn lacks(self, needle: &str) -> Out {
        assert!(
            !self.text.contains(needle),
            "unexpected {needle:?} in:\n{}",
            self.text
        );
        self
    }
}

/// Write `contents` to a uniquely-named temp definition file and return its path.
fn write_def(name: &str, contents: &str) -> String {
    write_def_ext(name, "yaml", contents)
}

/// Like `write_def` but with an explicit extension (or none, when `ext` is empty).
fn write_def_ext(name: &str, ext: &str, contents: &str) -> String {
    let file = if ext.is_empty() {
        format!("fsmp-lint-it-{name}")
    } else {
        format!("fsmp-lint-it-{name}.{ext}")
    };
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, contents).unwrap();
    path.to_string_lossy().into_owned()
}

/// The shipped dev-cycle definition — dogfooding the guardrail we ship.
fn dev_cycle() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".claude/skills/dev-cycle/machine-definition.yaml")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_shipped_dev_cycle_definition_lints_clean() {
    run(&["lint", "--def", &dev_cycle()]).ok().has("clean");
}

#[test]
fn a_clean_custom_definition_exits_zero() {
    let p = write_def(
        "clean",
        "\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: b }
  b:
    terminal: true
",
    );
    run(&["lint", "--def", &p]).ok().has("clean");
}

#[test]
fn reports_unknown_target_and_unreachable_together_and_exits_nonzero() {
    // a → ghost (unknown target); b is unreachable. lint must report BOTH rather
    // than dying on the first structural error the way `new` would.
    let p = write_def(
        "combined",
        "\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: ghost }
  b:
    terminal: true
",
    );
    run(&["lint", "--def", &p])
        .fail()
        .has("ghost")
        .has("unreachable")
        .has("2 problems found");
}

#[test]
fn json_output_reports_findings_and_ok_false() {
    let p = write_def(
        "json",
        "\
name: t
initial: a
states:
  a:
    transitions:
      go: { to: ghost }
  b:
    terminal: true
",
    );
    run(&["lint", "--def", &p, "--json"])
        .fail()
        .has("\"ok\": false")
        .has("\"findings\"")
        .has("\"unknown_target\"")
        .has("\"unreachable_state\"");
}

#[test]
fn json_output_reports_ok_true_for_a_clean_definition() {
    let p = write_def(
        "jsonclean",
        "\
name: t
initial: a
states:
  a:
    terminal: true
",
    );
    run(&["lint", "--def", &p, "--json"])
        .ok()
        .has("\"ok\": true");
}

#[test]
fn a_malformed_definition_is_a_hard_error_not_a_finding() {
    let p = write_def("malformed", "name: [unterminated\n");
    run(&["lint", "--def", &p]).fail().lacks("clean");
}

/// Content is a valid definition; only the `.txt` extension is wrong. This must be
/// a HARD error (non-zero, message naming the accepted set), not a lint finding —
/// the allowlist lives in the shared loader, so `lint` never reaches its checks.
#[test]
fn an_unsupported_extension_is_a_hard_error_not_a_finding() {
    let p = write_def_ext(
        "wrongext",
        "txt",
        "\
name: t
initial: a
states:
  a:
    terminal: true
",
    );
    run(&["lint", "--def", &p])
        .fail()
        .lacks("clean")
        .has(".yaml");
}

/// The allowlist is enforced in the shared loader, so `new` rejects a wrong
/// extension identically — proving it is not bolted onto `lint` alone.
#[test]
fn new_also_hard_errors_on_an_unsupported_extension() {
    let p = write_def_ext(
        "wrongextnew",
        "txt",
        "\
name: t
initial: a
states:
  a:
    terminal: true
",
    );
    run(&["new", "--def", &p, "--id", "ext-reject"])
        .fail()
        .has(".yaml");
}
