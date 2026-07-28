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
    insta::assert_snapshot!(check("func main() { let a = 1; let b = 5.5; let c = \"\"; let d = 'a'; let e = 10ms; let f = true; }"));
}

// TODO: Finish checking this test after implementing Return in lower_stmt
#[test]
fn lower_params() {
    insta::assert_snapshot!(check("func add(a: Int, b: Int) -> Int {\n let c = 1; return c;\n}"));
}

#[test]
fn lower_binary_precedence() {
    insta::assert_snapshot!(check("func main() { let x = 1 + 2 * 3; }"));
}

#[test]
fn lower_unary_neg() {
    insta::assert_snapshot!(check("func main() { let x = -5; }"));
}

#[test]
fn lower_unary_not() {
    // TODO: Panics because "not true" isn't being seen as an expression here so parser expects an expr and semi-colon
    // thread 'lower_unary_not' (118065) panicked at crates/arteziac/tests/lower.rs:10:5:
    //     parse errors: [Diagnostic { severity: Error, span: 21..24, message: "expected an expression, found `not`", label: None, notes: [], code: None },
    //     Diagnostic { severity: Error, span: 21..21, message: "expected a semi-colon, found `not`", label: None,
    //     notes: ["while parsing `;` after a statement"], code: None },
    //     Diagnostic { severity: Error, span: 21..24, message: "expected an expression, found `not`", label: None, notes: [], code: None },
    //     Diagnostic { severity: Error, span: 21..24, message: "expected `;` or `}`, found `not`", label: None, notes: [], code: None }]
    insta::assert_snapshot!(check("func main() { let b = not true; }"));
}

#[test]
fn lower_unary_in_binary() {
    insta::assert_snapshot!(check("func main(a: Int, b: Int) { let x = -a + b; }"));
}

#[test]
fn lower_comparison() {
    insta::assert_snapshot!(check("func main(a: Int, b: Int) { let c = a < b; }"));
}

#[test]
fn lower_call() {
    insta::assert_snapshot!(check("func f(a: Int) {}  func main() { f(1 + 2); }"));
}

#[test]
fn if_statement_unit() {
    // If with no else, no value - diamond with else_bb == join_bb, ConstUnit
    insta::assert_snapshot!(check(
        "func main(c: Bool) { if c { let x = 1; } }"
    ));
}

#[test]
fn if_else_statement() {
    // Three real blocks: then, else, join
    insta::assert_snapshot!(check(
        "func main(c: Bool) { if c { let x = 1; } else { let y = 2; } }"
    ));
}

#[test]
fn if_as_value() {
    // Result slot, both arms store it, join loads it
    // Confirm the slot appears in the locals header
    insta::assert_snapshot!(check(
        "func main(c: Bool) { let x = if c { 1; } else { 2; }; }"
    ));
}

#[test]
fn if_else_if_chain() {
    // else-branch is itself an If expr
    insta::assert_snapshot!(check(
        "func main(a: Bool, b: Bool) { if a { let x = 1; } else if b { let y = 2; } else { let z = 3; } }"
    ));
}

#[test]
fn while_basic() {
    // jump -> cond, cond branches body/exit, body jumps back to cond
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { let x = 1; } }"
    ));
}

#[test]
fn while_empty_body() {
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { ; } }"
    ));
}

#[test]
fn while_break() {
    // Body jumps to exit_bb; dead-code orphan block after break
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { break; } }"
    ));
}

#[test]
fn while_continue() {
    // Body jumps to cond_bb
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { continue; } }"
    ));
}

#[test]
fn while_break_then_dead_code() {
    // `let x = 1` after break lands in an unreachable block
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { break; let x = 1; } }"
    ));
}

#[test]
fn nested_while_break_targets_inner() {
    // Inner break targets the inner loop's exit
    insta::assert_snapshot!(check(
        "func main(c: Bool, d: Bool) { while c { while d { break; } } }"
    ));
}

