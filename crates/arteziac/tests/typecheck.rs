use arteziac::analysis::analyze;
use arteziac::parser::Parser;
use arteziac::lexer::lex;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    assert!(pdiags.is_empty(), "parse errors: {pdiags:?}");

    let (_a, adiags) = analyze(&file, src);

    // render diagnostics to a stable snapshot string
    if adiags.is_empty() {
        "no diagnostics".to_string()
    } else {
        adiags.iter()
            .map(|d| format!("{}..{}: {}", d.span.start, d.span.end, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[test]
fn struct_construction_types() {
    // Foo { bar: 5 } has type Foo; the 5 checks against bar: Int
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int } func main() { let f = Foo { bar: 5 }; }"
    ));
}

#[test]
fn struct_field_access_types() {
    // f.bar has type Int
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int } func main() -> Int { let f = Foo { bar: 5 }; return f.bar; }"
    ));
}

#[test]
fn struct_multi_field() {
    insta::assert_snapshot!(check(
        "struct P { x: Int, y: Int } func main() -> Int { let p = P { x: 1, y: 2 }; return p.y; }"
    ));
}

#[test]
fn struct_out_of_order_construction() {
    // Declared x, y; constructed y-first. Must typecheck (fields matched by name)
    // This is the structlit_order test - the reordering must be correct
    insta::assert_snapshot!(check(
        "struct P { x: Int, y: Bool } func main() { let p = P { y: true, x: 1 }; }"
    ));
}

#[test]
fn struct_typed_field() {
    // A struct field whose type is another struct - proves T1 resolves
    // struct-typed fields, and chained field access types correctly
    insta::assert_snapshot!(check(
        "struct Inner { v: Int } struct Outer { inner: Inner } \
         func main() -> Int { let o = Outer { inner: Inner { v: 7 } }; return o.inner.v; }"
    ));
}

#[test]
fn struct_field_type_mismatch() {
    // bar: Int but constructed with a Bool
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int } func main() { let f = Foo { bar: true }; }"
    ));
}

#[test]
fn struct_unknown_field_in_construction() {
    // Foo has no field `baz`
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int } func main() { let f = Foo { baz: 5 }; }"
    ));
}

#[test]
fn struct_unknown_field_access() {
    // f.baz - no such field
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int } func main() { let f = Foo { bar: 5 }; let x = f.baz; }"
    ));
}

#[test]
fn field_access_on_non_struct() {
    // 5.foo - field access on an Int
    insta::assert_snapshot!(check(
        "func main() { let x = 5; let y = x.foo; }"
    ));
}

#[test]
fn unknown_struct_in_construction() {
    // Nonexistent { ... } - no such struct
    insta::assert_snapshot!(check(
        "func main() { let f = Nonexistent { bar: 5 }; }"
    ));
}

#[test]
fn construction_missing_field() {
    // Foo needs bar but it's not provided. Do we check for missing fields? If not, this may currently pass silently (a gap worth knowing about)
    insta::assert_snapshot!(check(
        "struct Foo { bar: Int, baz: Int } func main() { let f = Foo { bar: 5 }; }"
    ));
}

#[test]
fn method_declares_cleanly() {
    // Body deliberately doesn't touch `self` - tests declaration + table population work
    insta::assert_snapshot!(check(
        "struct Foo { n: Int } func Foo.get(self) -> Int { return 0; }"
    ));
}

#[test]
fn method_on_unknown_type() {
    insta::assert_snapshot!(check(
        "func Ghost.get(self) -> Int { return 0; }"
    ));
}

#[test]
fn self_on_plain_function() {
    insta::assert_snapshot!(check(
        "func plain(self) -> Int { return 0; }"
    ));
}

#[test]
fn struct_field_access() {
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.get(self) -> Int { return self.n; }"
    ))
}

#[test]
fn struct_field_mutate() {
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.inc(mut self) { self.n = self.n + 1; }"
    ))
}

#[test]
fn mut_self_on_nonlocal_receiver() {
    // Should produce a diagnostic
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.new() -> C { return C { n: 0 }; }
         func C.inc(mut self) { self.n = self.n + 1 } func main() { C.new().inc(); }"
    ))
}

#[test]
fn mut_self_on_local_receiver() {
    // Should not produce a diagnostic since `C.new()` is assigned to a local
    insta::assert_snapshot!(check(
        "struct C { n: Int } func C.new() -> C { return C { n: 0 }; }
         func C.inc(mut self) { self.n = self.n + 1 } func main() { let c = C.new(); c.inc(); }"
    ))
}

#[test]
fn unknown_method() {
    insta::assert_snapshot!(check(
        "struct C { n: Int } func main() { let c = C { n: 0 }; c.foo(); }"
    ))
}