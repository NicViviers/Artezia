use crate::analysis::{Analysis, Type, TypeId};
use crate::parser::Span;
use crate::ast;
use artezia_diag::{Diagnostic, Severity};
use chumsky::container::Seq;

pub fn typecheck(
    file: &ast::File,
    src: &str,
    a: &mut Analysis,
    diags: &mut Vec<Diagnostic>,
) {
    let unit = a.prims.unit;
    let mut c = Checker {
        a,
        diags,
        src,
        current_ret: unit,
        in_loop: false,
    };

    for item in &file.items {
        if let ast::Item::Func(f) = item {
            let params: Vec<TypeId> =
                f.params.iter().map(|p| c.lower_type(&p.ty)).collect();
            let ret = f
                .ret
                .as_ref()
                .map_or(c.a.prims.unit, |t| c.lower_type(t));
            let fty = c
                .a
                .type_table
                .intern(Type::Func { params: params.clone(), ret });

            // The defs convention pays off: defining node -> its own DefId
            if let Some(&def) = c.a.defs.get(&f.id) {
                c.a.def_types.insert(def, fty);
            }
            for (p, ty) in f.params.iter().zip(&params) {
                if let Some(&pd) = c.a.defs.get(&p.id) {
                    c.a.def_types.insert(pd, *ty);
                }
            }
        }
    }

    for item in &file.items {
        if let ast::Item::Func(f) = item {
            // Recover this function's return type for `return` checking
            c.current_ret = c
                .a
                .defs
                .get(&f.id)
                .and_then(|def| c.a.def_types.get(def))
                .map(|&fty| match c.a.type_table.get(fty) {
                    Type::Func { ret, .. } => *ret,
                    _ => c.a.prims.unit,
                })
                .unwrap_or(c.a.prims.unit);
            c.in_loop = false;
            c.check_block(&f.body);
        }
    }
}

struct Checker<'a> {
    a: &'a mut Analysis,
    diags: &'a mut Vec<Diagnostic>,
    src: &'a str,
    current_ret: TypeId,
    in_loop: bool,
}