#[test]
fn short_circuit_and() {
    // g()'s call must be inside rhs_bb, not the entry block - that is the short-circuit
    insta::assert_snapshot!(check(
        "func f() -> Bool { return true; }  func g() -> Bool { return false; }  func main() { let x = f() and g(); }"
    ));
}

#[test]
fn short_circuit_or() {
    insta::assert_snapshot!(check(
        "func f() -> Bool { return true; }  func g() -> Bool { return false; }  func main() { let x = f() or g(); }"
    ));
}

#[test]
fn short_circuit_nested() {
    // a and b and c - left-assoc chain
    insta::assert_snapshot!(check(
        "func main(a: Bool, b: Bool, c: Bool) { let x = a and b and c; }"
    ));
}

#[test]
fn for_basic() {
    // Full counter shape: init i = lo, hi -> temp, cond(i < hi), body, incr(i + 1)->cond
    insta::assert_snapshot!(check(
        "func main() { for i in 0 .. 10 { let x = 1; } }"
    ));
}

#[test]
fn for_uses_loop_var() {
    insta::assert_snapshot!(check(
        "func main(n: Int) { for i in 0 .. n {\n let x = i;\n } }"
    ));
}

#[test]
fn for_inclusive() {
    // ..= must produce cmp.le in the cond block, not cmp.lt
    insta::assert_snapshot!(check(
        "func main() { for i in 0 ..= 10 { ; } }"
    ));
}

#[test]
fn for_continue_advances() {
    // continue must jump to incr_bb (which does i + 1), not cond - else infinite
    insta::assert_snapshot!(check(
        "func main() { for i in 0 .. 10 { continue; } }"
    ));
}

#[test]
fn for_hi_evaluated_once() {
    // f()'s call must be in the entry block (evaluated once), not in cond (which would re-run it every iteration)
    insta::assert_snapshot!(check(
        "func f() -> Int { return 10; }  func main() { for i in 0 .. f() { ; } }"
    ));
}

#[test]
fn while_containing_if_break() {
    // If's inside while's body; break targets while's exit through the if
    insta::assert_snapshot!(check(
        "func main(c: Bool, d: Bool) { while c { if d { break; } } }"
    ));
}

#[test]
fn valued_if_in_loop() {
    // Result slot + loop interaction
    insta::assert_snapshot!(check(
        "func main(c: Bool) { while c { let x = if c { 1 } else { 2 }; } }"
    ));
}

#[test]
fn for_containing_while() {
    insta::assert_snapshot!(check(
        "func main(d: Bool) { for i in 0 .. 10 { while d { break; } } }"
    ));
}

#[test]
fn struct_new() {
    insta::assert_snapshot!(check(
        "struct Foo { bar: Bool }"
    ));
}

#[test]
fn struct_out_of_order_lit() {
    insta::assert_snapshot!(check(
        "struct Foo { b: Int, a: Int }" // Should preserve order
    ));
}

#[test]
fn struct_field_access() {
    insta::assert_snapshot!(check(
        "struct Foo { bar: Bool } func main() -> Bool { let x = Foo { bar: true }; return x.bar; }"
    ))
}

#[test]
fn struct_nested_field_access() {
    insta::assert_snapshot!(check(
        "struct Inner { foo: Bool } struct Outer { bar: Inner } func main() { let inner = Inner { foo: true }; let outer = Outer { bar: inner }; }"
    ))
}

#[test]
fn struct_out_of_order_construction() {
    insta::assert_snapshot!(check(
        "struct P { x: Int, y: Bool } func main() { let p = P { y: true, x: 1 }; }"
    ));
}

#[test]
fn lower_method_byvalue_self() {
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.get(self) -> Int { return self.n; }"
    ))
}

#[test]
fn lower_method_mut_self() {
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.inc(mut self) { self.n = self.n + 1; }"
    ))
}