use std::{collections::{HashMap, HashSet}, path::{Path, PathBuf}};

use artezia_diag::{Diagnostic, Severity, Span, SourceMap};

use crate::{ast, lexer::{Token, lex}, parser::Parser};

pub struct LoadResult {
    pub map: SourceMap,
    pub file: ast::File, // All items from all files merged
    pub diags: Vec<Diagnostic>
}

pub fn load(entry: &Path, stdlib_root: &Path) -> LoadResult {
    let mut map = SourceMap::new();
    let mut diags = Vec::new();
    let mut items: Vec<ast::Item> = Vec::new();
    let mut loaded: HashSet<PathBuf> = HashSet::new();
    let mut next_id: u32 = 0;

    let mut queue: Vec<(PathBuf, Option<Span>)> = vec![(entry.to_path_buf(), None)];

    while let Some((path, from_span)) = queue.pop() {
        let canon =  match path.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                let span = from_span.unwrap_or(0 .. 0);
                diags.push(Diagnostic::new(
                    Severity::Error,
                    span,
                    format!("cannot find module file `{}`", path.display())
                ));

                continue;
            }
        };

        // Handles cycles and diamond imports
        if !loaded.insert(canon.clone()) { continue; }

        let text = match std::fs::read_to_string(&canon) {
            Ok(t) => t,
            Err(e) => {
                let span = from_span.unwrap_or(0 .. 0);
                diags.push(Diagnostic::new(
                    Severity::Error,
                    span,
                    format!("cannot read `{}`: {e}", canon.display())
                ));
                
                continue;
            }
        };

        let fid = map.add(canon.clone(), &text);
        let base = map.base_of(fid);

        let tokens: Vec<(Token, Span)> = lex(&text)
            .into_iter()
            .map(|(t, s)| (t, s.start + base .. s.end + base))
            .collect();

        let mut parser = Parser::with_id_base(tokens, next_id);
        let (parsed, pdiags) = parser.parse_file();
        next_id = parser.next_node_id();
        diags.extend(pdiags);

        // Queue this file's imports
        let dir = canon.parent().unwrap_or(Path::new(".")).to_path_buf();
        for item in &parsed.items {
            if let ast::Item::Import(imp) = item {
                let target = import_to_path(imp, &map.text, &dir, stdlib_root);
                queue.push((target, Some(imp.span.clone())));
            }
        }

        items.extend(parsed.items);
    }

    LoadResult {
        map,
        file: ast::File { items },
        diags
    }
}

/// `import std::io` -> <stdlib_root>/std/io.tia
/// `import util` -> <import_dir>/util.tia
fn import_to_path(imp: &ast::Import, src: &str, dir: &Path, stdlib_root: &Path) -> PathBuf {
    let segs: Vec<&str> = imp.path.iter().map(|s| &src[s.clone()]).collect();

    let mut rel = PathBuf::new();
    for s in &segs {
        rel.push(s);
    }

    rel.set_extension("tia");

    if segs.first() == Some(&"std") {
        stdlib_root.join(rel.strip_prefix("std").unwrap_or(&rel))
    } else {
        dir.join(rel)
    }
}