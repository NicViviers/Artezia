use std::collections::HashMap;
use artezia_diag::{Diagnostic, Severity};
use crate::{analysis::{Analysis, DefId, DefInfo, DefKind, Symbol}, parser::Span};
use crate::ast;

struct Resolver<'a> {
    src: &'a str,
    a: &'a mut Analysis,
    diags: &'a mut Vec<Diagnostic>,
    scopes: Vec<HashMap<Symbol, DefId>>
}

impl Resolver<'_> {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn intern_span(&mut self, span: &Span) -> Symbol {
        self.a.symbols.intern(&self.src[span.clone()])
    }

    /// Innermost-first search
    fn lookup(&self, sym: Symbol) -> Option<DefId> {
        self.scopes.iter().rev().find_map(|s| s.get(&sym).copied())
    }

    fn declare(&mut self, name_span: &Span, kind: DefKind, node: ast::NodeId) -> Option<DefId> {
        if name_span.is_empty() {
            return None;
        }

        let sym = self.intern_span(name_span);

        // Duplicate in the same scope? (Shadowing an outer scope is fine since that's just lookup finding a nearer entry)
        if let Some(&prev) = self.scopes.last().unwrap().get(&sym) {
            let first = self.a.definitions.info(prev).name_span.clone();

            self.diags.push(Diagnostic::new(Severity::Error, name_span.clone(),
                format!("`{}` is already defined in this scope", self.a.symbols.resolve(sym)))
                .with_note(format!("first defined at {}..{}", first.start, first.end))); // TODO: Can this highlight the line where it's first defined?

            return Some(prev); // Keep the first usable
        }

        let def = self.a.definitions.create(DefInfo {
            kind,
            name: sym,
            node,
            name_span: name_span.clone()
        });
        self.scopes.last_mut().unwrap().insert(sym, def);
        self.a.defs.insert(node, def); // Defining node also maps to it's own DefId (typeck needs it)

        Some(def)
    }

    fn resolve_use(&mut self, node: ast::NodeId, name_span: &Span) {
        if name_span.is_empty() {
            return;
        }

        let sym = self.intern_span(name_span);

        // On failure, no defs entry
        // Not being in defs means later passes see it as already diagnosed
        match self.lookup(sym) {
            Some(def) => { self.a.defs.insert(node, def); }
            None => {
                self.diags.push(Diagnostic::new(
                    Severity::Error,
                    name_span.clone(),
                    format!("cannot find `{}` in this scope",  self.a.symbols.resolve(sym))
                ));
            }
        }
    }

    fn resolve_block(&mut self, b: &ast::Block) {
        self.push();

        for s in &b.stmts {
            self.resolve_stmt(s);
        }

        self.pop();
    }

    fn resolve_expr(&mut self, e: &ast::Expr) {
        match e {
            ast::Expr::Var { id, span } => self.resolve_use(*id, span),
            ast::Expr::Field { obj, .. } => self.resolve_expr(obj), // Only resolve the root object, nothing more since the rest depends on type
            ast::Expr::Unary { rhs, .. } => self.resolve_expr(rhs),
            ast::Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(lhs);
                self.resolve_expr(rhs);
            }

            ast::Expr::Assign { target, value, .. } => {
                self.resolve_expr(target);
                self.resolve_expr(value);
            }

            ast::Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee);

                for arg in args {
                    self.resolve_expr(&arg.value);
                }
            }

            ast::Expr::Index { obj, index, .. } => {
                self.resolve_expr(obj);
                self.resolve_expr(index);
            }

            ast::Expr::Range { lo, hi, .. } => {
                self.resolve_expr(lo);
                self.resolve_expr(hi);
            }

            ast::Expr::If { cond, then, els, .. } => {
                self.resolve_expr(cond);
                self.resolve_block(then);
                
                if let Some(e) = els {
                    self.resolve_expr(e);
                }
            }

            ast::Expr::Block(b) => self.resolve_block(b),
            ast::Expr::Scope { body, .. } => self.resolve_block(body),
            ast::Expr::Spawn { call, .. } => self.resolve_expr(call),
            ast::Expr::Within { dur, body, els, .. } => {
                self.resolve_expr(dur);
                self.resolve_block(body);
                
                if let Some(b) = els {
                    self.resolve_block(b);
                }
            }

            // Nothing to resolve so ignore
            ast::Expr::Int { .. }
            | ast::Expr::Float { .. }
            | ast::Expr::Str { .. }
            | ast::Expr::Char { .. }
            | ast::Expr::Duration { .. }
            | ast::Expr::Bool { .. }
            | ast::Expr::Error { .. } => {}
        }
    }

    fn resolve_stmt(&mut self, s: &ast::Stmt) {
        match s {
            ast::Stmt::Let { id, name_span, init, .. } => {
                self.resolve_expr(init); // Before declare due to `let x = x` needing to resolve `= x` first before the name `let x`
                self.declare(name_span, DefKind::Local, *id);
            }

            ast::Stmt::For { id, var_span, iter, body, .. } => {
                self.resolve_expr(iter); // Iter can't see the loop var
                self.push(); // Loop var scoped to body only
                self.declare(var_span, DefKind::Local, *id);

                for st in &body.stmts {
                    self.resolve_stmt(st); // Share var scope
                }

                self.pop();
            }

            ast::Stmt::Expr(e) => self.resolve_expr(e),

            ast::Stmt::While { cond, body, .. } => {
                self.resolve_expr(cond);
                self.resolve_block(body);
            }

            ast::Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.resolve_expr(v);
                }
            }

            ast::Stmt::Break { .. } | ast::Stmt::Continue { .. } => {}
        }
    }
}

pub fn resolve(file: &ast::File, src: &str, a: &mut Analysis, diags: &mut Vec<Diagnostic>) {
    let mut r = Resolver {
        src,
        a,
        diags,
        scopes: Vec::new()
    };

    r.push();

    // Declare every item name but walk no bodies
    for item in &file.items {
        match item {
            ast::Item::Func(f) => {
                r.declare(&f.name_span, DefKind::Func, f.id);
            }

            ast::Item::Import(imp) => {
                // Only pull last part of the path: `io` from `import std::io`
                if let Some(last) = imp.path.last() {
                    r.declare(last, DefKind::Import, imp.id);
                }
            }
        }
    }

    // Walk bodies - every function name will already exist by now
    for item in &file.items {
        if let ast::Item::Func(f) = item {
            r.push(); // param scope

            for p in &f.params {
                r.declare(&p.name_span, DefKind::Param, p.id);
            }

            r.resolve_block(&f.body); // Body pushes it's own scope
            r.pop();
        }
    }

    r.pop();
}