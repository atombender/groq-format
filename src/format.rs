//! GROQ expression formatting.

use crate::doc::Doc;
use groq_parser::ast::*;

/// Format a full parse result (function definitions + expression) as a document.
pub fn format_parse_result(result: &ParseResult) -> Doc {
    if result.functions.is_empty() {
        return format_expr(&result.expr);
    }

    let func_docs: Vec<Doc> = result
        .functions
        .iter()
        .map(format_function_definition)
        .collect();
    let funcs = Doc::join(Doc::text("\n"), func_docs);

    Doc::concat([funcs, Doc::text("\n\n"), format_expr(&result.expr)])
}

fn format_function_definition(func: &FunctionDefinition) -> Doc {
    let name = format!("{}::{}", func.id.namespace, func.id.name);
    let params: Vec<String> = func
        .parameters
        .iter()
        .map(|p| format!("${}", p.name))
        .collect();
    let params_str = params.join(", ");

    Doc::concat([
        Doc::text(format!("fn {}({}) = ", name, params_str)),
        format_expr(&func.body),
        Doc::text(";"),
    ])
}

/// Format a GROQ expression as a document.
pub fn format_expr(expr: &Expr) -> Doc {
    match expr {
        Expr::Everything(_) => Doc::text("*"),
        Expr::This(_) => Doc::text("@"),
        Expr::Parent(_) => Doc::text("^"),
        Expr::Literal(lit) => format_literal(lit),
        Expr::Attribute(attr) => Doc::text(&attr.name),
        Expr::Param(param) => Doc::text(format!("${}", param.name)),
        Expr::Filter(filter) => {
            let lhs = format_expr(&filter.lhs);
            let constraint = format_expr(&filter.constraint.expression);
            Doc::concat([
                lhs,
                Doc::group(Doc::concat([Doc::text("["), constraint])),
                Doc::text("]"),
            ])
        }
        Expr::Slice(slice) => {
            let lhs = format_expr(&slice.lhs);
            let range = format_expr(&slice.range.value);
            Doc::concat([lhs, Doc::text("["), range, Doc::text("]")])
        }
        Expr::Element(elem) => {
            let lhs = format_expr(&elem.lhs);
            let idx = format_expr(&elem.idx.value);
            Doc::concat([lhs, Doc::text("["), idx, Doc::text("]")])
        }
        Expr::ArrayTraversal(at) => Doc::concat([format_expr(&at.expr), Doc::text("[]")]),
        Expr::Dot(dot) => format_dot(dot),
        Expr::Projection(proj) => {
            let lhs = format_expr(&proj.lhs);
            let obj = format_object(&proj.object);
            Doc::concat([lhs, Doc::text(" "), obj])
        }
        Expr::Pipe(pipe) => {
            let lhs = format_expr(&pipe.lhs);
            let rhs = format_expr(&pipe.rhs);
            Doc::group(Doc::concat([
                lhs,
                Doc::nest(2, Doc::concat([Doc::line(), Doc::text("| "), rhs])),
            ]))
        }
        Expr::FunctionPipe(fp) => {
            let lhs = format_expr(&fp.lhs);
            let func = format_function_call(&fp.func);
            Doc::group(Doc::concat([
                lhs,
                Doc::nest(2, Doc::concat([Doc::line(), Doc::text("| "), func])),
            ]))
        }
        Expr::Binary(bin) => format_binary(bin),
        Expr::Prefix(prefix) => format_prefix(prefix),
        Expr::Postfix(postfix) => format_postfix(postfix),
        Expr::FunctionCall(func) => format_function_call(func),
        Expr::Array(arr) => format_array(arr),
        Expr::Object(obj) => format_object(obj),
        Expr::Group(grp) => {
            Doc::concat([Doc::text("("), format_expr(&grp.expression), Doc::text(")")])
        }
        Expr::Range(range) => format_range(range),
        Expr::Ellipsis(_) => Doc::text("..."),
        Expr::Constraint(c) => format_expr(&c.expression),
        Expr::Subscript(s) => format_expr(&s.value),
        Expr::Tuple(t) => {
            let members: Vec<Doc> = t.members.iter().map(format_expr).collect();
            let content = Doc::join(Doc::concat([Doc::text(","), Doc::line()]), members);
            Doc::concat([Doc::text("("), Doc::group(content), Doc::text(")")])
        }
    }
}

