//! Real-world queries from the GROQ test suite, groq-js, Sanity Studio and
//! Sanity starter templates, checked against the invariants in `common` at
//! several widths.

mod common;

/// Entries start with a `# source` line, followed by the query on one or
/// more lines. `real_world.txt` holds queries from other projects and
/// `docs.txt` the examples in the Sanity documentation.
fn corpus() -> Vec<(String, String)> {
    let text = [
        include_str!("corpus/real_world.txt"),
        include_str!("corpus/docs.txt"),
    ]
    .join("\n");
    let mut entries: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(source) = line.strip_prefix("# ") {
            entries.push((source.to_string(), String::new()));
        } else if let Some((_, query)) = entries.last_mut() {
            if !query.is_empty() {
                query.push('\n');
            }
            query.push_str(line);
        }
    }
    entries
}

#[test]
fn corpus_is_formatted_correctly() {
    let entries = corpus();
    assert!(entries.len() > 1000);
    for (source, query) in &entries {
        assert!(
            common::parse(query).is_some(),
            "{source}: does not parse:\n{query}"
        );
        for width in [0, 20, 40, 60, 80, 100, 120] {
            for force_wrap in [false, true] {
                let result = std::panic::catch_unwind(|| common::check(query, width, force_wrap));
                assert!(result.is_ok(), "{source}: failed at width {width}");
            }
        }
    }
}
