use std::collections::HashMap;
use crate::analysis::*;
use crate::tir::*;
use crate::ast;

pub struct Lowerer<'a> {
    a: &'a Analysis, // Read only since lowering should only translate existing info not write new info
    f: Function, // Function being built
    cur: BlockId, // Current block instructions are being appended to
    next_value: u32, // Counter for temporaries - never reset or reused
    def_local: HashMap<DefId, LocalId>, // Bridge between analysis and TIR
    loop_stack: Vec<(BlockId, BlockId)> // (continue_target, break_target) per enclosing loop. `break` & `continue` is a one line lookup against the top of this stack
}

impl<'a> Lowerer<'a> {
    fn new_block(&mut self) -> BlockId {
        self.f.blocks.push(Block {
            instrs: Vec::new(),
            term: Terminator::Unfinished
        });

        BlockId(self.f.blocks.len() as u32 - 1)
    }

    fn switch_to(&mut self, b: BlockId) {
        self.cur = b;
    }

    /// Close the current block only if it's still open
    fn terminate(&mut self, t: Terminator) {
        let b = &mut self.f.blocks[self.cur.0 as usize];

        if matches!(b.term, Terminator::Unfinished) {
            b.term = t;
        }
    }

    /// Other half of `terminate()` rule: if the current block is already closed
    /// any further emission goes into a fresh orphan block which creates dead code
    /// since nothing will jump to it - LLVM should optimize it away
    fn ensure_open(&mut self) {
        if !matches!(self.f.blocks[self.cur.0 as usize].term, Terminator::Unfinished) {
            let b = self.new_block();
            self.switch_to(b);
        }
    }

    /// Emit an instruction that produces a value
    fn emit(&mut self, kind: InstrKind, ty: TypeId, origin: ast::NodeId) -> ValueId {
        self.ensure_open();

        let dest = ValueId(self.next_value);
        self.next_value += 1;

        self.f.blocks[self.cur.0 as usize].instrs.push(Instr {
            dest: Some(dest),
            ty,
            kind,
            origin
        });

        dest
    }

    /// Emit an instruction that only has an effect (stores, scope markers)
    fn emit_effect(&mut self, kind: InstrKind, origin: ast::NodeId) {
        self.ensure_open();

        let unit = self.a.prims.unit;
        self.f.blocks[self.cur.0 as usize].instrs.push(Instr {
            dest: None,
            ty: unit,
            kind,
            origin
        });
    }

    fn new_local(&mut self, ty: TypeId, name: Option<String>) -> LocalId {
        self.f.locals.push(LocalInfo { ty, name });
        LocalId(self.f.locals.len() as u32 - 1)
    }

    /// An expression's type. Panics if typeck was violated
    fn ty(&self, e: &ast::Expr) -> TypeId {
        *self.a.types.get(&e.id()).expect("typeck violated: expression has no type")
    }

    fn name_of(&self, def: DefId) -> String {
        self.a.symbols.resolve(self.a.definitions.info(def).name).to_string()
    }

