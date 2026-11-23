//! Minimal inputs found by `tests/fuzz.rs`, checked against the invariants in
//! `common` at the width that exposed each bug and at a few others.

mod common;

const CASES: &[(&str, usize, bool)] = &[
    // A comment after `=` moved behind the body on the second pass.
    ("fn x::y($p) = // hf\n $p{a};*", 80, false),
    // A comment inside a subscript drifted out of the brackets.
    ("*[0 // \n]==*", 80, false),
    // Two comments in a row were merged onto one line.
    ("*[\"post\" // \n] // trailing", 80, false),
    ("[a, // one\n// two\nb]", 80, false),
    // Large integers re-parse as floats and cannot take `.name`.
    ("99999999999999999999[\"post\"]", 80, false),
    ("1.5[\"a\"]", 80, false),
    ("_typein1.5[\"post\"]", 80, false),
    // Calls hugging their last argument.
    ("count([*])==^.^match releaseDate", 10, false),
    (
        "array::unique(*[_type == \"post\"].category) | order(@ asc)",
        30,
        false,
    ),
    // A value moved after `=` because only there it fits.
    ("fn _type::_type($p) =!*[*];*", 10, false),
    // A broken `=>` condition left the value's closing brace misaligned.
    (
        "*[_type == \"project\"][0].members[]{@->salary > 90000 => @->{name, salary}}",
        0,
        false,
    ),
    (
        "*{a == 1 && b == 2 && count(*[c]) > 0 => {d, e}}",
        20,
        false,
    ),
    ("select(*== // \n* =>count(current,^.^), *)", 0, false),
    // Comments before `|` moved behind it.
    ("* // \n| order(*)", 0, false),
    // Comments between operands were ignored when force-wrapping.
    ("fn _type::_type($p) =* // \n==*| score(*);*", 0, true),
    // Brackets after a broken binary expression did not line up.
    ("*==*==*[true]", 0, false),
    ("* // \n+0..0{ // \n}", 0, false),
    ("!@+*==*{ // \n}", 0, false),
    ("a == b | order(c)", 10, false),
    // Comments after a prefix operator, or inside a subscript.
    ("fn _type::_type($p) =* **+ // \n*;*", 0, false),
    ("*[$_type+ // \n0]", 0, false),
    (
        "fn _type::_type($p) =0..0+*==select(* =>*, releaseDate =>*, *);*",
        40,
        false,
    ),
    // A comment earlier on the line let a `=>` condition break.
    (
        "select(*[ // \n$_type] =>*, *==^.^ =>count(*,*), *)",
        0,
        false,
    ),
    ("_typein // \n[] // \n{}&&*", 0, false),
    // A projection in the middle of a pipeline was printed as its own chain.
    (
        "*|score(*|boost(*, *)|{current,current[]->{name},_a_00AaAa0aaA0}|order(_score desc)[10...10000]{})",
        68,
        false,
    ),
    // A suffix after a pipeline stage (`->[...]`) was not indented with it.
    (
        "*|boost($_type, *)|order(current asc)|boost(*, _ref)->[*{\"post\":*,\"post\":aAa0a0__AA000a}]",
        0,
        false,
    ),
    // A hugged last argument joins `)` and what follows onto its last line.
    ("*|score(*|{_type}[0...$_type * 1])[0...0]", 13, false),
    ("score(*{_type}[0...$_type * 1])[0...0]", 13, false),
];

#[test]
fn fuzz_regressions() {
    for &(query, width, force_wrap) in CASES {
        for w in [width, 0, 20, 40, 80] {
            common::check(query, w, force_wrap);
            common::check(query, w, !force_wrap);
        }
    }
}
