use arteziac::{lexer::lex, parser::Parser};
use insta::assert_snapshot;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, diags) = Parser::new(tokens).parse_file();
    format!("{file:#?}\n--- diagnostics ---\n{diags:#?}")
}

#[test]
fn precedence() {
    assert_snapshot!(check("func f() { let x = 1 + 2 * 3 ** 2 ** 2; }"));
}

#[test]
fn postfix_chain() {
    assert_snapshot!(check("func f() { a.b(1).c[0].d(); }"));
}

#[test]
fn newline_cont() {
    assert_snapshot!(check("func f() { let a = 1 +\n 2;\n let b = a; }"));
}

#[test]
fn concurrency() {
    assert_snapshot!(check("func f() { scope { spawn work(1); } }"));
}

#[test]
fn recovery() {
    assert_snapshot!(check("func f() { let = 5;\n let ok = 1; }"));
}

#[test]
fn ranges() {
    insta::assert_snapshot!(check("func f() { let r = 0 .. 10; }"));
}
#[test]
fn range_precedence() {
    insta::assert_snapshot!(check("func f() { let r = 0 .. n + 1; }"));
    // must nest as Range(0, Add(n, 1)) 0 additive (11) binds tighter than range (9)
}

#[test]
fn empty_char_diagnoses() {
    insta::assert_snapshot!(check("func f() { let d = ''; }")) ;// Should produce 1 diagnostic
}

#[test]
fn stray_garbage_recovers() {
    insta::assert_snapshot!(check("func f() { let x = §; }")); // Should produce 1 diagnostic
}

#[test]
fn struct_decl() {
    insta::assert_snapshot!(check("struct Foo { a: Int }"));
}

#[test]
fn struct_literal() {
    insta::assert_snapshot!(check("struct Foo { bar: Bool } func main() { let x = Foo { bar: true }; }"));
}

#[test]
fn struct_abiguity() {
    // Check while still parses with empty body instead of mistaking for struct literal
    insta::assert_snapshot!(check("func main() { while c { } }"));
}

#[test]
fn struct_literal_in_loop() {
    insta::assert_snapshot!(check("struct Foo { bar: Bool } func main() { while true { let x = Foo { bar: false }; } }"));
}

#[test]
fn struct_complex() {
    // TODO: Consider where parse_struct should just eat trailing commas? For now it produces diagnostics on it expecting a new field
    insta::assert_snapshot!(check("struct Multi { lhs: Int, rhs: Int } struct Trailing { foo: Bool,, } struct Empty {  }"));
}

// TODO: Run tests for typecheck.rs and consider above comment