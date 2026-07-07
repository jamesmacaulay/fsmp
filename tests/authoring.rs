//! Integration tests: drive the real `fsmp` binary against the shipped
//! author-fsmp-workflow definition. Per CLAUDE.md's rule for a new machine, these
//! lock in the gates the pipeline exists to enforce — you cannot skip lint, the
//! dry-run, or the user sign-off; a failed stage loops back to `drafting`;
//! `done` is reachable only through `accepted`.
//!
//! Each test runs the compiled binary via `CARGO_BIN_EXE_fsmp` with `FSMP_HOME`
//! pointed at a per-test temp dir, so nothing touches a real `~/.fsmp`.

use std::path::PathBuf;
use std::process::Command;

/// The author-fsmp-workflow definition that backs this repo's authoring skill.
/// Driving it directly means the exemplar we ship is guaranteed to load and
/// sequence correctly.
fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".claude/skills/author-fsmp-workflow/fsmp-definition.yaml")
        .to_string_lossy()
        .into_owned()
}

struct Env {
    home: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let home = std::env::temp_dir().join(format!("fsmp-author-it-{name}"));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        Env { home }
    }

    fn run(&self, args: &[&str]) -> Out {
        let out = Command::new(env!("CARGO_BIN_EXE_fsmp"))
            .env("FSMP_HOME", &self.home)
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
}

/// Drive: new → graph sign-off (capturing def_path). Returns the Env positioned
/// at `drafting`.
fn to_drafting(name: &str) -> Env {
    let e = Env::new(name);
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m"])
        .ok()
        .has("state: graph_review");
    e.run(&[
        "do",
        "graph_approved",
        "--id",
        "m",
        "--data",
        "def_path=/tmp/demo.yaml",
    ])
    .ok()
    .has("state: drafting")
    // def_path is interpolated into the drafting guidance.
    .has("/tmp/demo.yaml");
    e
}

#[test]
fn happy_path_reaches_done_through_every_gate() {
    let e = to_drafting("happy");
    e.run(&["do", "draft_written", "--id", "m"])
        .ok()
        .has("state: linting");
    e.run(&["do", "lint_clean", "--id", "m"])
        .ok()
        .has("state: dry_run");
    e.run(&["do", "dryrun_passed", "--id", "m"])
        .ok()
        .has("state: user_signoff");
    e.run(&["do", "accepted", "--id", "m"])
        .ok()
        .has("state: done")
        .has("terminal");
    // Terminal: no further moves.
    e.run(&["do", "accepted", "--id", "m"])
        .fail()
        .has("terminal");
}

#[test]
fn no_yaml_before_the_graph_is_signed_off() {
    // From graph_review the only forward move is graph_approved; you cannot jump
    // straight into drafting/linting.
    let e = Env::new("gate_graph");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m"]).ok();
    e.run(&["do", "draft_written", "--id", "m"])
        .fail()
        .has("not a valid transition")
        .has("graph_approved"); // the valid list is re-printed
                                // And graph_approved requires the path it will write to.
    e.run(&["do", "graph_approved", "--id", "m"])
        .fail()
        .has("requires data: def_path")
        .has("--data def_path=");
}

#[test]
fn no_dry_run_before_lint_is_clean() {
    // From drafting you must go through linting; you cannot skip to dry_run.
    let e = to_drafting("gate_lint");
    e.run(&["do", "dryrun_passed", "--id", "m"])
        .fail()
        .has("not a valid transition");
    e.run(&["do", "draft_written", "--id", "m"])
        .ok()
        .has("state: linting");
    // From linting you cannot skip straight to user_signoff.
    e.run(&["do", "dryrun_passed", "--id", "m"])
        .fail()
        .has("not a valid transition");
}

#[test]
fn no_sign_off_before_a_dry_run() {
    // Walk to dry_run, then confirm `accepted` isn't reachable until dryrun_passed.
    let e = to_drafting("gate_signoff");
    e.run(&["do", "draft_written", "--id", "m"]).ok();
    e.run(&["do", "lint_clean", "--id", "m"])
        .ok()
        .has("state: dry_run");
    e.run(&["do", "accepted", "--id", "m"])
        .fail()
        .has("not a valid transition");
    e.run(&["do", "dryrun_passed", "--id", "m"])
        .ok()
        .has("state: user_signoff");
}

#[test]
fn a_failed_gate_loops_back_to_drafting() {
    // Each quality gate can send the author back to drafting but never forward
    // past a failure.
    let e = to_drafting("retry");
    e.run(&["do", "draft_written", "--id", "m"]).ok();
    e.run(&["do", "lint_failed", "--id", "m"])
        .ok()
        .has("state: drafting");
    e.run(&["do", "draft_written", "--id", "m"]).ok();
    e.run(&["do", "lint_clean", "--id", "m"]).ok();
    e.run(&["do", "dryrun_failed", "--id", "m"])
        .ok()
        .has("state: drafting");
    e.run(&["do", "draft_written", "--id", "m"]).ok();
    e.run(&["do", "lint_clean", "--id", "m"]).ok();
    e.run(&["do", "dryrun_passed", "--id", "m"]).ok();
    // The user can still send it back from sign-off.
    e.run(&["do", "changes_requested", "--id", "m"])
        .ok()
        .has("state: drafting");
}

#[test]
fn escalate_reaches_a_terminal_state() {
    let e = Env::new("escalate");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m"]).ok();
    e.run(&["do", "escalate", "--id", "m"])
        .ok()
        .has("state: escalated")
        .has("terminal");
    e.run(&["do", "escalate", "--id", "m"])
        .fail()
        .has("terminal");
}
