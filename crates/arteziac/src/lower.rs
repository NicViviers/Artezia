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

    /// An expression's type. Panics if typecheck was violated
    fn ty(&self, e: &ast::Expr) -> TypeId {
        *self.a.types.get(&e.id()).expect("typecheck violated: expression has no type")
    }

    fn name_of(&self, def: DefId) -> String {
        self.a.symbols.resolve(self.a.definitions.info(def).name).to_string()
    }

    fn current_terminated(&self) -> bool {
        !matches!(self.f.blocks[self.cur.0 as usize].term, Terminator::Unfinished)
    }

    fn lower_block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.lower_stmt(s);
        }

        if let Some(e) = &b.tail {
            self.lower_expr(e);
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

            ast::Stmt::While { cond, body, .. } => {
                let cond_bb = self.new_block();
                let body_bb = self.new_block();
                let exit_bb = self.new_block();

                self.terminate(Terminator::Jump(cond_bb)); // Enter the loop

                // Condition
                self.switch_to(cond_bb);
                let c = self.lower_expr(cond);
                self.terminate(Terminator::Branch { cond: c, then_bb: body_bb, else_bb: exit_bb });

                // Body - register loop targets so break/continue resolve
                self.loop_stack.push((cond_bb, exit_bb)); // (continue -> cond, break -> exit)
                self.switch_to(body_bb);
                self.lower_block(body);
                self.terminate(Terminator::Jump(cond_bb)); // back-edge <- future yield point
                self.loop_stack.pop();

                // Continue after the loop
                self.switch_to(exit_bb);
            }

            ast::Stmt::Break { .. } => {
                let (_, brk) = *self.loop_stack.last().expect("break outside loop (typeck missed?)");
                self.terminate(Terminator::Jump(brk));
                // code after break: terminate() froze this block
                // the next emit hits ensure_open -> fresh orphan -> dead code vanishes
            }

            ast::Stmt::Continue { .. } => {
                let (cont, _) = *self.loop_stack.last().expect("continue outside loop");
                self.terminate(Terminator::Jump(cont));
            }

            ast::Stmt::For { id, iter, body, .. } => {
                // Iter is always a Range (typeck enforced). Pull lo/hi out
                let ast::Expr::Range { lo, hi, inclusive, .. } = iter else {
                    unreachable!("for over non-range: typeck should have caught")
                };

                // Loop variable = a local, initialized to lo
                let idef = self.a.defs[id];
                let int_ty = self.a.prims.int;
                let i_local = self.new_local(int_ty, Some(self.name_of(idef)));
                self.def_local.insert(idef, i_local);

                let lo_v = self.lower_expr(lo);
                self.emit_effect(InstrKind::StoreLocal(i_local, lo_v), *id);

                // hi evaluated once, into a hidden temp local (not per-iteration)
                let hi_v = self.lower_expr(hi);
                let hi_local = self.new_local(int_ty, None);
                self.emit_effect(InstrKind::StoreLocal(hi_local, hi_v), *id);

                let cond_bb = self.new_block();
                let body_bb = self.new_block();
                let incr_bb = self.new_block(); // continue targets this, not cond
                let exit_bb = self.new_block();

                self.terminate(Terminator::Jump(cond_bb));

                // cond: i < hi  (or <= if inclusive)
                self.switch_to(cond_bb);
                let iv = self.emit(InstrKind::LoadLocal(i_local), int_ty, *id);
                let hv = self.emit(InstrKind::LoadLocal(hi_local), int_ty, *id);
                let cmp = if *inclusive { BinOp::LtEq } else { BinOp::Lt };
                let c = self.emit(InstrKind::Binary { op: cmp, l: iv, r: hv }, self.a.prims.boolean, *id);
                self.terminate(Terminator::Branch { cond: c, then_bb: body_bb, else_bb: exit_bb });

                // Body — continue goes to increment, not cond, so it still advances
                self.loop_stack.push((incr_bb, exit_bb));
                self.switch_to(body_bb);
                self.lower_block(body);
                self.terminate(Terminator::Jump(incr_bb));
                self.loop_stack.pop();

                // Increment: i = i + 1; back to cond
                self.switch_to(incr_bb);
                let iv2 = self.emit(InstrKind::LoadLocal(i_local), int_ty, *id);
                let one = self.emit(InstrKind::ConstInt(1), int_ty, *id);
                let next = self.emit(InstrKind::Binary { op: BinOp::Add, l: iv2, r: one }, int_ty, *id);
                self.emit_effect(InstrKind::StoreLocal(i_local, next), *id);
                self.terminate(Terminator::Jump(cond_bb));

                self.switch_to(exit_bb);
            }

            ast::Stmt::Return { value, .. } => {
                let v = value.as_ref().map(|e| self.lower_expr(e));
                self.terminate(Terminator::Return(v))
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
                self.emit(InstrKind::Unary { op: lower_unop(*op), v }, ty, origin)
            }

            ast::Expr::Binary { op, lhs, rhs, .. } => match op {
                ast::BinOp::And | ast::BinOp::Or => self.lower_short_circuit(e),
                _ => {
                    let l = self.lower_expr(lhs);
                    let r = self.lower_expr(rhs);
                    self.emit(InstrKind::Binary { op: lower_binop(*op), l, r }, ty, origin)
                }
            }

            ast::Expr::Range { lo, hi, inclusive, .. } => {
                let l = self.lower_expr(lo);
                let h = self.lower_expr(hi);
                self.emit(InstrKind::RangeNew { lo: l, hi: h, inclusive: *inclusive }, ty, origin)
            }

            ast::Expr::Call { callee, args, .. } => {
                let fdef = self.a.defs[&callee.id()];
                let args_vals: Vec<ValueId> = args.iter().map(|arg| self.lower_expr(&arg.value)).collect();
                self.emit(InstrKind::Call { func: fdef, args: args_vals }, ty, origin)
            }

            ast::Expr::Assign { target, value, .. } => {
                let v = self.lower_expr(value);
                let def = self.a.defs[&target.id()];
                let local = self.def_local[&def];
                self.emit_effect(InstrKind::StoreLocal(local, v), origin);
                self.emit(InstrKind::ConstUnit, self.a.prims.unit, origin)
            }

            ast::Expr::If { cond, then, els, .. } => {
                let c = self.lower_expr(cond);

                let then_bb = self.new_block();
                let join_bb = self.new_block();
                let else_bb = match els {
                    Some(_) => self.new_block(),
                    None => join_bb, // no else -> false path goes straight to join
                };

                // Valued if? allocate a result slot (skip for Unit)
                let result = if ty != self.a.prims.unit {
                    Some(self.new_local(ty, None))
                } else {
                    None
                };

                // Close the current block with the branch
                self.terminate(Terminator::Branch { cond: c, then_bb, else_bb });

                self.switch_to(then_bb);
                match self.lower_block_value(then) {
                    Some(tv) => {
                        if let Some(slot) = result {
                            self.emit_effect(InstrKind::StoreLocal(slot, tv), origin);
                        }

                        self.terminate(Terminator::Jump(join_bb));
                    }

                    None => {} // Branch already terminated (returned/broke) - no store, no jump
                }

                if let Some(else_expr) = els {
                    self.switch_to(else_bb);
                    let ev = self.lower_expr(else_expr);
                    if let Some(slot) = result {
                        self.emit_effect(InstrKind::StoreLocal(slot, ev), origin);
                    }
                    self.terminate(Terminator::Jump(join_bb));
                }

                self.switch_to(join_bb);
                match result {
                    Some(slot) => self.emit(InstrKind::LoadLocal(slot), ty, origin),
                    None => self.emit(InstrKind::ConstUnit, ty, origin),
                }
            }

            ast::Expr::Block(b) => {
                self.lower_block(b);
                self.emit(InstrKind::ConstUnit, self.a.prims.unit, origin)
            }

            _ => todo!("lower_expr: {e:?}")
        }
    }

    fn lower_block_value(&mut self, b: &ast::Block) -> Option<ValueId> {
        for s in &b.stmts {
            self.lower_stmt(s);
        }

        if self.current_terminated() {
            return None; // A statement returned/broke - no value, no orphan
        }
        
        Some(match &b.tail {
            Some(e) => self.lower_expr(e),
            None => self.emit(InstrKind::ConstUnit, self.a.prims.unit, b.id)
        })
    }

    fn lower_short_circuit(&mut self, e: &ast::Expr) -> ValueId {
        let ast::Expr::Binary { op, lhs, rhs, .. } = e else { unreachable!() };
        let origin = e.id();
        let bool_ty = self.a.prims.boolean;

        let l = self.lower_expr(lhs); // Always evaluate lhs
        let result = self.new_local(bool_ty, None);

        let rhs_bb = self.new_block();
        let join_bb = self.new_block();

        match op {
            ast::BinOp::And => {
                // If l -> evaluate rhs; else -> result = false
                let short_bb = self.new_block();
                self.terminate(Terminator::Branch { cond: l, then_bb: rhs_bb, else_bb: short_bb });
                // Short: false
                self.switch_to(short_bb);
                let f = self.emit(InstrKind::ConstBool(false), bool_ty, origin);
                self.emit_effect(InstrKind::StoreLocal(result, f), origin);
                self.terminate(Terminator::Jump(join_bb));
            }

            ast::BinOp::Or => {
                // If l -> result = true; else -> evaluate rhs
                let short_bb = self.new_block();
                self.terminate(Terminator::Branch { cond: l, then_bb: short_bb, else_bb: rhs_bb });
                self.switch_to(short_bb);
                let t = self.emit(InstrKind::ConstBool(true), bool_ty, origin);
                self.emit_effect(InstrKind::StoreLocal(result, t), origin);
                self.terminate(Terminator::Jump(join_bb));
            }
            _ => unreachable!(),
        }

        // rhs path (shared): result = rhs
        self.switch_to(rhs_bb);
        let r = self.lower_expr(rhs); // Only evaluated on this path
        self.emit_effect(InstrKind::StoreLocal(result, r), origin);
        self.terminate(Terminator::Jump(join_bb));

        self.switch_to(join_bb);
        self.emit(InstrKind::LoadLocal(result), bool_ty, origin)
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

fn lower_unop(op: ast::UnOp) -> UnOp {
    match op {
        ast::UnOp::Neg => UnOp::Neg,
        ast::UnOp::Not => UnOp::Not,
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

    match lw.lower_block_value(&src.body) {
        Some(v) if lw.f.ret_ty != a.prims.unit => {
            lw.terminate(Terminator::Return(Some(v)));
        }

        Some(_) => {
            lw.terminate(Terminator::Return(None));
        }

        None => {} // Body already terminated (explicit return) - append nothing
    }

    verify(&lw.f);
    lw.f
}