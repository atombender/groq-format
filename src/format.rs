//! GROQ expression formatting.
//!
//! The layout follows Prettier's conventions for JavaScript, treating GROQ
//! constructs like their closest JS counterparts:
//!
//! - A filter `[ ... ]` is laid out like the condition of an `if (...)`:
//!   when it does not fit, the brackets break around an indented body, and
//!   `&&`/`||` chains in the body break one operand per line with leading
//!   operators if the body still does not fit.
//! - Projections `{ ... }`, arrays and function arguments are laid out like
//!   JS object literals, array literals and call arguments.
//! - Slices, element access, `->`, `.field`, pipes and projections hug the
//!   expression they apply to.

use crate::doc::Doc;
use groq_parser::ast::*;

const INDENT: usize = 2;

/// A comment, classified by where it sits in the source.
#[derive(Clone, Copy)]
struct PendingComment<'a> {
    /// The comment follows code on the same line.
    trailing: bool,
    text: &'a str,
}

/// Formats GROQ syntax trees, emitting comments alongside the nodes they
/// precede in the source.
struct Formatter<'a> {
    source: &'a str,
    comments: &'a [Comment],
    /// Index of the next unconsumed comment.
    cursor: usize,
    /// Allow breaking around comparison and arithmetic operators.
    force_wrap: bool,
    /// Set while formatting the inside of `[a...b]` or `[i]`, which never break.
    in_subscript: bool,
}

impl<'a> Formatter<'a> {
    fn new(source: &'a str, comments: &'a [Comment], force_wrap: bool) -> Self {
        Formatter {
            source,
            comments,
            cursor: 0,
            force_wrap,
            in_subscript: false,
        }
    }

    // ----- Comments -------------------------------------------------------

