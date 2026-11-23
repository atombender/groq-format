//! Snapshots of hand-picked queries at widths 40, 60, 80 and 120, for
//! reviewing layout changes. Update with `INSTA_UPDATE=always cargo test`
//! or `cargo insta review`.

mod common;

use std::fs;
use std::path::Path;

#[test]
fn gallery_snapshots() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/gallery");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "groq"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 30);

    for path in paths {
        let query = fs::read_to_string(&path).unwrap();
        let mut snapshot = String::new();
        for width in [40, 60, 80, 120] {
            let output = common::check(&query, width, false);
            snapshot.push_str(&format!("--- width {width}\n{output}\n"));
        }
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        insta::assert_snapshot!(name, snapshot);
    }
}

#[test]
fn demo_join_in_filter() {
    let query = include_str!("gallery/01-demo-object.groq");
    let expected = r#"
  "joinInFilter": *[
    _type == "book" && author->name == "Ada GROQ-lovelace"
  ][0...5] { title },
"#;
    let output = common::check(query, 80, false);
    assert!(output.contains(expected), "{output}");
}

#[test]
fn pipelines_stay_inline_when_they_fit() {
    let query = r#"*[_type == $type] | score(text::semanticSimilarity($q)) | order(_score desc)[0...10] { title }"#;
    assert_eq!(common::check(query, 120, false), query);
}

#[test]
fn pipelines_break_every_pipe_or_none() {
    let query = r#"*[_type == $type] | score(text::semanticSimilarity($q)) | order(_score desc)[0...10] { title }"#;
    // The first stage would fit at 60 and 80, but the chain does not, so
    // every `|` moves to its own line.
    let expected = r#"*[_type == $type]
  | score(text::semanticSimilarity($q))
  | order(_score desc)[0...10] { title }"#;
    for width in [40, 60, 80] {
        assert_eq!(common::check(query, width, false), expected);
    }
}

#[test]
fn pipelines_break_all_stages_when_one_stage_breaks() {
    let query = r#"*[_type == "article" && !(_id in path("drafts.**"))] | score(boost([title] match text::query("summer fashion"), 2), boost(category == "editorial", 1.5), boost(publishedAt > "2024-01-01", 1.2)) | order(_score desc)[0...20] { title, slug, _score, "highlights": text::highlight() }"#;
    let expected = r#"*[_type == "article" && !(_id in path("drafts.**"))]
  | score(
    boost([title] match text::query("summer fashion"), 2),
    boost(category == "editorial", 1.5),
    boost(publishedAt > "2024-01-01", 1.2)
  )
  | order(_score desc)[0...20] {
    title,
    slug,
    _score,
    "highlights": text::highlight()
  }"#;
    assert_eq!(common::check(query, 80, false), expected);
}

#[test]
fn a_pipe_after_a_multi_line_head_stays_when_it_fits() {
    let query = r#"*[_type == "post"] { title, "slug": slug.current, "author": author->name, "image": mainImage.asset->url }[0...10] | order(_createdAt desc)"#;
    let expected = r#"*[_type == "post"] {
  title,
  "slug": slug.current,
  "author": author->name,
  "image": mainImage.asset->url
}[0...10] | order(_createdAt desc)"#;
    assert_eq!(common::check(query, 60, false), expected);
}

#[test]
fn pipeline_projection_hugs_the_last_stage() {
    let query = r#"*[_type == $type] | score(text::semanticSimilarity($q)) | order(_score desc)[0...10] { title, body, "slug": slug.current, "author": author->name }"#;
    let expected = r#"*[_type == $type]
  | score(text::semanticSimilarity($q))
  | order(_score desc)[0...10] {
    title,
    body,
    "slug": slug.current,
    "author": author->name
  }"#;
    assert_eq!(common::check(query, 60, false), expected);
}

