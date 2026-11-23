//! Invariants every formatted query must satisfy.

#![allow(dead_code)]

use groq_format::{FormatOptions, format_query_with_options};
use groq_parser::ast::ParseResult;
use groq_parser::parser::{Parser, ParserConfig};
use unicode_width::UnicodeWidthStr;

pub fn parse(query: &str) -> Option<ParseResult> {
    let config = ParserConfig::without_param_validation().with_comments();
    Parser::new_with_config(query.trim(), config).parse().ok()
}

/// The parse tree with source positions removed, for comparing queries.
pub fn shape(result: &ParseResult) -> String {
    let debug = format!("{:?}|{:?}", result.expr, result.functions);
    strip_positions(&debug)
}

fn strip_positions(debug: &str) -> String {
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(i) = rest.find("pos: Position {") {
        out.push_str(&rest[..i]);
        let close = rest[i..].find('}').unwrap();
        rest = &rest[i + close + 1..];
    }
    out.push_str(rest);
    out
}

/// A line without indentation, and without the closing brackets it starts
/// with or the opening brackets it ends with, which differ when a call hugs
/// its last argument (`count(*[` versus `count(` and `*[`).
fn line_core(line: &str) -> &str {
    line.trim_start()
        .trim_start_matches([')', ']', '}'])
        .trim_end_matches(['(', '[', '{', ')', ']', '}'])
        .trim()
}

pub fn comment_texts(result: &ParseResult) -> Vec<String> {
    result
        .comments
        .iter()
        .map(|c| c.text.trim_end().to_string())
        .collect()
}

pub fn format(query: &str, width: usize, force_wrap: bool) -> String {
    let options = FormatOptions::new(width).with_force_wrap(force_wrap);
    format_query_with_options(query, &options)
        .unwrap_or_else(|e| panic!("failed to format {query:?}: {e}"))
}

/// Checks all invariants for `query` at `width`, returning the formatted text.
///
/// 1. The output parses to the same tree (and comments) as the input.
/// 2. Formatting the output again changes nothing.
/// 3. A line longer than `width` cannot be broken any further: it also
///    appears (or starts a line) in the output at width 0, where every
///    group is broken.
///    (Skipped when comments are present, since comments are never wrapped.)
/// 4. No trailing whitespace, tabs or blank lines (other than the one after
///    function definitions), and indentation moves in steps of two spaces.
/// 5. Pipes are all or nothing: every `|` of a chain is inline or every `|`
///    starts a line at the same indent, and a lone `|` breaks only when its
///    stage does not fit on the previous line. (The fit part is skipped when
///    comments are present, since a comment can force the break.)
/// 6. A line starting with a closing bracket is indented like the line that
///    opened it, and binary operators never end a line or stand alone.
pub fn check(query: &str, width: usize, force_wrap: bool) -> String {
    let input = parse(query).unwrap_or_else(|| panic!("input does not parse: {query:?}"));
    let output = format(query, width, force_wrap);
    let ctx = || {
        format!(
            "\n--- input (width {width}, force_wrap {force_wrap}):\n{query}\n--- output:\n{output}\n---"
        )
    };

    let reparsed = parse(&output).unwrap_or_else(|| panic!("output does not parse{}", ctx()));
    assert_eq!(
        shape(&input),
        shape(&reparsed),
        "output changes the query{}",
        ctx()
    );
    assert_eq!(
        comment_texts(&input),
        comment_texts(&reparsed),
        "output changes comments{}",
        ctx()
    );

    let again = format(&output, width, force_wrap);
    assert_eq!(output, again, "not idempotent{}", ctx());

    if input.comments.is_empty() {
        let broken = format(query, 0, force_wrap);
        let unbreakable: Vec<&str> = broken.lines().map(line_core).collect();
        for line in output.lines() {
            if line.width() > width {
                let is_unbreakable = |text: &str| {
                    let core = line_core(text);
                    unbreakable.iter().any(|u| u.starts_with(core))
                };
                // A call that hugs its last argument ends it with `)` on the
                // line of the argument's own closing bracket, where the fully
                // broken layout has two lines; both pieces must exist there.
                let hugged_call = line.match_indices(')').any(|(i, _)| {
                    i > 0
                        && is_unbreakable(&line[..i])
                        && is_unbreakable(&format!(
                            "{}{}",
                            &line[..line.len() - line.trim_start().len()],
                            &line[i..]
                        ))
                });
                assert!(
                    is_unbreakable(line) || hugged_call,
                    "line could have been broken: {line:?}{}\n--- fully broken:\n{broken}",
                    ctx()
                );
            }
        }
    }

    let mut previous_indent = 0;
    let mut blank_lines = 0;
    for line in output.lines() {
        assert!(
            !line.ends_with(' '),
            "trailing whitespace in {line:?}{}",
            ctx()
        );
        assert!(!line.contains('\t'), "tab in {line:?}{}", ctx());
        if line.is_empty() {
            blank_lines += 1;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        assert!(indent % 2 == 0, "odd indentation in {line:?}{}", ctx());
        assert!(
            indent <= previous_indent + 2,
            "indentation jumps by more than one level at {line:?}{}",
            ctx()
        );
        previous_indent = indent;
    }
    check_structure(&output, &ctx);
    check_pipe_chains(&output, width, input.comments.is_empty(), &ctx);

    let allowed_blank_lines = usize::from(!input.functions.is_empty());
    assert!(
        blank_lines <= allowed_blank_lines,
        "unexpected blank line{}",
        ctx()
    );
    assert!(
        !output.starts_with(char::is_whitespace),
        "leading whitespace{}",
        ctx()
    );

    output
}

const BINARY_OPERATORS: &[&str] = &[
    "&&", "||", "==", "!=", "<=", ">=", "<", ">", "+", "-", "/", "%", "**", "|",
];

/// The code of each line with strings and comments blanked out, so brackets
/// and operators inside them are ignored.
fn code_lines(output: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = output.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        if c == '\n' {
            lines.push(std::mem::take(&mut current));
            continue;
        }
        match quote {
            Some(q) => {
                if c == '\\' {
                    chars.next();
                } else if c == q {
                    quote = None;
                }
                current.push('_');
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push('_');
            }
            None if c == '/' && chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|&c| c != '\n') {
                    chars.next();
                }
            }
            None => current.push(c),
        }
    }
    lines.push(current);
    lines
}