    /// Takes all comments that start before `pos`.
    fn take_comments_before(&mut self, pos: usize) -> Vec<PendingComment<'a>> {
        let mut result = Vec::new();
        while self.cursor < self.comments.len() && self.comments[self.cursor].pos.start < pos {
            let comment = &self.comments[self.cursor];
            result.push(PendingComment {
                trailing: is_trailing_comment(self.source, comment),
                text: comment.text.trim_end(),
            });
            self.cursor += 1;
        }
        result
    }

    fn has_comments_before(&self, pos: usize) -> bool {
        self.comments
            .get(self.cursor)
            .is_some_and(|c| c.pos.start < pos)
    }

    /// Comments before `pos`, each printed on a line of its own before the
    /// node that starts there.
    fn leading_comments(&mut self, pos: usize) -> Doc {
        let comments = self.take_comments_before(pos);
        Doc::concat(
            comments
                .into_iter()
                .map(|c| Doc::concat([Doc::fresh_line(), Doc::text(c.text), Doc::hard_line()])),
        )
    }

    /// Comments before `pos` that sit between two items separated by a line
    /// break. Returns the docs to place before and after that line break, so
    /// a trailing comment stays on the line of the item it follows.
    fn separator_comments(&mut self, pos: usize) -> (Doc, Doc) {
        let mut before = Vec::new();
        let mut after = Vec::new();
        for c in self.take_comments_before(pos) {
            if c.trailing && before.is_empty() && after.is_empty() {
                before.push(trailing_comment(c.text));
            } else {
                after.push(Doc::concat([
                    Doc::fresh_line(),
                    Doc::text(c.text),
                    Doc::hard_line(),
                ]));
            }
        }
        (Doc::concat(before), Doc::concat(after))
    }

    /// Comments before `end`, the closing delimiter of a container. They are
    /// printed before the line break that precedes the delimiter.
    fn dangling_comments(&mut self, end: usize) -> Doc {
        let comments = self.take_comments_before(end);
        Doc::concat(comments.into_iter().enumerate().map(|(i, c)| {
            if c.trailing && i == 0 {
                trailing_comment(c.text)
            } else {
                Doc::concat([
                    Doc::fresh_line(),
                    Doc::line_suffix(c.text),
                    Doc::break_parent(),
                ])
            }
        }))
    }

    /// Comments before the `]` at `end` of brackets that never break. They
    /// are kept in place, followed by a newline.
    fn bracket_comments(&mut self, end: usize) -> Doc {
        let comments = self.take_comments_before(end);
        Doc::concat(comments.into_iter().enumerate().map(|(i, c)| {
            if c.trailing && i == 0 {
                Doc::concat([Doc::text(format!(" {}", c.text)), Doc::hard_line()])
            } else {
                Doc::concat([Doc::fresh_line(), Doc::text(c.text), Doc::hard_line()])
            }
        }))
    }

    /// The position of the `)` matching the `(` at or after `pos`.
    fn closing_paren(&self, pos: usize) -> usize {
        matching_close(self.source, pos).unwrap_or(pos)
    }

    // ----- Expressions ----------------------------------------------------

    fn format_expr(&mut self, expr: &Expr) -> Doc {
        let comments = self.leading_comments(leftmost_pos(expr));
        let doc = self.format_expr_inner(expr);
        Doc::concat([comments, doc])
    }

    /// Formats an expression that starts on its own indented line, such as a
    /// filter body: `&&`/`||` chains are not indented further.
    fn format_aligned(&mut self, expr: &Expr) -> Doc {
        let comments = self.leading_comments(leftmost_pos(expr));
        let doc = match expr {
            Expr::Binary(bin) if is_chain_operator(bin.operator) => self.format_chain(bin, true),
            _ if is_pipeline(expr) => {
                let pipeline = self.format_pipeline(expr, true);
                Doc::concat([pipeline.head, pipeline.stages, pipeline.suffix])
            }
            _ => self.format_expr_inner(expr),
        };
        Doc::concat([comments, doc])
    }

    fn format_expr_inner(&mut self, expr: &Expr) -> Doc {
        if is_pipeline(expr) {
            return self.format_pipeline_inline(expr);
        }
        match expr {
            Expr::Everything(_) => Doc::text("*"),
            Expr::This(_) => Doc::text("@"),
            Expr::Parent(_) => Doc::text("^"),
            Expr::Literal(lit) => self.format_literal(lit),
            Expr::Attribute(attr) => Doc::text(&attr.name),
            Expr::Param(param) => Doc::text(format!("${}", param.name)),
            Expr::Filter(_)
            | Expr::Slice(_)
            | Expr::Element(_)
            | Expr::ArrayTraversal(_)
            | Expr::Dot(_)
            | Expr::Postfix(_)
            | Expr::Projection(_) => {
                let lhs = self.format_expr(postfix_lhs(expr));
                self.format_suffix(expr, lhs)
            }
            Expr::Pipe(_) | Expr::FunctionPipe(_) => self.format_pipeline_inline(expr),
            Expr::Binary(bin) => self.format_binary(bin),
            Expr::Prefix(prefix) => self.format_prefix(prefix),
            Expr::FunctionCall(func) => self.format_function_call(func),
            Expr::Array(arr) => self.format_array(arr),
            Expr::Object(obj) => self.format_object(obj),
            Expr::Group(grp) => {
                let end = self.closing_paren(grp.pos.start);
                let inner = self.format_aligned(&grp.expression);
                let dangling = self.dangling_comments(end);
                Doc::group(Doc::concat([
                    Doc::text("("),
                    Doc::nest(INDENT, Doc::concat([Doc::softline(), inner, dangling])),
                    Doc::softline(),
                    Doc::text(")"),
                ]))
            }
            Expr::Range(range) => self.format_range(range),
            Expr::Ellipsis(_) => Doc::text("..."),
            Expr::Constraint(c) => self.format_aligned(&c.expression),
            Expr::Subscript(s) => self.format_expr(&s.value),
            Expr::Tuple(t) => {
                let end = self.closing_paren(t.pos.start);
                let items = self.format_list(&t.members, Doc::softline(), false);
                let dangling = self.dangling_comments(end);
                Doc::group(Doc::concat([
                    Doc::text("("),
                    Doc::nest(INDENT, Doc::concat([items, dangling])),
                    Doc::softline(),
                    Doc::text(")"),
                ]))
            }
        }
    }

    /// `[constraint]`, laid out like the condition of an `if (...)`.
    fn format_constraint(&mut self, constraint: &Constraint) -> Doc {
        let body = self.format_aligned(&constraint.expression);
        let dangling = self.dangling_comments(constraint.pos.end);
        Doc::group(Doc::concat([
            Doc::text("["),
            Doc::nest(INDENT, Doc::concat([Doc::softline(), body, dangling])),
            Doc::softline(),
            Doc::text("]"),
        ]))
    }

    /// The part of a filter, slice, element, traversal, dot or projection
    /// that follows its (already formatted) left-hand side.
    fn format_suffix(&mut self, expr: &Expr, lhs: Doc) -> Doc {
        match expr {
            Expr::Filter(filter) => {
                let brackets = self.format_constraint(&filter.constraint);
                Doc::concat([lhs, brackets])
            }
            Expr::Slice(slice) => {
                let range = self.format_subscript(&slice.range);
                Doc::concat([lhs, range])
            }
            Expr::Element(elem) => {
                let idx = self.format_subscript(&elem.idx);
                Doc::concat([lhs, idx])
            }
            Expr::ArrayTraversal(_) => Doc::concat([lhs, Doc::text("[]")]),
            Expr::Dot(dot) => self.format_dot(dot, lhs),
            Expr::Projection(proj) => self.format_projection(proj, lhs),
            Expr::Postfix(postfix) => self.format_postfix(postfix, lhs),
            _ => unreachable!("not a postfix expression"),
        }
    }

    /// A pipeline `head | stage | stage`, with any slice or projection kept
    /// on the stage it follows.
    ///
    /// The stages form one group, like a `&&` chain: if they all fit after
    /// the head's last line they stay there, and otherwise every `|` goes on
    /// a line of its own, at the same indent. The head is not part of the
    /// group, so a multi-line head does not force the stages to break. The
    /// last slice or projection is outside the group: the stages only have
    /// to fit up to its first bracket, as for a member chain in Prettier.
    /// With `aligned`, broken stages line up with the head (the caller
    /// already indents); otherwise they are indented.
    fn format_pipeline(&mut self, expr: &Expr, aligned: bool) -> PipelineDoc {
        // Walk the left spine: every stage may be followed by a slice,
        // projection or the like, which belongs to that stage.
        let mut stages: Vec<(Stage, Vec<&Expr>)> = Vec::new();
        let mut cur = expr;
        loop {
            let mut suffixes = Vec::new();
            while is_pipeline(cur) && !matches!(cur, Expr::Pipe(_) | Expr::FunctionPipe(_)) {
                suffixes.push(cur);
                cur = postfix_lhs(cur);
            }
            match cur {
                Expr::Pipe(pipe) => {
                    stages.push((Stage::Expr(&pipe.rhs), suffixes));
                    cur = &pipe.lhs;
                }
                Expr::FunctionPipe(fp) => {
                    stages.push((Stage::Call(&fp.func), suffixes));
                    cur = &fp.lhs;
                }
                _ => break,
            }
        }
        stages.reverse();

        let head = self.format_expr(cur);
        let mut parts = Vec::new();
        let mut last_suffix = Doc::Nil;
        let count = stages.len();
        for (i, (stage, suffixes)) in stages.into_iter().enumerate() {
            let start = match stage {
                Stage::Expr(e) => leftmost_pos(e),
                Stage::Call(f) => f.pos.start,
            };
            let (before, after) = self.separator_comments(start);
            let doc = match stage {
                Stage::Expr(e) => self.format_expr(e),
                Stage::Call(f) => self.format_function_call(f),
            };
            let mut suffix = Vec::new();
            for node in suffixes.into_iter().rev() {
                suffix.push(self.format_suffix(node, Doc::Nil));
            }
            // A suffix is indented from the line it starts on, which keeps
            // `}` aligned with that line when the stage was indented.
            let suffix = Doc::at_line(Doc::concat(suffix));
            parts.extend([before, Doc::line(), after, Doc::text("| "), doc]);
            if i + 1 == count {
                last_suffix = suffix;
            } else {
                parts.push(suffix);
            }
        }
        let stages = Doc::group(Doc::concat(parts));
        PipelineDoc {
            head,
            stages: if aligned {
                stages
            } else {
                Doc::nest(INDENT, stages)
            },
            suffix: last_suffix,
        }
    }

    fn format_pipeline_inline(&mut self, expr: &Expr) -> Doc {
        let pipeline = self.format_pipeline(expr, false);
        Doc::concat([pipeline.head, pipeline.stages, pipeline.suffix])
    }

    /// `[index]` or `[start...end]`, which never break, unless comments
    /// inside force them to break like a filter.
    fn format_subscript(&mut self, subscript: &Subscript) -> Doc {
        if self.has_comments_before(subscript.pos.end) {
            let value = self.format_aligned(&subscript.value);
            let dangling = self.dangling_comments(subscript.pos.end);
            return Doc::group(Doc::concat([
                Doc::text("["),
                Doc::nest(INDENT, Doc::concat([Doc::softline(), value, dangling])),
                Doc::softline(),
                Doc::text("]"),
            ]));
        }
        let was_in_subscript = std::mem::replace(&mut self.in_subscript, true);
        let value = self.format_expr(&subscript.value);
        self.in_subscript = was_in_subscript;
        Doc::concat([Doc::text("["), value, Doc::text("]")])
    }

    fn format_projection(&mut self, proj: &Projection, lhs: Doc) -> Doc {
        let comments = self.take_comments_before(proj.object.pos.start);
        let obj = self.format_object(&proj.object);
        if comments.is_empty() {
            let separator = if ends_with_arrow(&proj.lhs) { "" } else { " " };
            return Doc::concat([lhs, Doc::text(separator), obj]);
        }
        let mut parts = vec![lhs];
        for (i, c) in comments.into_iter().enumerate() {
            if c.trailing && i == 0 {
                parts.push(Doc::line_suffix(format!(" {}", c.text)));
            } else {
                parts.push(Doc::fresh_line());
                parts.push(Doc::text(c.text));
            }
        }
        parts.push(Doc::hard_line());
        parts.push(obj);
        Doc::concat(parts)
    }

    fn format_dot(&mut self, dot: &DotOperator, lhs: Doc) -> Doc {
        let after_arrow = ends_with_arrow(&dot.lhs);
        let after_integer = ends_with_number(&dot.lhs);

        if let Expr::Attribute(attr) = dot.rhs.as_ref() {
            let comments = self.leading_comments(attr.pos.start);
            let inner = self.bracket_comments(attr.pos.end);
            if !is_identifier(&attr.name) || after_integer || !matches!(inner, Doc::Nil) {
                let key = format!("\"{}\"", escape_string(&attr.name));
                return Doc::concat([
                    lhs,
                    comments,
                    Doc::text("["),
                    Doc::text(key),
                    inner,
                    Doc::text("]"),
                ]);
            }
            let dot_text = if after_arrow { "" } else { "." };
            return Doc::concat([lhs, Doc::text(dot_text), comments, Doc::text(&attr.name)]);
        }

        let dot_text = if after_integer { " ." } else { "." };
        let rhs = match dot.rhs.as_ref() {
            Expr::Subscript(sub) => self.format_subscript(sub),
            Expr::Constraint(c) => self.format_constraint(c),
            Expr::ArrayTraversal(at) if matches!(at.expr.as_ref(), Expr::Everything(e) if e.pos.end == 0) => {
                Doc::text("[]")
            }
            rhs => self.format_expr(rhs),
        };
        Doc::concat([lhs, Doc::text(dot_text), rhs])
    }

    fn format_binary(&mut self, bin: &BinaryOperator) -> Doc {
        if is_chain_operator(bin.operator) {
            return self.format_chain(bin, false);
        }
        match bin.operator {
            Token::Colon => {
                let key = self.format_expr(&bin.lhs);
                let value = self.format_value(&bin.rhs);
                Doc::concat([key, Doc::text(":"), value])
            }
            Token::Rocket => {
                let condition = self.format_condition(&bin.lhs, leftmost_pos(&bin.rhs));
                // Comments can force a condition onto hanging lines, after
                // which the value's brackets would no longer line up.
                let value = if condition.has_forced_break() {
                    let value = self.format_aligned(&bin.rhs);
                    Doc::nest(INDENT, Doc::concat([Doc::hard_line(), value]))
                } else {
                    self.format_value(&bin.rhs)
                };
                Doc::concat([condition, Doc::text(" =>"), value])
            }
            op if self.in_subscript && !self.has_comments_before(leftmost_pos(&bin.rhs)) => {
                let left = self.format_expr(&bin.lhs);
                let right = self.format_expr(&bin.rhs);
                Doc::concat([left, Doc::text(format!(" {} ", op.literal())), right])
            }
            _ => self.format_operator_chain(bin),
        }
    }

    /// The condition before `=>`. The value starts on the condition's last
    /// line, so the condition only breaks between the operands of a
    /// top-level `&&` or `||` chain, aligned with its first line. Comments
    /// force breaks anyway; then the value goes on a line of its own.
    fn format_condition(&mut self, condition: &Expr, end: usize) -> Doc {
        if self.has_comments_before(end) {
            return self.format_expr(condition);
        }
        let Expr::Binary(bin) = condition else {
            return Doc::flat(self.format_expr(condition));
        };
        if !is_chain_operator(bin.operator) {
            return Doc::flat(self.format_expr(condition));
        }
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        collect_chain(bin, &mut operands, &mut operators);
        let mut parts = vec![Doc::flat(self.format_expr(operands[0]))];
        for (operand, op) in operands[1..].iter().zip(operators) {
            let (before, after) = self.separator_comments(leftmost_pos(operand));
            let doc = Doc::flat(self.format_expr(operand));
            parts.extend([
                before,
                Doc::line(),
                after,
                Doc::text(format!("{} ", op.literal())),
                doc,
            ]);
        }
        Doc::group(Doc::concat(parts))
    }

    /// A comparison or arithmetic chain, which stays on one line unless
    /// breaking before its operators is the only way to make it fit (or,
    /// when force-wrapping, as soon as it does not fit).
    fn format_operator_chain(&mut self, bin: &BinaryOperator) -> Doc {
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        if is_arithmetic(bin.operator) {
            collect_chain(bin, &mut operands, &mut operators);
        } else {
            operands.extend([bin.lhs.as_ref(), bin.rhs.as_ref()]);
            operators.push(bin.operator);
        }

        let first = Doc::shared(self.format_expr(operands[0]));
        let mut hugged = vec![first.clone()];
        let mut broken = Vec::new();
        let mut has_comments = false;
        for (operand, op) in operands[1..].iter().zip(operators) {
            let (before, after) = self.separator_comments(leftmost_pos(operand));
            has_comments |= !matches!((&before, &after), (Doc::Nil, Doc::Nil));
            let doc = Doc::shared(self.format_expr(operand));
            hugged.push(Doc::text(format!(" {} ", op.literal())));
            hugged.push(doc.clone());
            broken.extend([
                before,
                Doc::line(),
                after,
                Doc::text(format!("{} ", op.literal())),
                doc,
            ]);
        }
        let broken = Doc::concat([first.clone(), Doc::nest(INDENT, Doc::concat(broken))]);
        if has_comments || self.force_wrap {
            return Doc::group(broken);
        }
        let hugged = Doc::concat(hugged);
        // An operand that breaks on its own, like `count(*[...]) > 0`, is
        // better broken inside than split from its operator, as long as it
        // ends at its starting indentation (it is not itself an operator).
        let fallback = if first.has_line() && !hangs(operands[0]) {
            hugged.clone()
        } else {
            Doc::broken_group(broken.clone())
        };
        Doc::best_fit(vec![hugged, Doc::broken_group(broken), fallback])
    }

    /// A chain of `&&`, `||` (or, when force-wrapping, arithmetic) operators,
    /// broken one operand per line with leading operators when it does not
    /// fit. When `aligned`, continuation lines are not indented, since the
    /// chain already starts on an indented line of its own.
    fn format_chain(&mut self, bin: &BinaryOperator, aligned: bool) -> Doc {
        let mut operands = Vec::new();
        let mut operators = Vec::new();
        collect_chain(bin, &mut operands, &mut operators);

        let first = self.format_expr(operands[0]);
        let mut rest = Vec::new();
        for (operand, op) in operands[1..].iter().zip(operators) {
            let (before, after) = self.separator_comments(leftmost_pos(operand));
            let doc = self.format_expr(operand);
            rest.push(before);
            rest.push(Doc::line());
            rest.push(after);
            rest.push(Doc::text(format!("{} ", op.literal())));
            rest.push(doc);
        }
        let rest = Doc::concat(rest);
        if aligned {
            Doc::group(Doc::concat([first, rest]))
        } else {
            Doc::group(Doc::concat([first, Doc::nest(INDENT, rest)]))
        }
    }

    /// The value after `key:` or `condition =>`. Values that cannot break on
    /// their own move to the next line when they do not fit; others (objects,
    /// subqueries, calls) hug the key.
    fn format_value(&mut self, value: &Expr) -> Doc {
        if is_pipeline(value) {
            // Like a value after `=` in Prettier: the pipeline stays next to
            // the key when it fits, and otherwise moves to its own indented
            // line, with its stages lined up with the head.
            let pipeline = self.format_pipeline(value, true);
            return Doc::concat([
                Doc::group(Doc::nest(
                    INDENT,
                    Doc::concat([Doc::line(), pipeline.head, pipeline.stages]),
                )),
                pipeline.suffix,
            ]);
        }
        let breaks_after = breaks_after_operator(value);
        let doc = Doc::shared(if breaks_after {
            self.format_aligned(value)
        } else {
            self.format_expr(value)
        });
        let is_chain = matches!(value, Expr::Binary(bin) if is_chain_operator(bin.operator));
        if breaks_after && (is_chain || !doc.has_line()) {
            return Doc::group(Doc::nest(INDENT, Doc::concat([Doc::line(), doc])));
        }
        let hugged = Doc::concat([Doc::text(" "), doc.clone()]);
        // Paths like `crew[job == "Director"][0].person->name` read better
        // moved to the next line than with their filter broken open.
        if !breaks_after && !(matches!(value, Expr::Dot(_)) && doc.has_line()) {
            return hugged;
        }
        // Only move to the next line if the value then fits there;
        // otherwise keep it next to the key and break inside it.
        Doc::best_fit(vec![
            hugged.clone(),
            Doc::broken_group(Doc::nest(INDENT, Doc::concat([Doc::line(), doc]))),
            hugged,
        ])
    }

    fn format_prefix(&mut self, prefix: &PrefixOperator) -> Doc {
        let op = prefix.operator.literal();
        let operand = self.format_expr(&prefix.rhs);
        let space = match (prefix.operator, prefix.rhs.as_ref()) {
            (Token::DotDotDot, rhs) => starts_with_dot(rhs),
            (Token::Minus | Token::Plus, Expr::Prefix(inner)) => {
                matches!(inner.operator, Token::Minus | Token::Plus)
            }
            (Token::Minus | Token::Plus, Expr::Literal(Literal::Integer(i))) => i.value < 0,
            _ => false,
        };
        Doc::concat([
            Doc::text(op),
            Doc::text(if space { " " } else { "" }),
            operand,
        ])
    }

    fn format_postfix(&mut self, postfix: &PostfixOperator, operand: Doc) -> Doc {
        let op = postfix.operator.literal();
        let op_text =
            if postfix.operator == Token::AscOperator || postfix.operator == Token::DescOperator {
                format!(" {}", op)
            } else {
                op.to_string()
            };
        Doc::concat([operand, Doc::text(op_text)])
    }

    fn format_function_call(&mut self, func: &FunctionCall) -> Doc {
        let name = if func.namespace.is_empty() {
            func.name.clone()
        } else {
            format!("{}::{}", func.namespace, func.name)
        };
        let end = self.closing_paren(func.pos.end);

        if func.arguments.is_empty() {
            let dangling = self.dangling_comments(end);
            if matches!(dangling, Doc::Nil) {
                return Doc::text(format!("{}()", name));
            }
            return Doc::group(Doc::concat([
                Doc::text(format!("{}(", name)),
                Doc::nest(INDENT, dangling),
                Doc::softline(),
                Doc::text(")"),
            ]));
        }

        let cursor_before = self.cursor;
        let mut args = Vec::new();
        let mut items = Vec::new();
        for (i, arg) in func.arguments.iter().enumerate() {
            if i > 0 {
                items.push(Doc::text(","));
            }
            let (before, after) = self.separator_comments(leftmost_pos(arg));
            let doc = Doc::shared(self.format_expr(arg));
            items.push(before);
            items.push(if i == 0 { Doc::softline() } else { Doc::line() });
            items.push(after);
            items.push(doc.clone());
            args.push(doc);
        }
        let dangling = self.dangling_comments(end);
        let has_comments = self.cursor != cursor_before;

        let open = Doc::text(format!("{}(", name));
        let expanded = Doc::concat([
            open.clone(),
            Doc::nest(INDENT, Doc::concat([Doc::concat(items), dangling])),
            Doc::softline(),
            Doc::text(")"),
        ]);

        if !has_comments && should_hug_last_argument(&func.arguments) {
            let (last, init) = args.split_last().unwrap();
            let mut flat = vec![open.clone()];
            let mut hugged = vec![open];
            for arg in init {
                flat.extend([arg.clone(), Doc::text(", ")]);
                hugged.extend([arg.clone(), Doc::text(", ")]);
            }
            flat.extend([last.clone(), Doc::text(")")]);
            hugged.extend([Doc::broken_group(last.clone()), Doc::text(")")]);
            return Doc::conditional_group(vec![
                Doc::concat(flat),
                Doc::concat(hugged),
                Doc::broken_group(expanded),
            ]);
        }

        let all_simple = func.arguments.iter().all(is_simple_argument);
        if all_simple && args.len() == 1 && !has_comments && !self.force_wrap {
            return Doc::concat([
                Doc::text(format!("{}(", name)),
                args[0].clone(),
                Doc::text(")"),
            ]);
        }
        if all_simple {
            Doc::tight_group(expanded)
        } else {
            Doc::group(expanded)
        }
    }

    fn format_array(&mut self, arr: &Array) -> Doc {
        let items = self.format_list(&arr.expressions, Doc::softline(), false);
        let dangling = self.dangling_comments(arr.pos.end);
        if arr.expressions.is_empty() && matches!(dangling, Doc::Nil) {
            return Doc::text("[]");
        }
        Doc::group(Doc::concat([
            Doc::text("["),
            Doc::nest(INDENT, Doc::concat([items, dangling])),
            Doc::softline(),
            Doc::text("]"),
        ]))
    }

    fn format_object(&mut self, obj: &Object) -> Doc {
        let items = self.format_list(&obj.expressions, Doc::line(), true);
        let dangling = self.dangling_comments(obj.pos.end);
        if obj.expressions.is_empty() {
            if matches!(dangling, Doc::Nil) {
                return Doc::text("{}");
            }
            return Doc::group(Doc::concat([
                Doc::text("{"),
                Doc::nest(INDENT, dangling),
                Doc::line(),
                Doc::text("}"),
            ]));
        }
        Doc::group(Doc::concat([
            Doc::text("{"),
            Doc::nest(INDENT, Doc::concat([items, dangling])),
            Doc::line(),
            Doc::text("}"),
        ]))
    }

    /// Comma-separated items, each preceded by `first_line` (for the first)
    /// or a line break, with comments kept next to the items they follow.
    fn format_list(&mut self, exprs: &[Expr], first_line: Doc, as_object_field: bool) -> Doc {
        let mut parts = Vec::new();
        for (i, expr) in exprs.iter().enumerate() {
            if i > 0 {
                parts.push(Doc::text(","));
            }
            let (before, after) = self.separator_comments(leftmost_pos(expr));
            parts.push(before);
            parts.push(if i == 0 {
                first_line.clone()
            } else {
                Doc::line()
            });
            parts.push(after);
            parts.push(if as_object_field {
                self.format_object_field(expr)
            } else {
                self.format_expr(expr)
            });
        }
        Doc::concat(parts)
    }

    fn format_object_field(&mut self, expr: &Expr) -> Doc {
        match expr {
            Expr::Attribute(attr) => Doc::text(&attr.name),
            Expr::Ellipsis(_) => Doc::text("..."),
            _ => self.format_expr(expr),
        }
    }

    fn format_range(&mut self, range: &Range) -> Doc {
        let start = self.format_expr(&range.start);
        let end = self.format_expr(&range.end);
        let op = if range.inclusive { ".." } else { "..." };
        let space = if starts_with_dot(&range.end) { " " } else { "" };
        let op = if hangs(&range.start) {
            Doc::concat([Doc::softline(), Doc::text(op), Doc::text(space)])
        } else {
            Doc::text(format!("{op}{space}"))
        };
        hang_after_operator(&range.start, start, op, end)
    }

    fn format_literal(&self, lit: &Literal) -> Doc {
        match lit {
            Literal::String(s) => Doc::text(format!("\"{}\"", escape_string(&s.value))),
            Literal::Integer(i) => Doc::text(
                self.source_text(i.pos)
                    .unwrap_or_else(|| i.value.to_string()),
            ),
            Literal::Float(f) => Doc::text(
                self.source_text(f.pos)
                    .unwrap_or_else(|| format_float(f.value)),
            ),
            Literal::Boolean(b) => Doc::text(if b.value { "true" } else { "false" }),
            Literal::Null(_) => Doc::text("null"),
        }
    }

    /// The source text of a numeric literal, to preserve how it was written.
    fn source_text(&self, pos: Position) -> Option<String> {
        let text = self.source.get(pos.start..pos.end)?;
        let is_number = !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'));
        is_number.then(|| text.to_string())
    }

    fn format_function_definition(&mut self, func: &FunctionDefinition) -> Doc {
        let name = format!("{}::{}", func.id.namespace, func.id.name);
        let params: Vec<String> = func
            .parameters
            .iter()
            .map(|p| format!("${}", p.name))
            .collect();
        let body = self.format_value(&func.body);
        Doc::concat([
            Doc::text(format!("fn {}({}) =", name, params.join(", "))),
            body,
            Doc::text(";"),
        ])
    }
}

