use arteziac::analysis::analyze;
use arteziac::lower::lower;
use arteziac::parser::Parser;
use arteziac::lexer::lex;
use arteziac::tir::dump_program;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    assert!(pdiags.is_empty(), "parse errors: {pdiags:?}");

    let (a, adiags) = analyze(&file, src);
    assert!(adiags.is_empty(), "analysis errors: {adiags:?}");

    let program = lower(&file, &a);
    dump_program(&program, &a)
}

#[test]
fn lower_constants_and_lets() {
    insta::assert_snapshot!(check("func main() {\n let a = 1\n let b = 5.5\n let c = \"\"\n let d = 'a'\n let e = 10ms\n let f = true\n}"));
}

#[test]
fn lower_params() {
    insta::assert_snapshot!(check("func add(a: Int, b: Int) -> Int {\n let c = 1\n}"));
}

// TODO: Test these after completing lowering
#[test]
fn lower_binary_precedence() {
    insta::assert_snapshot!(check("func main() {\n let x = 1 + 2 * 3\n}"));
}

#[test]
fn lower_unary_neg() {
    insta::assert_snapshot!(check("func main() {\n let x = -5\n}"));
}

#[test]
fn lower_unary_not() {
    insta::assert_snapshot!(check("func main() {\n let b = not true\n}"));
}

#[test]
fn lower_unary_in_binary() {
    insta::assert_snapshot!(check("func main(a: Int, b: Int) {\n let x = -a + b\n}"));
}

#[test]
fn lower_comparison() {
    insta::assert_snapshot!(check("func main(a: Int, b: Int) {\n let c = a < b\n}"));
}

#[test]
fn lower_call() {
    insta::assert_snapshot!(check("func f(a: Int) {}  func main() { f(1 + 2) }"));
}

#[test]
fn if_statement_unit() {
    // If with no else, no value - diamond with else_bb == join_bb, ConstUnit
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n if c {\n let x = 1\n }\n}"
    ));
}

#[test]
fn if_else_statement() {
    // Three real blocks: then, else, join
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n if c {\n let x = 1\n } else {\n let y = 2\n }\n}"
    ));
}

#[test]
fn if_as_value() {
    // Result slot, both arms store it, join loads it
    // Confirm the slot appears in the locals header
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n let x = if c { 1 } else { 2 }\n}"
    ));
}

#[test]
fn if_else_if_chain() {
    // else-branch is itself an If expr
    insta::assert_snapshot!(check(
        "func main(a: Bool, b: Bool) {\n if a {\n let x = 1\n } else if b {\n let y = 2\n } else {\n let z = 3\n }\n}"
    ));
}

#[test]
fn while_basic() {
    // jump -> cond, cond branches body/exit, body jumps back to cond
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n let x = 1\n }\n}"
    ));
}

#[test]
fn while_empty_body() {
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n }\n}"
    ));
}

#[test]
fn while_break() {
    // Body jumps to exit_bb; dead-code orphan block after break
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n break\n }\n}"
    ));
}

#[test]
fn while_continue() {
    // Body jumps to cond_bb
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n continue\n }\n}"
    ));
}

#[test]
fn while_break_then_dead_code() {
    // `let x = 1` after break lands in an unreachable block
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n break\n let x = 1\n }\n}"
    ));
}

#[test]
fn nested_while_break_targets_inner() {
    // Inner break targets the inner loop's exit
    insta::assert_snapshot!(check(
        "func main(c: Bool, d: Bool) {\n while c {\n while d {\n break\n }\n }\n}"
    ));
}

#[test]
fn short_circuit_and() {
    // g()'s call must be inside rhs_bb, not the entry block - that is the short-circuit
    insta::assert_snapshot!(check(
        "func f() -> Bool { return true }  func g() -> Bool { return false }  func main() {\n let x = f() and g()\n}"
    ));
}

#[test]
fn short_circuit_or() {
    insta::assert_snapshot!(check(
        "func f() -> Bool { return true }  func g() -> Bool { return false }  func main() {\n let x = f() or g()\n}"
    ));
}

#[test]
fn short_circuit_nested() {
    // a and b and c - left-assoc chain
    insta::assert_snapshot!(check(
        "func main(a: Bool, b: Bool, c: Bool) {\n let x = a and b and c\n}"
    ));
}

#[test]
fn for_basic() {
    // Full counter shape: init i = lo, hi -> temp, cond(i < hi), body, incr(i + 1)->cond
    insta::assert_snapshot!(check(
        "func main() {\n for i in 0 .. 10 {\n let x = 1\n }\n}"
    ));
}

#[test]
fn for_uses_loop_var() {
    insta::assert_snapshot!(check(
        "func main(n: Int) {\n for i in 0 .. n {\n let x = i\n }\n}"
    ));
}

#[test]
fn for_inclusive() {
    // ..= must produce cmp.le in the cond block, not cmp.lt
    insta::assert_snapshot!(check(
        "func main() {\n for i in 0 ..= 10 {\n }\n}"
    ));
}

#[test]
fn for_continue_advances() {
    // continue must jump to incr_bb (which does i + 1), not cond - else infinite
    insta::assert_snapshot!(check(
        "func main() {\n for i in 0 .. 10 {\n continue\n }\n}"
    ));
}

#[test]
fn for_hi_evaluated_once() {
    // f()'s call must be in the entry block (evaluated once), not in cond (which would re-run it every iteration)
    insta::assert_snapshot!(check(
        "func f() -> Int { return 10 }  func main() {\n for i in 0 .. f() {\n }\n}"
    ));
}

#[test]
fn while_containing_if_break() {
    // If's inside while's body; break targets while's exit through the if
    insta::assert_snapshot!(check(
        "func main(c: Bool, d: Bool) {\n while c {\n if d {\n break\n }\n }\n}"
    ));
}

#[test]
fn valued_if_in_loop() {
    // Result slot + loop interaction
    insta::assert_snapshot!(check(
        "func main(c: Bool) {\n while c {\n let x = if c { 1 } else { 2 }\n }\n}"
    ));
}

#[test]
fn for_containing_while() {
    insta::assert_snapshot!(check(
        "func main(d: Bool) {\n for i in 0 .. 10 {\n while d {\n break\n }\n }\n}"
    ));
}