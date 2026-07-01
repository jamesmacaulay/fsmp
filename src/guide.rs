//! Embedded authoring/driving docs, served by `fsmp guide [topic]`.
//!
//! The markdown under `docs/` is the single source of truth; it is compiled into
//! the binary with `include_str!` (no runtime file reads, no `man` dependency —
//! fsmp installs to a non-standard prefix, so real man pages are a separate
//! follow-up). The `include_str!` paths are resolved relative to THIS file, so a
//! missing doc is a compile error — a free guarantee the docs exist.

/// The definition/authoring reference (`docs/definition.md`).
const DEFINITION: &str = include_str!("../docs/definition.md");
/// The driving primer (`docs/driving.md`).
const DRIVING: &str = include_str!("../docs/driving.md");

/// The known topics, in the order they're listed. Each is `(name, one-liner,
/// text)`.
const TOPICS: &[(&str, &str, &str)] = &[
    (
        "definition",
        "the YAML definition format, authoring patterns, and anti-patterns",
        DEFINITION,
    ),
    (
        "driving",
        "a short primer on driving any machine one transition at a time",
        DRIVING,
    ),
];

/// The doc text for a topic, or `None` if the topic is unknown.
pub fn topic_text(topic: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(name, _, _)| *name == topic)
        .map(|(_, _, text)| *text)
}

/// The comma-separated list of valid topic names, for error messages.
pub fn topic_names() -> String {
    TOPICS
        .iter()
        .map(|(name, _, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The no-argument listing: each topic name and its one-liner, plus a usage hint.
pub fn list_topics() -> String {
    let mut out = String::from("fsmp guide — authoring and driving reference\n\nTopics:\n");
    for (name, blurb, _) in TOPICS {
        out.push_str(&format!("  {name:<12} {blurb}\n"));
    }
    out.push_str("\nRun `fsmp guide <topic>` to print one.\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_topics_return_their_docs() {
        assert!(topic_text("definition")
            .unwrap()
            .contains("Authoring fsmp definitions"));
        assert!(topic_text("driving")
            .unwrap()
            .contains("Driving an fsmp machine"));
    }

    #[test]
    fn unknown_topic_returns_none() {
        assert!(topic_text("nonsense").is_none());
    }

    #[test]
    fn topic_names_lists_every_topic() {
        let names = topic_names();
        assert!(names.contains("definition"));
        assert!(names.contains("driving"));
    }

    #[test]
    fn listing_names_every_topic_with_a_blurb() {
        let listing = list_topics();
        assert!(listing.contains("definition"));
        assert!(listing.contains("driving"));
        // A one-liner, not just the bare name.
        assert!(listing.contains("anti-patterns"));
        assert!(listing.contains("primer"));
    }
}