#[test]
fn pipelines_in_values_and_arguments() {
    let query = r#"{"search": *[_type in ["post", "page"] && language == $lang] | score(text::semanticSimilarity($q)) | order(_score desc)[0...10] { title }, "n": count(*[_type == $type] | score(text::semanticSimilarity($q)) | order(_score desc))}"#;
    let expected = r#"{
  "search":
    *[_type in ["post", "page"] && language == $lang]
    | score(text::semanticSimilarity($q))
    | order(_score desc)[0...10] { title },
  "n": count(
    *[_type == $type]
      | score(text::semanticSimilarity($q))
      | order(_score desc)
  )
}"#;
    assert_eq!(common::check(query, 60, false), expected);
}

const DEX_FILTER: &str = r#"*[_type == $type && nationalDexNumber >= 1 && nationalDexNumber <= 151 && count(types[lower(@->name) == "water"]) > 0]"#;

#[test]
fn single_pipe_stays_on_the_closing_line_of_a_multi_line_head() {
    let query = format!(
        r#"{DEX_FILTER} | order(baseStats.attack desc, nationalDexNumber asc) {{ name, nationalDexNumber, "types": types[]->name, "attack": baseStats.attack }}"#
    );
    let expected = r#"*[
  _type == $type
  && nationalDexNumber >= 1
  && nationalDexNumber <= 151
  && count(types[lower(@->name) == "water"]) > 0
] | order(baseStats.attack desc, nationalDexNumber asc) {
  name,
  nationalDexNumber,
  "types": types[]->name,
  "attack": baseStats.attack
}"#;
    // `] | order(...) {` is 58 columns.
    for width in [60, 80] {
        assert_eq!(
            common::check(&query, width, false),
            expected,
            "width {width}"
        );
    }
    // The hand-formatted layout is already stable.
    assert_eq!(common::check(expected, 80, false), expected);
}

#[test]
fn single_pipe_that_does_not_fit_breaks_cleanly() {
    let query = format!(
        r#"{DEX_FILTER} | order(baseStats.attack desc, nationalDexNumber asc) {{ name, nationalDexNumber, "types": types[]->name, "attack": baseStats.attack }}"#
    );
    let expected = r#"*[
  _type == $type
  && nationalDexNumber >= 1
  && nationalDexNumber <= 151
  && count(types[
    lower(@->name) == "water"
  ]) > 0
]
  | order(
    baseStats.attack desc,
    nationalDexNumber asc
  ) {
    name,
    nationalDexNumber,
    "types": types[]->name,
    "attack": baseStats.attack
  }"#;
    assert_eq!(common::check(&query, 40, false), expected);
}

#[test]
fn pipe_chain_stays_on_the_closing_line_when_it_fits() {
    let query = format!(
        r#"{DEX_FILTER} | score(boost(name match $q, 2)) | order(_score desc) {{ name, nationalDexNumber }}"#
    );
    let expected = r#"*[
  _type == $type
  && nationalDexNumber >= 1
  && nationalDexNumber <= 151
  && count(types[lower(@->name) == "water"]) > 0
] | score(boost(name match $q, 2)) | order(_score desc) {
  name,
  nationalDexNumber
}"#;
    for width in [60, 80] {
        assert_eq!(
            common::check(&query, width, false),
            expected,
            "width {width}"
        );
    }
}

#[test]
fn pipe_chain_that_does_not_fit_breaks_every_pipe() {
    let query = format!(
        r#"{DEX_FILTER} | score(boost(name match $q, 2)) | order(_score desc) {{ name, nationalDexNumber }}"#
    );
    let expected = r#"*[
  _type == $type
  && nationalDexNumber >= 1
  && nationalDexNumber <= 151
  && count(types[
    lower(@->name) == "water"
  ]) > 0
]
  | score(boost(name match $q, 2))
  | order(_score desc) {
    name,
    nationalDexNumber
  }"#;
    assert_eq!(common::check(&query, 40, false), expected);

    let long = format!(
        r#"{DEX_FILTER} | score(boost(name match $q, 2), boost(types[]->name match $q, 1), boost(baseStats.attack > 100, 3)) | order(_score desc, baseStats.attack desc) {{ name, nationalDexNumber }}"#
    );
    let output = common::check(&long, 80, false);
    assert!(output.contains("]\n  | score(\n"), "{output}");
    assert!(
        output.contains("\n  )\n  | order(_score desc, baseStats.attack desc) {"),
        "{output}"
    );
}