impl Checker<'_> {
    fn error(&mut self, span: Span, msg: impl Into<String>) {
        self.diags
            .push(Diagnostic::new(Severity::Error, span, msg.into()));
    }

    fn expect_type(&mut self, actual: TypeId, expected: TypeId, span: Span) {
        if actual == expected {
            return;
        }
        if actual == self.a.prims.error || expected == self.a.prims.error {
            return; // poison: already diagnosed upstream
        }
        let msg = format!(
            "expected {}, found {}",
            self.a.type_table.display(expected),
            self.a.type_table.display(actual)
        );
        self.error(span, msg);
    }

    fn check_binop(
        &mut self,
        op: ast::BinOp,
        l: TypeId,
        r: TypeId,
        span: &Span,
    ) -> TypeId {
        use ast::BinOp::*;

        let p = self.a.prims;
        if l == p.error || r == p.error {
            return p.error; // poison in = poison out, silently
        }

        match op {
            Add | Sub | Mul | Div | Rem | Pow => {
                if l == r && (l == p.int || l == p.float) {
                    l
                } else {
                    self.binop_err(op, l, r, span);
                    p.error
                }
            }

            Eq | NotEq | Lt | Gt | LtEq | GtEq => {
                if l == r && l != p.unit {
                    p.boolean
                } else {
                    self.binop_err(op, l, r, span);
                    p.error
                }
            }

            And | Or => {
                if l == p.boolean && r == p.boolean {
                    p.boolean
                } else {
                    self.binop_err(op, l, r, span);
                    p.error
                }
            }
        }
    }

    fn binop_err(&mut self, op: ast::BinOp, l: TypeId, r: TypeId, span: &Span) {
        let msg = format!(
            "operator `{:?}` cannot be applied to {} and {}",
            op,
            self.a.type_table.display(l),
            self.a.type_table.display(r)
        );
        self.error(span.clone(), msg);
    }

    fn check_unop(&mut self, op: ast::UnOp, r: TypeId, span: &Span) -> TypeId {
        let p = self.a.prims;
        if r == p.error {
            return p.error;
        }
        match op {
            ast::UnOp::Neg if r == p.int || r == p.float => r,
            ast::UnOp::Not if r == p.boolean => p.boolean,
            _ => {
                let msg = format!(
                    "operator `{:?}` cannot be applied to {}",
                    op,
                    self.a.type_table.display(r)
                );
                self.error(span.clone(), msg);
                p.error
            }
        }
    }

    /// Branch joining for if/within expressions: equal -> that type; one
    /// Error -> the other (poison absorption); mismatch -> diagnostic + Error
    fn join_branches(&mut self, t: TypeId, f: TypeId, span: &Span) -> TypeId {
        let p = self.a.prims;

        if t == f {
            return t;
        }

        if t == p.error {
            return f;
        }

        if f == p.error {
            return t;
        }

        let msg = format!(
            "branches have incompatible types: {} and {}",
            self.a.type_table.display(t),
            self.a.type_table.display(f)
        );

        self.error(span.clone(), msg);
        p.error
    }

    fn lower_type(&mut self, t: &ast::Type) -> TypeId {
        let ast::Type::Named { path, span, .. } = t;
        if path.len() == 1 {
            match &self.src[path[0].clone()] {
                "Int" => return self.a.prims.int,
                "Float" => return self.a.prims.float,
                "Bool" => return self.a.prims.boolean,
                "String" => return self.a.prims.str_,
                "Char" => return self.a.prims.char_,
                "Duration" => return self.a.prims.duration,
                "Unit" => return self.a.prims.unit,
                _ => {}
            }
        }
        let name = &self.src[span.clone()];
        self.error(span.clone(), format!("unknown type `{name}`"));
        self.a.prims.error
    }

    fn check_block(&mut self, b: &ast::Block) -> TypeId {
        for s in &b.stmts {
            self.check_stmt(s);
        }

        let ty = self.a.prims.unit;
        self.a.types.insert(b.id, ty);

        ty
    }

    fn check_stmt(&mut self, s: &ast::Stmt) {
        match s {
            ast::Stmt::Let { id, ty, init, .. } => {
                let ity = self.check_expr(init);
                let final_ty = match ty {
                    Some(ann) => {
                        let want = self.lower_type(ann);
                        self.expect_type(ity, want, init.span());
                        want
                    }
                    None => ity,
                };
                if let Some(&def) = self.a.defs.get(id) {
                    self.a.def_types.insert(def, final_ty);
                }
            }

            ast::Stmt::Expr(e) => {
                self.check_expr(e);
            }

            ast::Stmt::While { cond, body, .. } => {
                let c = self.check_expr(cond);
                self.expect_type(c, self.a.prims.boolean, cond.span());
                let was = self.in_loop; // save/restore, not set/clear:
                self.in_loop = true; // nested whiles must not clear the
                self.check_block(body); // outer loop's flag on exit
                self.in_loop = was;
            }

            ast::Stmt::For { id, iter, body, .. } => {
                let ity = self.check_expr(iter);
                self.expect_type(ity, self.a.prims.range, iter.span());

                if let Some(&def) = self.a.defs.get(id) {
                    self.a.def_types.insert(def, self.a.prims.int);
                }

                let was = self.in_loop;
                self.in_loop = true;
                self.check_block(body);
                self.in_loop = was;
            }

            ast::Stmt::Return { value, span, .. } => {
                let vty = value
                    .as_ref()
                    .map_or(self.a.prims.unit, |v| self.check_expr(v));
                self.expect_type(vty, self.current_ret, span.clone());
            }

            ast::Stmt::Break { span, .. } | ast::Stmt::Continue { span, .. } => {
                if !self.in_loop {
                    self.error(
                        span.clone(),
                        "cannot be used outside of a loop".to_string(),
                    );
                }
            }
        }
    }

    fn check_expr(&mut self, e: &ast::Expr) -> TypeId {
        let p = self.a.prims;
        let ty = match e {
            ast::Expr::Int { .. } => p.int,
            ast::Expr::Float { .. } => p.float,
            ast::Expr::Bool { .. } => p.boolean,
            ast::Expr::Str { .. } => p.str_,
            ast::Expr::Char { .. } => p.char_,
            ast::Expr::Duration { .. } => p.duration,
            ast::Expr::Error { .. } => p.error, // poison flows

            ast::Expr::Var { id, .. } => match self.a.defs.get(id) {
                Some(def) => self
                    .a
                    .def_types
                    .get(def)
                    .copied()
                    .unwrap_or(p.error),
                None => p.error, // poison rule: resolve already spoke - silent
            },

            ast::Expr::Unary { op, rhs, span, .. } => {
                let r = self.check_expr(rhs);
                self.check_unop(*op, r, span)
            }

            ast::Expr::Binary { op, lhs, rhs, span, .. } => {
                let l = self.check_expr(lhs);
                let r = self.check_expr(rhs);
                self.check_binop(*op, l, r, span)
            }

            ast::Expr::Assign { target, value, .. } => {
                if !matches!(**target, ast::Expr::Var { .. }) {
                    self.error(
                        target.span(),
                        "invalid assignment target".to_string(),
                    );
                }
                let tty = self.check_expr(target);
                let vty = self.check_expr(value);
                self.expect_type(vty, tty, value.span());
                p.unit // assignment yields Unit
            }

            ast::Expr::Call { callee, args, span, .. } => {
                let fty = self.check_expr(callee);
                match self.a.type_table.get(fty).clone() {
                    Type::Func { params, ret } => {
                        if args.len() != params.len() {
                            let msg = format!(
                                "this call takes {} argument(s), found {}",
                                params.len(),
                                args.len()
                            );
                            self.error(span.clone(), msg);
                        }

                        for (arg, &pty) in args.iter().zip(&params) {
                            let aty = self.check_expr(&arg.value);
                            self.expect_type(aty, pty, arg.value.span());
                        }
                        ret
                    }
                    Type::Error => p.error, // poison: silent
                    _ => {
                        let msg = format!(
                            "this is not callable (it has type {})",
                            self.a.type_table.display(fty)
                        );
                        self.error(callee.span(), msg);
                        p.error
                    }
                }
            }

            ast::Expr::If { cond, then, els, span, .. } => {
                let c = self.check_expr(cond);
                self.expect_type(c, p.boolean, cond.span());
                let t = self.check_block(then);
                match els {
                    Some(e) => {
                        let f = self.check_expr(e);
                        self.join_branches(t, f, span)
                    }
                    None => p.unit,
                }
            }

            ast::Expr::Range { lo, hi, .. } => {
                let lt = self.check_expr(lo);
                let ht = self.check_expr(hi);

                self.expect_type(lt, p.int, lo.span());
                self.expect_type(ht, p.int, hi.span());
                
                p.range
            }

            ast::Expr::Scope { body, .. } => {
                self.check_block(body);
                p.unit
            }

            ast::Expr::Spawn { call, .. } => {
                if !matches!(**call, ast::Expr::Call { .. }) {
                    self.error(
                        call.span(),
                        "`spawn` expects a function call".to_string(),
                    );
                }
                self.check_expr(call);
                p.unit
            }

            ast::Expr::Within { dur, body, els, span, .. } => {
                let d = self.check_expr(dur);
                self.expect_type(d, p.duration, dur.span());
                let b = self.check_block(body);
                match els {
                    Some(eb) => {
                        let t = self.check_block(eb);
                        self.join_branches(b, t, span)
                    }
                    None => p.unit,
                }
            }

            ast::Expr::Block(b) => self.check_block(b),

            ast::Expr::Field { span, .. } => {
                self.error(span.clone(), "field access is not supported yet".to_string());
                p.error
            }
            ast::Expr::Index { span, .. } => {
                self.error(span.clone(), "indexing is not supported yet".to_string());
                p.error
            }
        };

        self.a.types.insert(e.id(), ty);
        ty
    }
}