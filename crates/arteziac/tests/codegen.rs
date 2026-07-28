use arteziac::{analysis::analyze, codegen::Codegen, lexer::lex, lower::lower, parser::Parser};
use inkwell::context::Context;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    assert!(pdiags.is_empty(), "parse errors: {pdiags:?}");
    let (a, adiags) = analyze(&file, src);
    assert!(adiags.is_empty(), "analysis errors: {adiags:?}");
    let program = lower(&file, &a);

    let ctx = Context::create();
    let module = Codegen::new(&ctx, &a).run(&program);

    // LLVM verifier. A translation bug surfaces here as a readable message
    if let Err(e) = module.verify() {
        panic!("LLVM verification failed:\n{}", e.to_string());
    }

    module.print_to_string().to_string() // owned String before ctx drops
}

#[test]
fn cg_const_return() {
    insta::assert_snapshot!(check("func answer() -> Int { return 42; }"));
}

#[test]
fn cg_struct() {
    insta::assert_snapshot!(check("struct Foo { bar: Int } func main() -> Int { let f = Foo { bar: 5 }; return f.bar; }"));
}

#[test]
fn cg_struct_out_of_order() {
    insta::assert_snapshot!(check(
        "struct P { x: Int, y: Bool } func main() -> Int { let p = P { y: true, x: 1 }; return p.x; }"
    ));
}

#[test]
fn cg_field_assign() {
    insta::assert_snapshot!(check(
        "struct P { x: Int } func main() -> Int { let p = P { x: 1 }; p.x = 5; return p.x; }"
    ));
}

#[test]
fn cg_nested_field_assign() {
    insta::assert_snapshot!(check(
        "struct Inner { v: Int } struct Outer { inner: Inner }
         func main() -> Int { let o = Outer { inner: Inner { v: 1 } }; o.inner.v = 9; return o.inner.v; }"
    ));
}

#[test]
fn cg_methods() {
    insta::assert_snapshot!(check(
        "struct C { n: Int }
         func C.new() -> C { return C { n: 0 }; }
         func C.get(self) -> Int { return self.n; }
         func C.inc(mut self) { self.n = self.n + 1; }
         func main() -> Int { let c = C.new(); c.inc(); return c.get(); }"
    ))
}