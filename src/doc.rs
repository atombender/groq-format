//! A Wadler-style document algebra and printer, modelled on Prettier's.
//!
//! Documents are built from text, line breaks, indentation and groups. When
//! printed, every group is laid out flat if its contents, together with
//! everything that follows up to the next possible line break, fit in the
//! remaining width; otherwise its line breaks become newlines.
//!
//! Beyond the classic algebra, this supports the parts of Prettier's doc
//! builder that GROQ layout needs: `ifBreak`, `conditionalGroup` (for hugging
//! a call's last argument), `lineSuffix` (for trailing `//` comments) and
//! hard breaks that propagate to every enclosing group.

use std::rc::Rc;

use unicode_width::UnicodeWidthStr;

/// A document to be pretty-printed.
#[derive(Debug, Clone)]
pub enum Doc {
    /// Empty document.
    Nil,
    /// Literal text. Must not contain newlines.
    Text(String),
    /// Concatenation of documents.
    Concat(Vec<Doc>),
    /// Increases the indentation of line breaks inside `doc`.
    Nest { indent: usize, doc: Box<Doc> },
    /// Lays out `doc` flat if it fits, otherwise breaks its lines.
    Group(Box<Group>),
    /// Tries each state in turn and prints the first one that fits; the last
    /// state is used when none does. Like Prettier's `conditionalGroup`, a
    /// state fits when its first line fits, unless `all_lines` is set, in
    /// which case every line of the state must fit with its groups flat.
    Conditional {
        states: Vec<Doc>,
        should_break: bool,
        all_lines: bool,
    },
    /// A line break; see [`LineKind`].
    Line(LineKind),
    /// `broken` when the enclosing group is broken, `flat` otherwise.
    IfBreak { broken: Box<Doc>, flat: Box<Doc> },
    /// Printed just before the next newline (used for trailing comments).
    LineSuffix(Box<Doc>),
    /// Forces every enclosing group to break.
    BreakParent,
    /// A document shared between the states of a [`Doc::Conditional`].
    Shared(Rc<Doc>),
    /// Prints `doc` with every group flat (forced breaks still break).
    Flat(Box<Doc>),
    /// Prints `doc` indented relative to the line it starts on, rather than
    /// the indentation of its parent. A projection that continues an
    /// already-indented line then closes on that line's indent.
    AtLine(Box<Doc>),
}

/// The contents of a [`Doc::Group`].
#[derive(Debug, Clone)]
pub struct Group {
    pub doc: Doc,
    /// Always break, regardless of width.
    pub should_break: bool,
    /// When measuring whether something *before* this group fits, assume this
    /// group stays flat instead of assuming it can break at its first line.
    /// Used for groups that should only break as a last resort.
    pub tight: bool,
    /// Whether `doc` contains a forced line break (cached).
    forced: bool,
}

/// The kinds of line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A space when flat, a newline when broken.
    Space,
    /// Nothing when flat, a newline when broken.
    Soft,
    /// Always a newline.
    Hard,
    /// A newline unless the current line is still empty.
    Fresh,
}

impl Doc {
    /// Literal text.
    pub fn text(s: impl Into<String>) -> Doc {
        let s = s.into();
        if s.is_empty() { Doc::Nil } else { Doc::Text(s) }
    }

    /// A line break that becomes a space in flat mode.
    pub fn line() -> Doc {
        Doc::Line(LineKind::Space)
    }

    /// A line break that disappears in flat mode.
    pub fn softline() -> Doc {
        Doc::Line(LineKind::Soft)
    }

    /// Alias of [`Doc::softline`].
    pub fn line_or_empty() -> Doc {
        Doc::softline()
    }

    /// A forced line break that also breaks every enclosing group.
    pub fn hard_line() -> Doc {
        Doc::Line(LineKind::Hard)
    }

    /// A forced line break, unless the current line has no content yet.
    pub fn fresh_line() -> Doc {
        Doc::Line(LineKind::Fresh)
    }

    /// Indents line breaks inside `doc` by `indent` additional spaces.
    pub fn nest(indent: usize, doc: Doc) -> Doc {
        match doc {
            Doc::Nil => Doc::Nil,
            doc => Doc::Nest {
                indent,
                doc: Box::new(doc),
            },
        }
    }

    /// A group that is laid out flat when it fits.
    pub fn group(doc: Doc) -> Doc {
        Doc::make_group(doc, false, false)
    }

