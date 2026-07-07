//! Integration tests: drive the real `fsmp` binary against the shipped
//! dev-cycle definition. These lock in the behaviours the tool exists to
//! guarantee — you cannot skip the reviewer response/re-assessment steps, you
//! cannot `converge` before the clean-initial counter bar is met, and
//! `presenting` is reachable only through the `verifying` capstone.
//!
//! Each test runs the compiled binary via `CARGO_BIN_EXE_fsmp` with `FSMP_HOME`
//! pointed at a per-test temp dir, so nothing touches a real `~/.fsmp`.

use std::path::PathBuf;
use std::process::Command;

/// The dev-cycle definition that backs this repo's own dev-cycle skill. Testing
/// it directly means the guardrail we dogfood is guaranteed to load and drive
/// correctly.
fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".claude/skills/dev-cycle/fsmp-definition.yaml")
        .to_string_lossy()
        .into_owned()
}

struct Env {
    home: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let home = std::env::temp_dir().join(format!("fsmp-it-{name}"));
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
    fn lacks(self, needle: &str) -> Out {
        assert!(
            !self.text.contains(needle),
            "unexpected {needle:?} in:\n{}",
            self.text
        );
        self
    }
}

/// Drive: new (bar) → brief → PR open. Returns the Env positioned at
/// `awaiting_review`, round 1.
fn to_awaiting_review(name: &str, bar: &str) -> Env {
    let e = Env::new(name);
    let f = fixture();
    e.run(&[
        "new",
        "--def",
        &f,
        "--id",
        "m",
        "--set",
        &format!("bar={bar}"),
    ])
    .ok()
    .has("state: triage");
    e.run(&["do", "brief_ready", "--id", "m"])
        .ok()
        .has("state: implementing");
    e.run(&[
        "do",
        "pr_opened",
        "--id",
        "m",
        "--data",
        "pr_url=https://x/1",
    ])
    .ok()
    .has("state: awaiting_review");
    e
}

#[test]
fn happy_path_two_clean_initials_reaches_merged() {
    let e = to_awaiting_review("happy", "2");
    // Round 1: clean initial → counts immediately, no exchange.
    e.run(&["do", "verdict_clean", "--id", "m"])
        .ok()
        .has("1 of 2");
    // Cannot converge with only one clean initial.
    e.run(&["do", "converge", "--id", "m"])
        .fail()
        .has("currently 1");
    // Round 2: fresh reviewer, clean again → bar met.
    e.run(&["do", "next_round", "--id", "m"])
        .ok()
        .has("Round 2");
    e.run(&["do", "verdict_clean", "--id", "m"])
        .ok()
        .has("2 of 2");
    // Convergence lands in the verification capstone, NOT presenting.
    e.run(&["do", "converge", "--id", "m"])
        .ok()
        .has("state: verifying");
    e.run(&["do", "verification_passed", "--id", "m"])
        .ok()
        .has("state: presenting");
    e.run(&["do", "operator_merged", "--id", "m"])
        .ok()
        .has("state: merged")
        .has("terminal");
    // Terminal: no further moves.
    e.run(&["do", "operator_merged", "--id", "m"])
        .fail()
        .has("terminal");
}

#[test]
fn clean_notes_cannot_skip_the_implementer_response() {
    // The primary bug this tool prevents: a `clean, notes` verdict must route
    // through the implementer response + reviewer re-assessment, not to present.
    let e = to_awaiting_review("cleannotes", "2");
    e.run(&["do", "verdict_clean_notes", "--id", "m"])
        .ok()
        .has("state: awaiting_impl_response")
        .has("NOT optional");
    // There is no path to converge from here.
    e.run(&["do", "converge", "--id", "m"])
        .fail()
        .has("not a valid transition");
    // The reviewer keeps the last say — must re-assess before the round ends.
    e.run(&["do", "impl_responded", "--id", "m"])
        .ok()
        .has("state: awaiting_reassessment");
    e.run(&["do", "reviewer_satisfied", "--id", "m"])
        .ok()
        .has("1 of 2");
}

