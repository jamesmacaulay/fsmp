//! Integration tests for `fsmp guide`. They run the real binary and assert the
//! embedded docs are served, topics are listed with no argument, and an unknown
//! topic is a non-zero error naming the valid set. `guide` reads no files at
//! runtime (the docs are `include_str!`'d), so these need no `FSMP_HOME`.

use std::process::Command;

fn run(args: &[&str]) -> Out {
    let out = Command::new(env!("CARGO_BIN_EXE_fsmp"))
        .args(args)
        .output()
        .expect("failed to run fsmp");
    Out {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

#[test]
fn definition_topic_prints_the_authoring_reference() {
    let o = run(&["guide", "definition"]);
    assert_eq!(o.code, 0, "expected success:\n{}{}", o.stdout, o.stderr);
    assert!(
        o.stdout.contains("# Authoring fsmp definitions"),
        "missing definition heading in:\n{}",
        o.stdout
    );
    // A stable section that must survive edits to the doc.
    assert!(o.stdout.contains("Anti-patterns"));
}

#[test]
fn driving_topic_prints_the_driving_primer() {
    let o = run(&["guide", "driving"]);
    assert_eq!(o.code, 0, "expected success:\n{}{}", o.stdout, o.stderr);
    assert!(
        o.stdout.contains("# Driving an fsmp machine"),
        "missing driving heading in:\n{}",
        o.stdout
    );
}

#[test]
fn no_topic_lists_the_available_topics() {
    let o = run(&["guide"]);
    assert_eq!(o.code, 0, "expected success:\n{}{}", o.stdout, o.stderr);
    assert!(
        o.stdout.contains("Topics:"),
        "missing listing:\n{}",
        o.stdout
    );
    assert!(o.stdout.contains("definition"));
    assert!(o.stdout.contains("driving"));
}

#[test]
fn unknown_topic_errors_to_stderr_naming_the_valid_set() {
    let o = run(&["guide", "bogus"]);
    assert_ne!(o.code, 0, "expected failure for an unknown topic");
    // The error names the valid topics so the caller can recover.
    assert!(
        o.stderr.contains("definition") && o.stderr.contains("driving"),
        "stderr should name the valid topics:\n{}",
        o.stderr
    );
}