    /// A group that always breaks.
    pub fn broken_group(doc: Doc) -> Doc {
        Doc::make_group(doc, true, false)
    }

    /// A group that is assumed to stay flat when measuring what precedes it.
    pub fn tight_group(doc: Doc) -> Doc {
        Doc::make_group(doc, false, true)
    }

    fn make_group(doc: Doc, should_break: bool, tight: bool) -> Doc {
        let forced = doc.has_forced_break();
        Doc::Group(Box::new(Group {
            doc,
            should_break: should_break || forced,
            tight,
            forced,
        }))
    }

    /// Prints the first of `states` whose first line fits, or the last one.
    pub fn conditional_group(states: Vec<Doc>) -> Doc {
        Doc::make_conditional(states, false)
    }

    /// Prints the first of `states` whose every line fits, or the last one.
    pub fn best_fit(states: Vec<Doc>) -> Doc {
        Doc::make_conditional(states, true)
    }

    fn make_conditional(states: Vec<Doc>, all_lines: bool) -> Doc {
        assert!(!states.is_empty());
        let should_break = states.iter().any(Doc::has_forced_break);
        Doc::Conditional {
            states,
            should_break,
            all_lines,
        }
    }

    /// `broken` when the enclosing group breaks, `flat` otherwise.
    pub fn if_break(broken: Doc, flat: Doc) -> Doc {
        Doc::IfBreak {
            broken: Box::new(broken),
            flat: Box::new(flat),
        }
    }

    /// Text deferred to the end of the current line.
    pub fn line_suffix(s: impl Into<String>) -> Doc {
        Doc::LineSuffix(Box::new(Doc::text(s)))
    }

    /// Forces all enclosing groups to break.
    pub fn break_parent() -> Doc {
        Doc::BreakParent
    }

    /// Wraps a document so it can be cheaply reused in several places.
    pub fn shared(doc: Doc) -> Doc {
        match doc {
            Doc::Shared(rc) => Doc::Shared(rc),
            doc => Doc::Shared(Rc::new(doc)),
        }
    }

    /// Indents `doc` relative to the line it is printed on.
    pub fn at_line(doc: Doc) -> Doc {
        match doc {
            Doc::Nil => Doc::Nil,
            doc => Doc::AtLine(Box::new(doc)),
        }
    }

    /// Lays out `doc` with all of its groups flat.
    pub fn flat(doc: Doc) -> Doc {
        match doc {
            Doc::Nil => Doc::Nil,
            Doc::Text(s) => Doc::Text(s),
            doc => Doc::Flat(Box::new(doc)),
        }
    }

    /// Concatenates documents.
    pub fn concat(docs: impl IntoIterator<Item = Doc>) -> Doc {
        let mut parts = Vec::new();
        for doc in docs {
            match doc {
                Doc::Nil => {}
                Doc::Concat(inner) => parts.extend(inner),
                doc => parts.push(doc),
            }
        }
        match parts.len() {
            0 => Doc::Nil,
            1 => parts.pop().unwrap(),
            _ => Doc::Concat(parts),
        }
    }

    /// Joins documents with a separator.
    pub fn join(sep: Doc, docs: Vec<Doc>) -> Doc {
        let mut parts = Vec::with_capacity(docs.len() * 2);
        for (i, doc) in docs.into_iter().enumerate() {
            if i > 0 {
                parts.push(sep.clone());
            }
            parts.push(doc);
        }
        Doc::concat(parts)
    }

    /// Whether this document contains any line break.
    pub fn has_line(&self) -> bool {
        match self {
            Doc::Line(_) => true,
            Doc::Nil | Doc::Text(_) | Doc::LineSuffix(_) | Doc::BreakParent => false,
            Doc::Concat(parts) => parts.iter().any(Doc::has_line),
            Doc::Nest { doc, .. } => doc.has_line(),
            Doc::Group(group) => group.doc.has_line(),
            Doc::Conditional { states, .. } => states.iter().any(Doc::has_line),
            Doc::IfBreak { broken, flat } => broken.has_line() || flat.has_line(),
            Doc::Shared(doc) => doc.has_line(),
            Doc::Flat(doc) | Doc::AtLine(doc) => doc.has_line(),
        }
    }

