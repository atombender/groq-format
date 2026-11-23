//! Grammar-based property tests: random GROQ queries, with random whitespace
//! and comments between tokens, must satisfy the invariants in `common`.
//!
//! Run longer with e.g. `PROPTEST_CASES=200000 cargo test --release --test fuzz`.

mod common;

use proptest::prelude::*;
use proptest::strategy::Union;

/// Whitespace between two tokens: usually nothing or a space, sometimes a
/// newline with indentation, occasionally a comment.
fn ws() -> BoxedStrategy<String> {
    prop_oneof![
        8 => Just(String::new()),
        8 => Just(" ".to_string()),
        2 => Just("\n  ".to_string()),
        1 => "[a-z ]{0,12}".prop_map(|text| format!(" // {}\n", text.trim_end())),
        1 => "[a-z]{1,8}".prop_map(|text| format!("\n// {}\n", text)),
    ]
    .boxed()
}

/// Whitespace that may not contain comments.
fn space() -> BoxedStrategy<String> {
    prop_oneof![
        Just(String::new()),
        Just(" ".to_string()),
        Just("\n".to_string())
    ]
    .boxed()
}

fn identifier() -> BoxedStrategy<String> {
    prop_oneof![
        6 => prop::sample::select(vec![
            "_type", "_id", "_ref", "title", "slug", "name", "author", "body", "image",
            "asset", "url", "current", "publishedAt", "categories", "tags", "price",
            "rating", "a", "b", "x", "releaseDate", "metadata", "dimensions",
        ]).prop_map(str::to_string),
        1 => "[a-z_][a-zA-Z0-9_]{0,24}".prop_filter("keyword", |s| {
            !matches!(s.as_str(), "true" | "false" | "null" | "in" | "match" | "asc" | "desc" | "fn")
        }),
    ]
    .boxed()
}

fn string_literal() -> BoxedStrategy<String> {
    prop_oneof![
        6 => prop::sample::select(vec![
            "\"post\"", "\"book\"", "\"author\"", "'movie'", "\"drafts.**\"", "\"\"",
            "\"Ada GROQ-lovelace\"", "\"en-US\"", "\"*foo*\"",
        ]).prop_map(str::to_string),
        2 => "[a-zA-Z0-9 .:/_-]{0,60}".prop_map(|s| format!("\"{}\"", s)),
        1 => prop::sample::select(vec![
            r#""quote \" inside""#, r#""back\\slash""#, r#""tab\there""#, r#""uni\u00e9""#,
            r#""astral\u{1F600}""#, "\"🌍 café\"", "\"日本語のテキスト\"", r#"'single \' quote'"#,
        ]).prop_map(str::to_string),
    ]
    .boxed()
}

fn number() -> BoxedStrategy<String> {
    prop_oneof![
        4 => (0i64..1000).prop_map(|n| n.to_string()),
        1 => prop::sample::select(vec![
            "1.5", "0.25", "1e3", "2.5E-3", "200000", "9223372036854775807",
            "99999999999999999999", "3.0",
        ]).prop_map(str::to_string),
    ]
    .boxed()
}

fn atom() -> BoxedStrategy<String> {
    prop_oneof![
        3 => Just("*".to_string()),
        1 => Just("@".to_string()),
        1 => Just("^".to_string()),
        1 => Just("^.^".to_string()),
        8 => identifier(),
        2 => identifier().prop_map(|p| format!("${}", p)),
        3 => string_literal(),
        2 => number(),
        1 => prop::sample::select(vec!["true", "false", "null"]).prop_map(str::to_string),
        1 => Just("[]".to_string()),
        1 => Just("{}".to_string()),
        1 => Just("now()".to_string()),
    ]
    .boxed()
}

fn subscript() -> BoxedStrategy<String> {
    let int = prop_oneof![
        4 => (0i64..300000).prop_map(|n| n.to_string()),
        1 => (1i64..10).prop_map(|n| format!("-{}", n)),
        1 => identifier().prop_map(|p| format!("${}", p)),
        1 => (identifier(), 1i64..20).prop_map(|(p, n)| format!("${} * {}", p, n)),
    ];
    let int = int.boxed();
    prop_oneof![
        (int.clone(), space(), prop::bool::ANY, space(), int.clone()).prop_map(
            |(a, s1, inclusive, s2, b)| format!(
                "[{}{}{}{}{}]",
                a,
                s1,
                if inclusive { ".." } else { "..." },
                s2,
                b
            )
        ),
        int.prop_map(|i| format!("[{}]", i)),
    ]
    .boxed()
}

