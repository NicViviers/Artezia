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