fn trailing_comment(text: &str) -> Doc {
    Doc::concat([Doc::line_suffix(format!(" {}", text)), Doc::break_parent()])
}

enum Stage<'e> {
    Expr(&'e Expr),
    Call(&'e FunctionCall),
}

struct PipelineDoc {
    head: Doc,
    /// The `| stage` parts, as one group.
    stages: Doc,
    /// The slice, projection or the like after the last stage.
    suffix: Doc,
}

/// The expression a filter, slice, element, traversal, dot or projection
/// applies to.
fn postfix_lhs(expr: &Expr) -> &Expr {
    match expr {
        Expr::Filter(f) => &f.lhs,
        Expr::Slice(s) => &s.lhs,
        Expr::Element(e) => &e.lhs,
        Expr::ArrayTraversal(a) => &a.expr,
        Expr::Dot(d) => &d.lhs,
        Expr::Projection(p) => &p.lhs,
        Expr::Postfix(p) => &p.lhs,
        _ => unreachable!("not a postfix expression"),
    }
}

/// Whether `expr` is a `|` pipeline, possibly followed by slices,
/// projections and the like.
fn is_pipeline(expr: &Expr) -> bool {
    let mut expr = expr;
    loop {
        match expr {
            Expr::Pipe(_) | Expr::FunctionPipe(_) => return true,
            Expr::Filter(_)
            | Expr::Slice(_)
            | Expr::Element(_)
            | Expr::ArrayTraversal(_)
            | Expr::Dot(_)
            | Expr::Postfix(_)
            | Expr::Projection(_) => expr = postfix_lhs(expr),
            _ => return false,
        }
    }
}

/// Whether `expr` ends with a binary expression, whose last line hangs one
/// level deeper than its first when it breaks.
fn hangs(expr: &Expr) -> bool {
    if is_pipeline(expr) {
        return true;
    }
    match expr {
        Expr::Binary(_) => true,
        Expr::Prefix(prefix) => hangs(&prefix.rhs),
        Expr::Postfix(postfix) => hangs(&postfix.lhs),
        Expr::Range(range) => hangs(&range.end),
        _ => false,
    }
}

/// `lhs op rhs`. When `lhs` is a binary expression that breaks, its last
/// line hangs one level deeper, so `op` (which then starts with a line
/// break) moves to the start of a line and `rhs` lines up with `lhs`.
fn hang_after_operator(lhs_expr: &Expr, lhs: Doc, op: Doc, rhs: Doc) -> Doc {
    if hangs(lhs_expr) {
        Doc::group(Doc::concat([lhs, op, rhs]))
    } else {
        Doc::concat([lhs, op, rhs])
    }
}

fn is_chain_operator(op: Token) -> bool {
    matches!(op, Token::And | Token::Or)
}

fn is_arithmetic(op: Token) -> bool {
    matches!(
        op,
        Token::Plus | Token::Minus | Token::Asterisk | Token::Slash | Token::Percent
    )
}

/// Operators that can be flattened into one chain with `op`.
fn same_chain(a: Token, b: Token) -> bool {
    match (a, b) {
        (Token::Plus | Token::Minus, Token::Plus | Token::Minus) => true,
        (Token::Asterisk | Token::Slash, Token::Asterisk | Token::Slash) => true,
        (a, b) => a == b,
    }
}

/// Flattens left-nested operators of the same precedence into a list of
/// operands and the operators between them.
fn collect_chain<'e>(
    bin: &'e BinaryOperator,
    operands: &mut Vec<&'e Expr>,
    operators: &mut Vec<Token>,
) {
    match bin.lhs.as_ref() {
        Expr::Binary(lhs) if same_chain(lhs.operator, bin.operator) => {
            collect_chain(lhs, operands, operators)
        }
        lhs => operands.push(lhs),
    }
    operators.push(bin.operator);
    operands.push(&bin.rhs);
}

