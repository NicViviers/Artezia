use arteziac::{lexer::lex, parser::Parser};
use insta::assert_snapshot;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, diags) = Parser::new(tokens).parse_file();
    format!("{file:#?}\n--- diagnostics ---\n{diags:#?}")
}

#[test]
fn precedence() {
    assert_snapshot!(check("func f() { let x = 1 + 2 * 3 ** 2 ** 2 }"));
}

#[test]
fn postfix_chain() {
    assert_snapshot!(check("func f() { a.b(1).c[0].d() }"));
}

#[test]
fn newline_cont() {
    assert_snapshot!(check("func f() {\n let a = 1 +\n 2\n let b = a\n}"));
}

#[test]
fn concurrency() {
    assert_snapshot!(check("func f() { scope { spawn work(1) } }"));
}

#[test]
fn recovery() {
    assert_snapshot!(check("func f() {\n let = 5\n let ok = 1\n}"));
}