/// Byte offsets of the `|` operators on `code` that are not inside brackets
/// opened on the same line, and not part of `||`.
fn top_level_pipes(code: &str) -> Vec<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut found = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 => {
                let double = bytes.get(i + 1) == Some(&b'|') || (i > 0 && bytes[i - 1] == b'|');
                if !double {
                    found.push(i);
                }
            }
            _ => {}
        }
    }
    found
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Whether another line starting with `| ` has the indent of line `i` within
/// the same block, that is, `i` is one stage of a chain with several.
fn has_sibling_stage(lines: &[&str], i: usize) -> bool {
    let indent = indent_of(lines[i]);
    let siblings = |range: &mut dyn Iterator<Item = usize>| {
        for j in range {
            let line = lines[j];
            if line.trim().is_empty() {
                continue;
            }
            let other = indent_of(line);
            if other < indent {
                return false;
            }
            if other == indent && line.trim_start().starts_with("| ") {
                return true;
            }
        }
        false
    };
    siblings(&mut (0..i).rev()) || siblings(&mut (i + 1..lines.len()))
}

/// Pipelines are all or nothing, like a `&&` chain: either every `|` stays
/// on the line of the head, or every `|` starts a line of its own at the same
/// indent. A lone `|` only moves to its own line when the stage does not fit
/// on the previous line. (The stage is measured as printed up to its first
/// bracket, because a trailing projection can break on its own.)
fn check_pipe_chains(output: &str, width: usize, check_fit: bool, ctx: &dyn Fn() -> String) {
    let lines: Vec<&str> = output.lines().collect();
    let code = code_lines(output);
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let indent = indent_of(line);
        let code_trimmed = code[i].trim_start();
        let pipes = top_level_pipes(code_trimmed);
        let leading = trimmed.starts_with("| ");
        if leading {
            assert_eq!(
                pipes.len(),
                1,
                "two pipeline stages on one line: {line:?}{}",
                ctx()
            );
        }

        // Pipes in a `=>` condition or a `key:` value belong to a nested value.
        let nested = code_trimmed.contains("=>") || code_trimmed.replace("::", "").contains(':');
        if !nested && pipes.iter().any(|&p| p > 0) {
            if let Some(next) = lines.get(i + 1) {
                assert!(
                    !(next.trim_start().starts_with("| ") && indent_of(next) > indent),
                    "pipeline mixes inline and broken stages at {line:?}{}",
                    ctx()
                );
            }
            // `) | stage` closing a stage that started with `| `.
            let opener = lines[..i]
                .iter()
                .rev()
                .find(|l| !l.trim().is_empty() && indent_of(l) == indent);
            if let (true, Some(opener)) = (trimmed.starts_with([')', ']', '}']), opener) {
                assert!(
                    !opener.trim_start().starts_with("| "),
                    "pipeline mixes inline and broken stages at {line:?}{}",
                    ctx()
                );
            }
        }

        if !leading || !check_fit || has_sibling_stage(&lines, i) {
            continue;
        }
        if let Some(next) = lines.get(i + 1) {
            // The stage itself broke, so its flat form is longer than this line.
            if !next.trim().is_empty() && indent_of(next) > indent {
                continue;
            }
        }
        let Some(prev) = lines[..i].iter().rev().find(|l| !l.trim().is_empty()) else {
            continue;
        };
        let joined = format!("{prev} {trimmed}");
        assert!(
            joined.width() > width,
            "pipe broke although it fits in {width} columns: {joined:?}{}",
            ctx()
        );
    }
}

fn check_structure(output: &str, ctx: &dyn Fn() -> String) {
    let mut open: Vec<(char, usize)> = Vec::new();
    for line in code_lines(output) {
        let code = line.trim_end();
        let trimmed = code.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let indent = code.len() - trimmed.len();
        let mut at_start = true;
        for c in trimmed.chars() {
            match c {
                '(' | '[' | '{' => open.push((c, indent)),
                ')' | ']' | '}' => {
                    let (_, opener_indent) = open
                        .pop()
                        .unwrap_or_else(|| panic!("unbalanced {c}{}", ctx()));
                    if at_start {
                        assert_eq!(
                            opener_indent,
                            indent,
                            "closer {c:?} is not aligned with its opener in {line:?}{}",
                            ctx()
                        );
                    }
                }
                _ => {}
            }
            at_start &= matches!(c, ')' | ']' | '}');
        }
        let last = trimmed.rsplit(' ').next().unwrap();
        let ends_with_operator = trimmed.contains(' ') && BINARY_OPERATORS.contains(&last);
        assert!(
            !ends_with_operator
                && (trimmed.starts_with(char::is_alphabetic)
                    || !BINARY_OPERATORS.contains(&trimmed)),
            "dangling operator in {line:?}{}",
            ctx()
        );
    }
}