#[test]
fn converge_is_gated_on_the_counter_not_on_round_count() {
    // A blocking (`changes`) round must NOT count toward the clean-initial bar,
    // even after the reviewer later reaches SATISFIED.
    let e = to_awaiting_review("counter", "1"); // bar=1: one clean initial converges
    e.run(&["do", "verdict_changes", "--id", "m"])
        .ok()
        .has("state: awaiting_impl_response");
    e.run(&["do", "impl_responded", "--id", "m"]).ok();
    e.run(&["do", "reviewer_satisfied", "--id", "m"])
        .ok()
        .has("0 of 1");
    // Still blocked despite a completed round — the round was not a clean initial.
    e.run(&["do", "converge", "--id", "m"])
        .fail()
        .has("currently 0");
    // A genuinely clean round then meets the bar.
    e.run(&["do", "next_round", "--id", "m"])
        .ok()
        .has("Round 2");
    e.run(&["do", "verdict_clean", "--id", "m"])
        .ok()
        .has("1 of 1");
    e.run(&["do", "converge", "--id", "m"])
        .ok()
        .has("state: verifying");
}

/// Drive a bar=1 machine (plus any extra `--set` overrides) to `verifying`.
fn to_verifying(name: &str, extra_sets: &[&str]) -> Env {
    let e = Env::new(name);
    let f = fixture();
    let mut args = vec!["new", "--def", &f, "--id", "m", "--set", "bar=1"];
    for s in extra_sets {
        args.push("--set");
        args.push(s);
    }
    e.run(&args).ok();
    e.run(&["do", "brief_ready", "--id", "m"]).ok();
    e.run(&[
        "do",
        "pr_opened",
        "--id",
        "m",
        "--data",
        "pr_url=https://x/1",
    ])
    .ok();
    e.run(&["do", "verdict_clean", "--id", "m"]).ok();
    e.run(&["do", "converge", "--id", "m"])
        .ok()
        .has("state: verifying");
    e
}

#[test]
fn presenting_is_reachable_only_through_verifying() {
    // The issue-11 invariant: no path from round_complete to presenting except
    // through the capstone.
    let e = to_awaiting_review("onlyverify", "1");
    e.run(&["do", "verdict_clean", "--id", "m"]).ok();
    // round_complete offers no direct move into presentation.
    e.run(&["do", "verification_passed", "--id", "m"])
        .fail()
        .has("not a valid transition");
    e.run(&["do", "converge", "--id", "m"])
        .ok()
        .has("state: verifying");
    // And from verifying you cannot skip ahead to the merge step.
    e.run(&["do", "operator_merged", "--id", "m"])
        .fail()
        .has("not a valid transition");
}

#[test]
fn verification_failure_loops_through_fix_review_and_reverifies() {
    let e = to_verifying("verifyfail", &[]);
    // The findings PR-comment url is required evidence for a failure.
    e.run(&["do", "verification_failed", "--id", "m"])
        .fail()
        .has("requires data: findings_url");
    e.run(&[
        "do",
        "verification_failed",
        "--id",
        "m",
        "--data",
        "findings_url=https://x/1#issuecomment-9",
    ])
    .ok()
    .has("state: fixing")
    .has("https://x/1#issuecomment-9");
    e.run(&["do", "fix_pushed", "--id", "m"])
        .ok()
        .has("state: awaiting_fix_review")
        .has("FRESH reviewer");
    // The fix reviewer can bounce the fix back to the implementer.
    e.run(&["do", "fix_changes", "--id", "m"])
        .ok()
        .has("state: fixing");
    e.run(&["do", "fix_pushed", "--id", "m"]).ok();
    // A satisfied fix review does NOT present — it re-enters verification.
    e.run(&["do", "fix_satisfied", "--id", "m"])
        .ok()
        .has("state: verifying");
    // Prior convergence stands: the clean-initial counter was never touched.
    e.run(&["show", "--id", "m", "--json"])
        .ok()
        .has("\"clean_initial_count\": 1");
    e.run(&["do", "verification_passed", "--id", "m"])
        .ok()
        .has("state: presenting");
}