/// Whether a value after `key:` should move to its own line when it does not
/// fit, rather than hugging the key and breaking internally.
fn breaks_after_operator(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::Param(_) | Expr::Attribute(_) => true,
        Expr::This(_) | Expr::Parent(_) | Expr::Everything(_) => true,
        Expr::Binary(bin) => !matches!(bin.operator, Token::Colon | Token::Rocket),
        Expr::Prefix(prefix) => prefix.operator != Token::DotDotDot,
        Expr::Dot(_) | Expr::Postfix(_) | Expr::ArrayTraversal(_) | Expr::Element(_) => {
            is_simple_path(expr)
        }
        _ => false,
    }
}

/// A chain of attribute accesses and dereferences, like `author->bio.name`.
fn is_simple_path(expr: &Expr) -> bool {
    match expr {
        Expr::Attribute(_) | Expr::Param(_) | Expr::This(_) | Expr::Parent(_) => true,
        Expr::Everything(_) => true,
        Expr::Dot(dot) => is_simple_path(&dot.lhs) && is_simple_path(&dot.rhs),
        Expr::Postfix(p) => is_simple_path(&p.lhs),
        Expr::ArrayTraversal(at) => is_simple_path(&at.expr),
        Expr::Element(e) => is_simple_path(&e.lhs),
        _ => false,
    }
}