fn binary_operator() -> BoxedStrategy<&'static str> {
    prop::sample::select(vec![
        "&&", "&&", "&&", "||", "||", "==", "==", "==", "!=", "<", ">", "<=", ">=", "in", "match",
        "+", "-", "*", "/", "%", "**",
    ])
    .boxed()
}

fn function_name() -> BoxedStrategy<&'static str> {
    prop::sample::select(vec![
        "count",
        "defined",
        "coalesce",
        "select",
        "references",
        "dateTime",
        "lower",
        "upper",
        "round",
        "length",
        "string::split",
        "array::join",
        "array::unique",
        "pt::text",
        "math::sum",
        "boost",
        "path",
        "identity",
        "global::now",
        "string",
        "array::compact",
        "geo::distance",
        "sanity::projectId",
    ])
    .boxed()
}

fn sep_list(items: Vec<String>, ws: Vec<String>) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
            out.push_str(&ws[i % ws.len()]);
        }
        out.push_str(item);
    }
    out
}

fn expr() -> BoxedStrategy<String> {
    atom()
        .prop_recursive(6, 96, 6, |inner| {
            let field = prop_oneof![
                4 => identifier(),
                3 => (string_literal(), ws(), ws(), inner.clone())
                    .prop_map(|(k, w1, w2, v)| format!("{}{}:{}{}", k, w1, w2, v)),
                1 => Just("...".to_string()),
                1 => inner.clone().prop_map(|e| format!("...{}", e)),
                1 => (inner.clone(), ws(), ws(), inner.clone())
                    .prop_map(|(c, w1, w2, v)| format!("{}{}=>{}{}", c, w1, w2, v)),
                1 => (identifier(), ws()).prop_map(|(i, w)| format!("{}[]->{}{{name}}", i, w)),
                1 => inner.clone(),
            ];
            let fields = (
                prop::collection::vec(field, 0..6),
                prop::collection::vec(ws(), 1..4),
            );
            let projection = (fields, ws(), ws()).prop_map(|((items, seps), w1, w2)| {
                format!("{{{}{}{}}}", w1, sep_list(items, seps), w2)
            });
            let projection = projection.boxed();

            let args = (
                prop::collection::vec(inner.clone(), 0..4),
                prop::collection::vec(ws(), 1..3),
            );
            let order_arg = (
                inner.clone(),
                prop::sample::select(vec!["", " asc", " desc"]),
            )
                .prop_map(|(e, d)| format!("{}{}", e, d));

            let stage = prop_oneof![
                prop::collection::vec(order_arg.clone(), 1..3)
                    .prop_map(|a| format!("order({})", a.join(", "))),
                inner.clone().prop_map(|a| format!("score({})", a)),
                (inner.clone(), inner.clone()).prop_map(|(a, b)| format!("boost({}, {})", a, b)),
                Just("order(_score desc)".to_string()),
                projection.clone(),
            ];
            let suffix = prop_oneof![
                3 => Just(String::new()),
                2 => subscript(),
                2 => (ws(), projection.clone()).prop_map(|(w, p)| format!("{}{}", w, p)),
                1 => (subscript(), ws(), projection.clone())
                    .prop_map(|(s, w, p)| format!("{}{}{}", s, w, p)),
                1 => identifier().prop_map(|i| format!(".{}", i)),
            ];
            let pipeline = (
                inner.clone(),
                prop::collection::vec((ws(), ws(), stage), 1..5),
                suffix,
            )
                .prop_map(|(head, stages, suffix)| {
                    let mut out = head;
                    for (w1, w2, stage) in stages {
                        out.push_str(&format!("{}|{}{}", w1, w2, stage));
                    }
                    out.push_str(&suffix);
                    out
                })
                .boxed();

            Union::new_weighted(vec![
                // Pipelines of several stages with a trailing slice or projection.
                (8, pipeline),
                // Filters, slices, element access and traversal.
                (
                    6,
                    (inner.clone(), ws(), ws(), inner.clone())
                        .prop_map(|(e, w1, w2, c)| format!("{}[{}{}{}]", e, w1, c, w2))
                        .boxed(),
                ),
                (
                    3,
                    (inner.clone(), subscript())
                        .prop_map(|(e, s)| format!("{}{}", e, s))
                        .boxed(),
                ),
                (1, inner.clone().prop_map(|e| format!("{}[]", e)).boxed()),
                // Attribute access and dereferencing.
                (
                    3,
                    (inner.clone(), space(), identifier())
                        .prop_map(|(e, w, i)| format!("{}.{}{}", e, w, i))
                        .boxed(),
                ),
                (
                    1,
                    (inner.clone(), string_literal())
                        .prop_map(|(e, s)| format!("{}[{}]", e, s))
                        .boxed(),
                ),
                (2, inner.clone().prop_map(|e| format!("{}->", e)).boxed()),
                (
                    2,
                    (inner.clone(), identifier())
                        .prop_map(|(e, i)| format!("{}->{}", e, i))
                        .boxed(),
                ),
                // Projections and pipes.
                (
                    6,
                    (inner.clone(), ws(), projection.clone())
                        .prop_map(|(e, w, p)| format!("{}{}{}", e, w, p))
                        .boxed(),
                ),
                (
                    1,
                    (inner.clone(), ws(), projection.clone())
                        .prop_map(|(e, w, p)| format!("{}{}|{}", e, w, p))
                        .boxed(),
                ),
                (
                    2,
                    (inner.clone(), ws(), prop::collection::vec(order_arg, 1..3))
                        .prop_map(|(e, w, a)| format!("{}{}| order({})", e, w, a.join(", ")))
                        .boxed(),
                ),
                (
                    1,
                    (inner.clone(), ws(), inner.clone())
                        .prop_map(|(e, w, a)| format!("{}{}| score({})", e, w, a))
                        .boxed(),
                ),
                // Operators.
                (
                    10,
                    (inner.clone(), ws(), binary_operator(), ws(), inner.clone())
                        .prop_map(|(l, w1, op, w2, r)| format!("{}{}{}{}{}", l, w1, op, w2, r))
                        .boxed(),
                ),
                (
                    2,
                    (
                        prop::sample::select(vec!["!", "-", "+"]),
                        space(),
                        inner.clone(),
                    )
                        .prop_map(|(op, w, e)| format!("{}{}{}", op, w, e))
                        .boxed(),
                ),
                (
                    3,
                    (ws(), inner.clone(), ws())
                        .prop_map(|(w1, e, w2)| format!("({}{}{})", w1, e, w2))
                        .boxed(),
                ),
                (
                    1,
                    (inner.clone(), inner.clone())
                        .prop_map(|(a, b)| format!("({}, {})", a, b))
                        .boxed(),
                ),
                (
                    1,
                    (number(), prop::sample::select(vec!["..", "..."]), number())
                        .prop_map(|(a, op, b)| format!("{}{}{}", a, op, b))
                        .boxed(),
                ),
                // Calls, arrays and objects.
                (
                    5,
                    (function_name(), args.clone())
                        .prop_map(|(f, (a, w))| format!("{}({})", f, sep_list(a, w)))
                        .boxed(),
                ),
                (
                    1,
                    (
                        prop::collection::vec((inner.clone(), ws(), inner.clone()), 1..4),
                        inner.clone(),
                    )
                        .prop_map(|(cases, fallback)| {
                            let mut parts: Vec<String> = cases
                                .into_iter()
                                .map(|(c, w, v)| format!("{} =>{}{}", c, w, v))
                                .collect();
                            parts.push(fallback);
                            format!("select({})", parts.join(", "))
                        })
                        .boxed(),
                ),
                (
                    3,
                    (
                        prop::collection::vec(inner.clone(), 0..5),
                        prop::collection::vec(ws(), 1..3),
                        ws(),
                    )
                        .prop_map(|(items, seps, w)| format!("[{}{}]", w, sep_list(items, seps)))
                        .boxed(),
                ),
                (3, projection),
            ])
            .boxed()
        })
        .boxed()
}