    fn lower_block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.lower_stmt(s);
        }
    }

    fn lower_stmt(&mut self, s: &ast::Stmt) {
        match s {
            ast::Stmt::Let { id, init, .. } => {
                let v = self.lower_expr(init);
                let def = self.a.defs[id];
                let ty = self.a.def_types[&def];
                let local = self.new_local(ty, Some(self.name_of(def)));
                self.def_local.insert(def, local);
                self.emit_effect(InstrKind::StoreLocal(local, v), *id);
            }

            ast::Stmt::Expr(e) => {
                self.lower_expr(e);
            }

            ast::Stmt::While { id, cond, body, span } => {

            }

            _ => todo!("lower_stmt: {s:?}")
        }
    }

    fn lower_expr(&mut self, e: &ast::Expr) -> ValueId {
        let ty = self.ty(e);
        let origin = e.id();

        match e {
            ast::Expr::Int { id, .. } => {
                let LitValue::Int(v) = self.a.values[id] else {
                    unreachable!("Int node without an Int LitValue");
                };

                self.emit(InstrKind::ConstInt(v), ty, origin)
            }

            ast::Expr::Float { id, .. } => {
                let LitValue::Float(v) = self.a.values[id] else {
                    unreachable!("Float node without a Float LitValue");
                };

                self.emit(InstrKind::ConstFloat(v), ty, origin)
            }

            ast::Expr::Str { id, .. } => {
                let LitValue::Str(ref v) = self.a.values[id] else {
                    unreachable!("Str node without a Str LitValue");
                };

                self.emit(InstrKind::ConstStr(v.to_owned()), ty, origin)
            }

            ast::Expr::Char { id, .. } => {
                let LitValue::Char(v) = self.a.values[id] else {
                    unreachable!("Char node without a Char LitValue");
                };

                self.emit(InstrKind::ConstChar(v), ty, origin)
            }

            ast::Expr::Duration { id, .. } => {
                let LitValue::Duration(v) = self.a.values[id] else {
                    unreachable!("Duration node without a Duration LitValue");
                };

                self.emit(InstrKind::ConstDuration(v), ty, origin)
            }

            ast::Expr::Bool { id, .. } => {
                let LitValue::Bool(v) = self.a.values[id] else {
                    unreachable!("Bool node without a Bool LitValue");
                };

                self.emit(InstrKind::ConstBool(v), ty, origin)
            }

            ast::Expr::Var { id, .. } => {
                let def = self.a.defs[id];
                let local = self.def_local[&def];
                self.emit(InstrKind::LoadLocal(local), ty, origin)
            }

            ast::Expr::Unary { op, rhs, .. } => {
                let v = self.lower_expr(rhs);
                self.emit(InstrKind::Unary { op: lower_unop(*op), v }, ty, origin) // TODO: Implement lower_unop
            }

            ast::Expr::Binary { op, lhs, rhs, .. } => match op {
                ast::BinOp::And | ast::BinOp::Or => self.lower_short_circuit(e), // TODO: Implement lower_short_circuit
                _ => {
                    let l = self.lower_expr(lhs);
                    let r = self.lower_expr(rhs);
                    self.emit(InstrKind::Binary { op: lower_binop(*op), l, r }, ty, origin)
                }
            }

            _ => todo!("lower_expr: {e:?}")
        }
    }
}

fn lower_binop(op: ast::BinOp) -> BinOp {
    use ast::BinOp as A;

    match op {
        A::Add => BinOp::Add,
        A::Sub => BinOp::Sub,
        A::Mul => BinOp::Mul,
        A::Div => BinOp::Div,
        A::Rem => BinOp::Rem,
        A::Pow => BinOp::Pow,
        A::Eq => BinOp::Eq,
        A::NotEq => BinOp::NotEq,
        A::Lt => BinOp::Lt,
        A::Gt => BinOp::Gt,
        A::LtEq => BinOp::LtEq,
        A::GtEq => BinOp::GtEq,
        A::And | A::Or => unreachable!("and/or lower to control flow, not Binary")
    }
}

pub fn lower(file: &ast::File, a: &Analysis) -> Program {
    let mut funcs = Vec::new();

    for item in &file.items {
        if let ast::Item::Func(f) = item {
            funcs.push(lower_func(f, a));
        }
    }

    Program { funcs }
}

fn lower_func(src: &ast::Func, a: &Analysis) -> Function {
    let def = a.defs[&src.id];
    let fty = a.def_types[&def];
    let ret_ty = match a.type_table.get(fty) {
        Type::Func { ret, .. } => *ret,
        _ => a.prims.unit
    };

    let name = a.symbols.resolve(a.definitions.info(def).name).to_string();
    let mut lw = Lowerer {
        a,
        f: Function {
            name,
            def,
            params: Vec::new(),
            locals: Vec::new(),
            blocks: Vec::new(),
            ret_ty
        },
        cur: BlockId(0), // fixed up immediately by the entry block below
        next_value: 0,
        def_local: HashMap::new(),
        loop_stack: Vec::new()
    };

    // bb0 - entry block
    let entry = lw.new_block();
    lw.switch_to(entry);

    // Parameters become the first locals in declaration order
    for p in &src.params {
        let pdef = a.defs[&p.id];
        let ty = a.def_types[&pdef];
        let local = lw.new_local(ty, Some(lw.name_of(pdef)));
        lw.f.params.push(local);
        lw.def_local.insert(pdef, local);
    }

    lw.lower_block(&src.body);

    // Implicit return for functions that fall off the end
    lw.terminate(Terminator::Return(None));

    verify(&lw.f);
    lw.f
}