#[test]
fn waive_is_blocked_unless_capstone_was_disabled_at_new() {
    // Default capstone=true: the waive edge is visible but guard-blocked.
    let e = to_verifying("waiveblocked", &[]);
    e.run(&["do", "verification_waived", "--id", "m"])
        .fail()
        .has("capstone=true");
    e.run(&["show", "--id", "m"])
        .ok()
        .has("Blocked from here")
        .has("verification_waived");

    // capstone=false at `new` (the Phase-0 call): waive is open.
    let e = to_verifying("waiveopen", &["capstone=false"]);
    e.run(&["do", "verification_waived", "--id", "m"])
        .ok()
        .has("state: presenting");
}

#[test]
fn verification_failed_is_blocked_at_the_round_ceiling() {
    // With the ceiling already reached, another fix round may not open;
    // escalate is the remaining hatch.
    let e = to_verifying("verifyceiling", &["round_ceiling=1"]);
    e.run(&[
        "do",
        "verification_failed",
        "--id",
        "m",
        "--data",
        "findings_url=https://x/1#c",
    ])
    .fail()
    .has("round ceiling 1 reached");
    e.run(&["do", "escalate", "--id", "m"])
        .ok()
        .has("state: escalated");
}

#[test]
fn pr_opened_requires_the_pr_url_and_interpolates_it() {
    let e = Env::new("requires");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m", "--set", "bar=2"])
        .ok();
    e.run(&["do", "brief_ready", "--id", "m"]).ok();
    // Missing required data is rejected with a helpful hint.
    e.run(&["do", "pr_opened", "--id", "m"])
        .fail()
        .has("requires data: pr_url")
        .has("--data pr_url=");
    // Supplied url is echoed into the next state's guidance.
    e.run(&[
        "do",
        "pr_opened",
        "--id",
        "m",
        "--data",
        "pr_url=https://x/9",
    ])
    .ok()
    .has("https://x/9");
}

#[test]
fn unknown_transition_is_rejected_with_the_valid_list() {
    let e = Env::new("unknown");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m"]).ok();
    e.run(&["do", "teleport", "--id", "m"])
        .fail()
        .has("not a valid transition")
        .has("brief_ready"); // the valid list is re-printed
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

#[test]
fn round_complete_lists_converge_as_blocked_with_a_reason() {
    let e = to_awaiting_review("blockedlist", "2");
    e.run(&["do", "verdict_clean", "--id", "m"]).ok();
    // At 1 of 2, `show` must present converge as blocked-and-why, next_round as valid.
    e.run(&["show", "--id", "m"])
        .ok()
        .has("Blocked from here")
        .has("converge")
        .has("Valid transitions")
        .has("next_round");
}

#[test]
fn json_view_exposes_state_and_transition_partitions() {
    let e = Env::new("json");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "m", "--json"])
        .ok()
        .has("\"state\": \"triage\"")
        .has("\"valid\"")
        .has("\"blocked\"")
        .has("\"guidance\"");
}

#[test]
fn log_records_the_full_transition_history() {
    let e = to_awaiting_review("log", "2");
    e.run(&["log", "--id", "m"])
        .ok()
        .has("new")
        .has("brief_ready")
        .has("pr_opened")
        .has("pr_url=https://x/1");
}

#[test]
fn a_fresh_instance_does_not_leak_state_between_ids() {
    let e = Env::new("isolation");
    let f = fixture();
    e.run(&["new", "--def", &f, "--id", "a", "--set", "bar=2"])
        .ok();
    e.run(&["do", "brief_ready", "--id", "a"]).ok();
    // A second machine under a different id starts clean.
    e.run(&["new", "--def", &f, "--id", "b", "--set", "bar=2"])
        .ok()
        .has("state: triage");
    e.run(&["show", "--id", "b"])
        .ok()
        .has("state: triage")
        .lacks("state: implementing");
    // Re-using an existing id is refused rather than clobbering.
    e.run(&["new", "--def", &f, "--id", "a"])
        .fail()
        .has("already exists");
}