/// Short arguments like `title`, `"x"`, `$p`, `date desc` or `now()`, which
/// make a call that is better kept on one line.
fn is_simple_argument(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(Literal::String(s)) => s.value.len() <= 20,
        Expr::Literal(_) | Expr::Ellipsis(_) => true,
        Expr::Prefix(p) => is_simple_argument(&p.rhs),
        Expr::Postfix(p) => is_simple_argument(&p.lhs),
        Expr::FunctionCall(f) => {
            f.arguments.len() <= 3
                && f.arguments
                    .iter()
                    .all(|a| is_simple_path(a) || matches!(a, Expr::Literal(_)))
        }
        _ => is_simple_path(expr),
    }
}

/// Whether to hug the last argument, Prettier-style: `count(*[...])` keeps
/// `count(*[` on one line and breaks inside the subquery instead.
fn should_hug_last_argument(args: &[Expr]) -> bool {
    let (last, init) = match args.split_last() {
        Some(split) => split,
        None => return false,
    };
    if !init.iter().all(is_simple_argument) {
        return false;
    }
    match last {
        Expr::Object(obj) => !obj.expressions.is_empty(),
        Expr::Array(arr) => !arr.expressions.is_empty(),
        _ => init.is_empty() && ends_with_filter_or_projection(last),
    }
}