    /// Whether printing this document always produces a newline.
    pub fn has_forced_break(&self) -> bool {
        match self {
            Doc::Line(LineKind::Hard | LineKind::Fresh) | Doc::BreakParent => true,
            Doc::Nil | Doc::Text(_) | Doc::Line(_) | Doc::LineSuffix(_) => false,
            Doc::Concat(parts) => parts.iter().any(Doc::has_forced_break),
            Doc::Nest { doc, .. } => doc.has_forced_break(),
            Doc::Group(group) => group.forced,
            Doc::Conditional { should_break, .. } => *should_break,
            Doc::IfBreak { broken, flat } => broken.has_forced_break() || flat.has_forced_break(),
            Doc::Shared(doc) => doc.has_forced_break(),
            Doc::Flat(doc) | Doc::AtLine(doc) => doc.has_forced_break(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Flat,
    Break,
}

#[derive(Clone, Copy)]
struct Cmd<'a> {
    indent: usize,
    mode: Mode,
    doc: &'a Doc,
}

/// Width of `s` in terminal columns.
pub fn text_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Renders a document to a string, wrapping at `width` columns.
pub fn pretty(width: usize, doc: Doc) -> String {
    let mut out = String::new();
    let mut pos: usize = 0;
    let mut cmds = vec![Cmd {
        indent: 0,
        mode: Mode::Break,
        doc: &doc,
    }];
    let mut line_suffixes: Vec<Cmd> = Vec::new();
    let mut should_remeasure = false;
    // Indentation of the line currently being printed.
    let mut line_indent: usize = 0;

    while let Some(cmd) = cmds.pop() {
        match cmd.doc {
            Doc::Nil | Doc::BreakParent => {}
            Doc::Text(s) => {
                out.push_str(s);
                pos += text_width(s);
            }
            Doc::Shared(doc) => cmds.push(Cmd { doc, ..cmd }),
            Doc::Flat(doc) => {
                should_remeasure = false;
                cmds.push(Cmd {
                    doc,
                    mode: Mode::Flat,
                    ..cmd
                });
            }
            Doc::AtLine(doc) => cmds.push(Cmd {
                indent: line_indent,
                doc,
                ..cmd
            }),
            Doc::Concat(parts) => {
                for doc in parts.iter().rev() {
                    cmds.push(Cmd { doc, ..cmd });
                }
            }
            Doc::Nest { indent, doc } => cmds.push(Cmd {
                indent: cmd.indent + indent,
                mode: cmd.mode,
                doc,
            }),
            Doc::Group(group) => {
                if cmd.mode == Mode::Flat && !should_remeasure {
                    let mode = if group.should_break {
                        Mode::Break
                    } else {
                        Mode::Flat
                    };
                    cmds.push(Cmd {
                        indent: cmd.indent,
                        mode,
                        doc: &group.doc,
                    });
                    continue;
                }
                should_remeasure = false;
                let flat = Cmd {
                    indent: cmd.indent,
                    mode: Mode::Flat,
                    doc: &group.doc,
                };
                let remaining = width as isize - pos as isize;
                if !group.should_break && fits(flat, &cmds, remaining, false) {
                    cmds.push(flat);
                } else {
                    cmds.push(Cmd {
                        mode: Mode::Break,
                        ..flat
                    });
                }
            }
            Doc::Conditional {
                states,
                should_break,
                all_lines,
            } => {
                let last = states.last().unwrap();
                if cmd.mode == Mode::Flat && !should_remeasure {
                    let (mode, doc) = if *should_break {
                        (Mode::Break, last)
                    } else {
                        (Mode::Flat, &states[0])
                    };
                    cmds.push(Cmd {
                        indent: cmd.indent,
                        mode,
                        doc,
                    });
                    continue;
                }
                should_remeasure = false;
                let remaining = width as isize - pos as isize;
                let mut chosen = Cmd {
                    indent: cmd.indent,
                    mode: Mode::Break,
                    doc: last,
                };
                if !*should_break {
                    for state in states {
                        let candidate = Cmd {
                            indent: cmd.indent,
                            mode: Mode::Flat,
                            doc: state,
                        };
                        let fit = if *all_lines {
                            fits_all_lines(candidate, &cmds, remaining, width)
                        } else {
                            fits(candidate, &cmds, remaining, false)
                        };
                        if fit {
                            chosen = candidate;
                            break;
                        }
                    }
                }
                cmds.push(chosen);
            }
            Doc::IfBreak { broken, flat } => cmds.push(Cmd {
                doc: if cmd.mode == Mode::Break {
                    broken
                } else {
                    flat
                },
                ..cmd
            }),
            Doc::LineSuffix(doc) => line_suffixes.push(Cmd { doc, ..cmd }),
            Doc::Line(kind) => {
                if cmd.mode == Mode::Flat {
                    match kind {
                        LineKind::Space => {
                            out.push(' ');
                            pos += 1;
                            continue;
                        }
                        LineKind::Soft => continue,
                        LineKind::Hard | LineKind::Fresh => should_remeasure = true,
                    }
                }
                if !line_suffixes.is_empty() {
                    cmds.push(cmd);
                    for suffix in line_suffixes.drain(..).rev() {
                        cmds.push(suffix);
                    }
                    continue;
                }
                if *kind == LineKind::Fresh && current_line_is_blank(&out) {
                    continue;
                }
                trim_trailing_spaces(&mut out);
                out.push('\n');
                out.extend(std::iter::repeat_n(' ', cmd.indent));
                pos = cmd.indent;
                line_indent = cmd.indent;
            }
        }

        if cmds.is_empty() && !line_suffixes.is_empty() {
            for suffix in line_suffixes.drain(..).rev() {
                cmds.push(suffix);
            }
        }
    }

    trim_trailing_spaces(&mut out);
    out
}

fn current_line_is_blank(out: &str) -> bool {
    let line = match out.rfind('\n') {
        Some(i) => &out[i + 1..],
        None => out,
    };
    line.chars().all(|c| c == ' ')
}

fn trim_trailing_spaces(out: &mut String) {
    let trimmed = out.trim_end_matches(' ').len();
    out.truncate(trimmed);
}

/// Whether `next`, followed by the pending `rest` commands up to their first
/// line break, fits in `width` columns.
fn fits(next: Cmd, rest: &[Cmd], width: isize, must_be_flat: bool) -> bool {
    let mut width = width;
    let mut rest_idx = rest.len();
    let mut stack: Vec<(Mode, &Doc)> = vec![(next.mode, next.doc)];

    while width >= 0 {
        let (mode, doc) = match stack.pop() {
            Some(item) => item,
            None => {
                if rest_idx == 0 {
                    return true;
                }
                rest_idx -= 1;
                (rest[rest_idx].mode, rest[rest_idx].doc)
            }
        };
        match doc {
            Doc::Nil | Doc::BreakParent | Doc::LineSuffix(_) => {}
            Doc::Text(s) => width -= text_width(s) as isize,
            Doc::Shared(doc) => stack.push((mode, doc)),
            Doc::Flat(doc) => stack.push((Mode::Flat, doc)),
            Doc::AtLine(doc) => stack.push((mode, doc)),
            Doc::Concat(parts) => {
                for part in parts.iter().rev() {
                    stack.push((mode, part));
                }
            }
            Doc::Nest { doc, .. } => stack.push((mode, doc)),
            Doc::Group(group) => {
                if must_be_flat && group.should_break {
                    return false;
                }
                let group_mode = if group.should_break {
                    Mode::Break
                } else if group.tight {
                    Mode::Flat
                } else {
                    mode
                };
                stack.push((group_mode, &group.doc));
            }
            Doc::Conditional {
                states,
                should_break,
                ..
            } => {
                if must_be_flat && *should_break {
                    return false;
                }
                let group_mode = if *should_break { Mode::Break } else { mode };
                let doc = if group_mode == Mode::Break {
                    states.last().unwrap()
                } else {
                    &states[0]
                };
                stack.push((group_mode, doc));
            }
            Doc::IfBreak { broken, flat } => {
                stack.push((mode, if mode == Mode::Break { broken } else { flat }));
            }
            Doc::Line(kind) => {
                if mode == Mode::Break || matches!(kind, LineKind::Hard | LineKind::Fresh) {
                    return true;
                }
                if *kind == LineKind::Space {
                    width -= 1;
                }
            }
        }
    }
    false
}

/// Whether every line of `next`, printed with its groups flat unless they
/// must break, fits in `width` columns; the line `next` starts on has
/// `remaining` columns left. The pending `rest` must fit after the last line
/// up to its first line break, as in [`fits`].
fn fits_all_lines(next: Cmd, rest: &[Cmd], remaining: isize, width: usize) -> bool {
    let mut remaining = remaining;
    let mut line_indent = next.indent;
    let mut stack: Vec<(usize, Mode, &Doc)> = vec![(next.indent, next.mode, next.doc)];
    while let Some((indent, mode, doc)) = stack.pop() {
        match doc {
            Doc::Nil | Doc::BreakParent | Doc::LineSuffix(_) => {}
            Doc::Text(s) => {
                remaining -= text_width(s) as isize;
                if remaining < 0 {
                    return false;
                }
            }
            Doc::Shared(doc) => stack.push((indent, mode, doc)),
            Doc::Flat(doc) => stack.push((indent, Mode::Flat, doc)),
            Doc::AtLine(doc) => stack.push((line_indent, mode, doc)),
            Doc::Concat(parts) => {
                for part in parts.iter().rev() {
                    stack.push((indent, mode, part));
                }
            }
            Doc::Nest { indent: extra, doc } => stack.push((indent + extra, mode, doc)),
            Doc::Group(group) => {
                let mode = if group.should_break {
                    Mode::Break
                } else {
                    Mode::Flat
                };
                stack.push((indent, mode, &group.doc));
            }
            Doc::Conditional {
                states,
                should_break,
                ..
            } => {
                if *should_break {
                    stack.push((indent, Mode::Break, states.last().unwrap()));
                } else {
                    stack.push((indent, Mode::Flat, &states[0]));
                }
            }
            Doc::IfBreak { broken, flat } => {
                stack.push((
                    indent,
                    mode,
                    if mode == Mode::Break { broken } else { flat },
                ));
            }
            Doc::Line(kind) => {
                if mode == Mode::Break || matches!(kind, LineKind::Hard | LineKind::Fresh) {
                    remaining = width as isize - indent as isize;
                    line_indent = indent;
                } else if *kind == LineKind::Space {
                    remaining -= 1;
                    if remaining < 0 {
                        return false;
                    }
                }
            }
        }
    }
    let end = Cmd {
        indent: next.indent,
        mode: Mode::Flat,
        doc: &Doc::Nil,
    };
    fits(end, rest, remaining, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_fits_flat() {
        let doc = Doc::group(Doc::concat([
            Doc::text("["),
            Doc::nest(2, Doc::concat([Doc::softline(), Doc::text("a, b")])),
            Doc::softline(),
            Doc::text("]"),
        ]));
        assert_eq!(pretty(80, doc.clone()), "[a, b]");
        assert_eq!(pretty(3, doc), "[\n  a, b\n]");
    }

    #[test]
    fn fits_accounts_for_trailing_text() {
        let doc = Doc::concat([
            Doc::group(Doc::concat([
                Doc::text("["),
                Doc::nest(2, Doc::concat([Doc::softline(), Doc::text("abc")])),
                Doc::softline(),
                Doc::text("]"),
            ])),
            Doc::text("[0...5]"),
        ]);
        assert_eq!(pretty(12, doc.clone()), "[abc][0...5]");
        assert_eq!(pretty(11, doc), "[\n  abc\n][0...5]");
    }

    #[test]
    fn line_suffix_is_flushed_before_newline() {
        let doc = Doc::concat([
            Doc::text("a"),
            Doc::line_suffix(" // c"),
            Doc::text(","),
            Doc::hard_line(),
            Doc::text("b"),
        ]);
        assert_eq!(pretty(80, doc), "a, // c\nb");
    }

    #[test]
    fn hard_line_breaks_enclosing_groups() {
        let doc = Doc::group(Doc::concat([
            Doc::text("{"),
            Doc::nest(
                2,
                Doc::concat([
                    Doc::line(),
                    Doc::text("a"),
                    Doc::hard_line(),
                    Doc::text("b"),
                ]),
            ),
            Doc::line(),
            Doc::text("}"),
        ]));
        assert_eq!(pretty(80, doc), "{\n  a\n  b\n}");
    }

    #[test]
    fn fresh_line_is_skipped_at_line_start() {
        let doc = Doc::concat([
            Doc::text("a"),
            Doc::fresh_line(),
            Doc::fresh_line(),
            Doc::text("b"),
        ]);
        assert_eq!(pretty(80, doc), "a\nb");
    }

    #[test]
    fn conditional_group_picks_first_fitting_state() {
        let states = vec![Doc::text("aaaaaaaa"), Doc::text("bbbb"), Doc::text("c")];
        assert_eq!(
            pretty(10, Doc::conditional_group(states.clone())),
            "aaaaaaaa"
        );
        assert_eq!(pretty(5, Doc::conditional_group(states.clone())), "bbbb");
        assert_eq!(pretty(1, Doc::conditional_group(states)), "c");
    }
}