/// A full query: optionally some function definitions, then an expression.
fn query() -> BoxedStrategy<String> {
    let definition = (identifier(), identifier(), ws(), expr())
        .prop_map(|(ns, name, w, body)| format!("fn {}::{}($p) ={}{};", ns, name, w, body));
    (
        prop::collection::vec(definition, 0..2),
        ws(),
        expr(),
        prop_oneof![4 => Just(String::new()), 1 => Just(" // trailing".to_string())],
    )
        .prop_map(|(defs, w, e, tail)| format!("{}{}{}{}", defs.join("\n"), w, e, tail))
        .prop_filter("must parse", |q| common::parse(q).is_some())
        .boxed()
}

fn width() -> BoxedStrategy<usize> {
    prop_oneof![
        Just(0usize),
        10usize..30,
        30usize..60,
        Just(40usize),
        Just(60usize),
        Just(80usize),
        Just(120usize),
        60usize..140,
    ]
    .boxed()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2000,
        max_global_rejects: 1_000_000,
        max_local_rejects: u32::MAX,
        ..ProptestConfig::default()
    })]

    #[test]
    fn formatting_preserves_queries(q in query(), width in width(), force_wrap in prop::bool::weighted(0.2)) {
        common::check(&q, width, force_wrap);
    }
}
