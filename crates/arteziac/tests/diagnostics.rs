use artezia_diag::*;
use arteziac::{lexer::lex, parser::Parser, analysis::analyze, ast};

fn render_diags(src: &str) -> String {
    let mut map = SourceMap::new();
    map.add("test.tia".into(), src);

    let tokens = lex(src);
    let (file, pdiags) = Parser::new(tokens).parse_file();
    if !pdiags.is_empty() {
        return render_to_string(&pdiags, &map, true);
    }

    let (_a, adiags) = analyze(&file, &map.text);
    render_to_string(&adiags, &map, true)
}

/// Compile several in-memory "files" and render the diagnostics
fn render_diags_multi(files: &[(&str, &str)]) -> String {
    let mut map = SourceMap::new();
    let mut items = Vec::new();
    let mut diags = Vec::new();
    let mut next_id = 0u32;

    for (name, text) in files {
        let fid = map.add(name.into(), text);
        let base = map.base_of(fid);
        let tokens: Vec<_> = lex(text)
            .into_iter()
            .map(|(t, s)| (t, s.start + base..s.end + base))
            .collect();
        let mut p = Parser::with_id_base(tokens, next_id);
        let (parsed, pdiags) = p.parse_file();
        next_id = p.next_node_id();
        diags.extend(pdiags);
        items.extend(parsed.items);
    }

    if diags.is_empty() {
        let merged = ast::File { items };
        let (_a, adiags) = analyze(&merged, &map.text);
        diags.extend(adiags);
    }
    render_to_string(&diags, &map, true)
}

#[test]
fn localize_translates_to_file_relative() {
    let mut m = SourceMap::new();
    m.add("a.tia".into(), "func f() {}");
    let b = m.add("b.tia".into(), "func g() {}");
    let base = m.base_of(b);

    // A span pointing at `g` in file b
    let global = (base + 5)..(base + 6);
    let (name, local) = localize(&m, &global);
    assert_eq!(name, "b.tia");
    assert_eq!(local, 5..6);
}

#[test]
fn renders_type_mismatch() {
    // Build a SourceMap with one file, run the pipeline, render the diags
    insta::assert_snapshot!(render_diags(
        "struct Foo { bar: Int } func main() { let f = Foo { bar: true }; }"
    ));
}

#[test]
fn renders_error_in_second_file() {
    insta::assert_snapshot!(render_diags_multi(&[
        ("a.tia", "func helper() -> Int { return 1; }"),
        ("b.tia", "func main() -> Int { return true; }"), // Bool vs Int
    ]));
}

#[test]
fn renders_duplicate_definition_across_files() {
    insta::assert_snapshot!(render_diags_multi(&[
        ("a.tia", "func helper() -> Int { return 1; }"),
        ("b.tia", "func helper() -> Int { return 2; }"),
    ]));
}