/// Whether a subquery ends with a filter or projection, optionally followed
/// by a slice or index, like `*[...]`, `*[...] { ... }` or `*[...][0...3]`.
fn ends_with_filter_or_projection(expr: &Expr) -> bool {
    if is_pipeline(expr) {
        return false;
    }
    match expr {
        Expr::Filter(_) | Expr::Projection(_) => true,
        Expr::Slice(s) => ends_with_filter_or_projection(&s.lhs),
        Expr::Element(e) => ends_with_filter_or_projection(&e.lhs),
        _ => false,
    }
}

/// Whether the printed form of `expr` ends with a number, after which `.`
/// would be read as a decimal point.
fn ends_with_number(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(Literal::Integer(_) | Literal::Float(_)) => true,
        Expr::Dot(dot) => ends_with_number(&dot.rhs),
        _ => false,
    }
}

fn ends_with_arrow(expr: &Expr) -> bool {
    matches!(expr, Expr::Postfix(p) if p.operator == Token::Arrow)
}

/// Whether the printed form of `expr` starts with `.`.
fn starts_with_dot(expr: &Expr) -> bool {
    match expr {
        Expr::Ellipsis(_) => true,
        Expr::Prefix(p) => p.operator == Token::DotDotDot,
        Expr::Range(r) => starts_with_dot(&r.start),
        Expr::Binary(b) => starts_with_dot(&b.lhs),
        Expr::Dot(d) => starts_with_dot(&d.lhs),
        Expr::Postfix(p) => starts_with_dot(&p.lhs),
        Expr::Filter(f) => starts_with_dot(&f.lhs),
        Expr::Slice(s) => starts_with_dot(&s.lhs),
        Expr::Element(e) => starts_with_dot(&e.lhs),
        Expr::ArrayTraversal(a) => starts_with_dot(&a.expr),
        Expr::Projection(p) => starts_with_dot(&p.lhs),
        Expr::FunctionPipe(p) => starts_with_dot(&p.lhs),
        Expr::Pipe(p) => starts_with_dot(&p.lhs),
        _ => false,
    }
}

