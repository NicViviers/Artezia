use arteziac::analysis::Analysis;
use artezia_diag::Diagnostic;
use arteziac::lexer::lex;
use arteziac::parser::Parser;
use arteziac::passes::resolve;

fn check(src: &str) -> String {
    let tokens = lex(src);
    let (file, parse_diags) = Parser::new(tokens).parse_file();

    let mut a = Analysis::new();
    let mut diags = Vec::new();
    resolve::resolve(&file, src, &mut a, &mut diags);

    dump_resolution(&a, src, &parse_diags, &diags)
}

fn dump_resolution(
    a: &Analysis,
    src: &str,
    parse_diags: &[Diagnostic],
    diags: &[Diagnostic],
) -> String {
    let mut out = String::new();

    for i in 0..a.definitions.len() {
        let info = a.definitions.info(arteziac::analysis::DefId(i as u32));
        out.push_str(&format!(
            "def {}: {:?} `{}` @ {}..{}\n",
            i,
            info.kind,
            a.symbols.resolve(info.name),
            info.name_span.start,
            info.name_span.end,
        ));
    }

    let mut uses: Vec<_> = a
        .defs
        .iter()
        .filter(|(node, def)| a.definitions.info(**def).node != **node)
        .collect();
    uses.sort_by_key(|(node, _)| node.0); // deterministic snapshot order

    for (node, def) in uses {
        let name = a.symbols.resolve(a.definitions.info(*def).name);
        out.push_str(&format!("use `{}` (node {}) -> def {}\n", name, node.0, def.0));
    }

    out.push_str(&format!("--- parse diagnostics: {}\n", parse_diags.len()));
    out.push_str("--- resolve diagnostics ---\n");
    for d in diags {
        out.push_str(&format!(
            "{:?} @ {}..{}: {}\n",
            d.severity, d.span.start, d.span.end, d.message
        ));
        // TODO: Might need to include labels/secondary spans if Diagnostic has them:
        // for (span, msg) in &d.secondary { ... }
    }

    let _ = src;
    out
}


/// pre-declaration - bodies may call functions defined LATER.
/// Expect: zero diagnostics; `b`'s use links to def created in phase 1.
#[test]
fn use_before_definition() {
    insta::assert_snapshot!(check(
        "func a() { b() }\nfunc b() { }"
    ));
}

/// inner scope may redefine an outer name; uses inside the inner scope see the inner def,
/// and the shadow's own initializer sees the OUTER def (init-before-declare)
/// Expect: two `x` defs; the inner init's use links to def of OUTER x; zero diagnostics
#[test]
fn shadowing_inner_init_sees_outer() {
    insta::assert_snapshot!(check(
        "func f() {\n let x = 1\n if true {\n let x = x\n }\n}"
    ));
}

/// init-before-declare with NO outer binding = error, not self-reference.
/// Expect: exactly one "cannot find `x`" diagnostic; x still gets declared
/// (later uses of x resolve fine — add one to prove it)
#[test]
fn let_x_equals_x_no_outer() {
    insta::assert_snapshot!(check(
        "func f() {\n let x = x\n let y = x\n}"
    ));
}

/// undefined variable -> exactly one diagnostic, no defs entry.
#[test]
fn undefined_variable() {
    insta::assert_snapshot!(check(
        "func f() {\n let a = missing\n}"
    ));
}

/// duplicate in the SAME scope -> one diagnostic pointing both ways
/// (primary on the redefinition, secondary/label on the first)
/// Also pins: the FIRST def stays usable (the later use links to def of the first `x`... or the first, per declare()'s return — snapshot decides)
#[test]
fn duplicate_same_scope() {
    insta::assert_snapshot!(check(
        "func f() {\n let x = 1\n let x = 2\n let y = x\n}"
    ));
}

/// params are definitions in the function scope; body uses resolve to them
/// Also pins your body-scope decision: `let a` in the body shadowing param `a` is ALLOWED (two defs, no diagnostic)
///     if you chose the other rule, this snapshot shows a duplicate error instead. Either way it's pinned
#[test]
fn params_and_body_shadowing() {
    insta::assert_snapshot!(check(
        "func f(a: Int) {\n let a = a\n}"
    ));
}

///  block scoping - a block-local is invisible after the block
#[test]
fn block_local_invisible_after() {
    insta::assert_snapshot!(check(
        "func f() {\n if true {\n let t = 1\n }\n let u = t\n}"
    ));
}

/// while bodies scope themselves (via resolve_block)
#[test]
fn while_body_scopes() {
    insta::assert_snapshot!(check(
        "func f() {\n while true {\n let t = 1\n }\n let u = t\n}"
    ));
}

/// for-loop variable is scoped to the body, invisible after; and the iter expression cannot see the loop variable
#[test]
fn for_var_scoping() {
    insta::assert_snapshot!(check(
        "func f() {\n for i in 0 .. i {\n let a = i\n }\n let b = i\n}"
    ));
    // expect: "cannot find `i`" TWICE - once in the iter (0 .. i),
    // once after the loop - and the body use resolves fine
}

/// parser recovery holes produce zeri resolution diagnostics - the parser already generates a diagnostic
/// Expect: parse diagnostics: 1, resolve diagnostics: bibe, and no phantom def for the missing name
#[test]
fn poison_zero_width_names() {
    insta::assert_snapshot!(check(
        "func f() {\n let = 5\n let ok = 1\n}"
    ));
}

/// import introduces its last path segment (v0 behavior).
#[test]
fn import_introduces_name() {
    insta::assert_snapshot!(check(
        "import std::io\nfunc f() { io() }"
    ));
}