fn format_literal(lit: &Literal) -> Doc {
    match lit {
        Literal::String(s) => Doc::text(format!("\"{}\"", escape_string(&s.value))),
        Literal::Integer(i) => Doc::text(i.value.to_string()),
        Literal::Float(f) => Doc::text(format_float(f.value)),
        Literal::Boolean(b) => Doc::text(if b.value { "true" } else { "false" }),
        Literal::Null(_) => Doc::text("null"),
    }
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
    // Format without trailing zeros, but ensure at least one decimal place
    let s = format!("{}", value);
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

fn format_dot(dot: &DotOperator) -> Doc {
    let lhs = format_expr(&dot.lhs);
    let rhs = format_expr(&dot.rhs);

    // After dereference (->), don't add extra dot
    if let Expr::Postfix(post) = dot.lhs.as_ref()
        && post.operator == Token::Arrow
    {
        return Doc::concat([lhs, rhs]);
    }

    Doc::concat([lhs, Doc::text("."), rhs])
}

fn format_binary(bin: &BinaryOperator) -> Doc {
    let op = bin.operator.literal();
    let left = format_expr(&bin.lhs);
    let right = format_expr(&bin.rhs);

    // For logical operators, allow line breaks with indentation
    if bin.operator == Token::And || bin.operator == Token::Or {
        return Doc::group(Doc::concat([
            left,
            Doc::nest(
                2,
                Doc::concat([Doc::line(), Doc::text(format!("{} ", op)), right]),
            ),
        ]));
    }

    // For colon (object key-value), use ": " format
    if bin.operator == Token::Colon {
        return Doc::concat([left, Doc::text(": "), right]);
    }

    Doc::concat([left, Doc::text(format!(" {} ", op)), right])
}

fn format_prefix(prefix: &PrefixOperator) -> Doc {
    let op = prefix.operator.literal();
    let operand = format_expr(&prefix.rhs);
    Doc::concat([Doc::text(op), operand])
}

fn format_postfix(postfix: &PostfixOperator) -> Doc {
    let operand = format_expr(&postfix.lhs);
    let op = postfix.operator.literal();

    // Add space before asc/desc
    let op_text =
        if postfix.operator == Token::AscOperator || postfix.operator == Token::DescOperator {
            format!(" {}", op)
        } else {
            op.to_string()
        };

    Doc::concat([operand, Doc::text(op_text)])
}

fn format_function_call(func: &FunctionCall) -> Doc {
    let name = if func.namespace.is_empty() {
        func.name.clone()
    } else {
        format!("{}::{}", func.namespace, func.name)
    };

    if func.arguments.is_empty() {
        return Doc::text(format!("{}()", name));
    }

    let args: Vec<Doc> = func.arguments.iter().map(format_expr).collect();
    let arg_list = Doc::join(Doc::concat([Doc::text(","), Doc::line()]), args);

    Doc::group(Doc::concat([
        Doc::text(format!("{}(", name)),
        Doc::nest(2, Doc::concat([Doc::line_or_empty(), arg_list])),
        Doc::line_or_empty(),
        Doc::text(")"),
    ]))
}

fn format_array(arr: &Array) -> Doc {
    if arr.expressions.is_empty() {
        return Doc::text("[]");
    }

    let elems: Vec<Doc> = arr.expressions.iter().map(format_expr).collect();
    let content = Doc::join(Doc::concat([Doc::text(","), Doc::line()]), elems);

    Doc::concat([
        Doc::text("["),
        Doc::group(Doc::nest(2, Doc::concat([Doc::line_or_empty(), content]))),
        Doc::text("]"),
    ])
}

fn format_object(obj: &Object) -> Doc {
    if obj.expressions.is_empty() {
        return Doc::text("{}");
    }

    let fields: Vec<Doc> = obj.expressions.iter().map(format_object_field).collect();
    let content = Doc::join(Doc::concat([Doc::text(","), Doc::line()]), fields);

    Doc::group(Doc::concat([
        Doc::text("{"),
        Doc::nest(2, Doc::concat([Doc::line(), content])),
        Doc::line(),
        Doc::text("}"),
    ]))
}

fn format_object_field(expr: &Expr) -> Doc {
    match expr {
        Expr::Binary(bin) if bin.operator == Token::Colon => {
            let key = format_expr(&bin.lhs);
            let value = format_expr(&bin.rhs);
            Doc::concat([key, Doc::text(": "), value])
        }
        Expr::Attribute(attr) => Doc::text(&attr.name),
        Expr::Ellipsis(_) => Doc::text("..."),
        _ => format_expr(expr),
    }
}

fn format_range(range: &Range) -> Doc {
    let start = format_expr(&range.start);
    let end = format_expr(&range.end);

    let op = if range.inclusive { ".." } else { "..." };

    Doc::concat([start, Doc::text(op), end])
}