/// Whether `name` can be written as a bare attribute after `.` or `->`.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(name, "true" | "false" | "null")
}

fn escape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

fn format_float(value: f64) -> String {
    let s = format!("{}", value);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

/// The leftmost source position of an expression, before which comments
/// are attached to it. Many compound nodes have `pos` set to their operator
/// rather than the start of the expression. Comments between a prefix
/// operator and its operand also count as before the expression.
fn leftmost_pos(expr: &Expr) -> usize {
    match expr {
        Expr::Prefix(p) => leftmost_pos(&p.rhs),
        Expr::Projection(p) => leftmost_pos(&p.lhs),
        Expr::Filter(f) => leftmost_pos(&f.lhs),
        Expr::Pipe(p) => leftmost_pos(&p.lhs),
        Expr::FunctionPipe(fp) => leftmost_pos(&fp.lhs),
        Expr::Dot(d) => leftmost_pos(&d.lhs),
        Expr::Binary(b) => leftmost_pos(&b.lhs),
        Expr::Postfix(p) => leftmost_pos(&p.lhs),
        Expr::Slice(s) => leftmost_pos(&s.lhs),
        Expr::Element(e) => leftmost_pos(&e.lhs),
        Expr::ArrayTraversal(a) => leftmost_pos(&a.expr),
        Expr::Range(r) => leftmost_pos(&r.start),
        _ => expr.get_pos().start,
    }
}

/// Whether a comment follows code on the same line.
fn is_trailing_comment(source: &str, comment: &Comment) -> bool {
    let before = &source[..comment.pos.start];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    before[line_start..].chars().any(|c| !c.is_whitespace())
}

/// The byte offset of the `)` matching the first `(` at or after `pos`,
/// skipping over strings and comments.
fn matching_close(source: &str, pos: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = pos;
    let mut depth = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (bytes[i] == b')').then_some(i);
                }
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Formats a full parse result (function definitions and the query) as a
/// document. `source` is the original query text, used to place comments.
/// When `force_wrap` is true, comparison and arithmetic operators also
/// become break points.
pub fn format_parse_result(result: &ParseResult, source: &str, force_wrap: bool) -> Doc {
    let mut fmt = Formatter::new(source, &result.comments, force_wrap);

    let mut parts: Vec<Doc> = Vec::new();

    let mut def_start = 0;
    for (i, func) in result.functions.iter().enumerate() {
        let (before, after) = fmt.separator_comments(skip_trivia(source, def_start));
        parts.push(before);
        if i > 0 {
            parts.push(Doc::hard_line());
        }
        parts.push(after);
        parts.push(fmt.format_function_definition(func));
        def_start = func.pos.start + 1;
    }

    let expr_start = leftmost_pos(&result.expr);
    let comments = fmt.take_comments_before(expr_start);
    if !result.functions.is_empty() {
        let trailing: Vec<_> = comments.iter().take(1).filter(|c| c.trailing).collect();
        for c in &trailing {
            parts.push(trailing_comment(c.text));
        }
        parts.push(Doc::hard_line());
        parts.push(Doc::hard_line());
        for c in &comments[trailing.len()..] {
            parts.push(Doc::text(c.text));
            parts.push(Doc::hard_line());
        }
    } else {
        for c in &comments {
            parts.push(Doc::text(c.text));
            parts.push(Doc::hard_line());
        }
    }

    parts.push(fmt.format_expr(&result.expr));

    for (i, c) in fmt.take_comments_before(usize::MAX).into_iter().enumerate() {
        if c.trailing && i == 0 {
            parts.push(Doc::line_suffix(format!(" {}", c.text)));
        } else {
            parts.push(Doc::hard_line());
            parts.push(Doc::text(c.text));
        }
    }

    Doc::concat(parts)
}

/// The position of the first token at or after `pos`, skipping whitespace
/// and comments.
fn skip_trivia(source: &str, mut pos: usize) -> usize {
    let bytes = source.as_bytes();
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
        } else if bytes[pos] == b'/' && bytes.get(pos + 1) == Some(&b'/') {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

/// Formats a GROQ expression as a document, without comments.
pub fn format_expr(expr: &Expr) -> Doc {
    let mut fmt = Formatter::new("", &[], false);
    fmt.format_expr(expr)
}
