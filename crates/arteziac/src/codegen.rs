use std::collections::HashMap;
use crate::{analysis::{Analysis, DefId, Type, TypeId}, tir::{self, BinOp, InstrKind::*, Terminator, UnOp}};
use inkwell::{
    IntPredicate, basic_block::BasicBlock, builder::Builder, context::Context, module::Module, types::{BasicType, BasicTypeEnum}, values::{BasicValueEnum, FunctionValue, PointerValue}
};

pub struct Codegen<'ctx> {
    ctx: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    a: &'ctx Analysis,

    // Per-function states
    blocks: HashMap<tir::BlockId, BasicBlock<'ctx>>, // TIR block -> LLVM block
    values: HashMap<tir::ValueId, BasicValueEnum<'ctx>>, // TIR temp -> LLVM value
    locals: HashMap<tir::LocalId, PointerValue<'ctx>>, // TIR slot -> alloca ptr
    func_vals: HashMap<DefId, FunctionValue<'ctx>> // TIR func -> LLVM function
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(ctx: &'ctx Context, a: &'ctx Analysis) -> Self {
        Self {
            ctx,
            module: ctx.create_module("artezia"),
            builder: ctx.create_builder(),
            a,

            blocks: HashMap::new(),
            values: HashMap::new(),
            locals: HashMap::new(),
            func_vals: HashMap::new()
        }
    }

    pub fn run(mut self, program: &tir::Program) -> Module<'ctx> {
        // Declare all function signatures
        for f in &program.funcs {
            let param_types: Vec<_> = f.params.iter().map(|param| self.llvm_type(f.locals[param.0 as usize].ty).into()).collect();
            let fn_type = if f.ret_ty == self.a.prims.unit {
                self.ctx.void_type().fn_type(&*param_types, false)
            } else {
                self.llvm_type(f.ret_ty).fn_type(&*param_types, false)
            };

            let fv = self.module.add_function(&f.name, fn_type, None);
            self.func_vals.insert(f.def, fv);
        }

        // Emit bodies
        for f in &program.funcs {
            self.emit_function(f);
        }

        self.module
    }

    fn llvm_type(&self, ty: TypeId) -> BasicTypeEnum<'ctx> {
        match self.a.type_table.get(ty) {
            Type::Int => self.ctx.i64_type().into(),
            Type::Bool => self.ctx.bool_type().into(),
            Type::Float => self.ctx.f64_type().into(),
            Type::Char => self.ctx.i32_type().into(),
            Type::Unit => self.ctx.i64_type().into(), // Just for representation because `void` isn't a real value
            // TODO: Implement Range, Str, Duration, Func later
            _ => todo!("llvm_type: {ty:?}")
        }
    }

    fn emit_function(&mut self, f: &tir::Function) {
        self.blocks.clear();
        self.values.clear();
        self.locals.clear();

        let fv = self.func_vals[&f.def];

        // Create all LLVM blocks upfront
        for (i, _) in f.blocks.iter().enumerate() {
            let bb = self.ctx.append_basic_block(fv, &format!("bb{i}"));
            self.blocks.insert(tir::BlockId(i as u32), bb);
        }

        // Alloca for all locals in the entry block
        self.builder.position_at_end(self.blocks[&tir::BlockId(0)]);
        for (i, local) in f.locals.iter().enumerate() {
            let name = local.name.as_deref().unwrap_or("tmp");
            let ptr = self.builder.build_alloca(self.llvm_type(local.ty), name).unwrap();
            self.locals.insert(tir::LocalId(i as u32), ptr);
        }

        // Store incoming parameters into their slots
        for (i, local_id) in f.params.iter().enumerate() {
            let arg = fv.get_nth_param(i as u32).unwrap();
            self.builder.build_store(self.locals[local_id], arg).unwrap();
        }

        // Fill each block with instructions and terminator
        for (i, block) in f.blocks.iter().enumerate() {
            self.builder.position_at_end(self.blocks[&tir::BlockId(i as u32)]);

            for instr in &block.instrs {
                self.emit_instr(instr);
            }

            self.emit_terminator(&block.term);
        }
    }

    fn emit_instr(&mut self, instr: &tir::Instr) {
        let v: Option<BasicValueEnum> = match &instr.kind {
            ConstInt(n) => Some(self.ctx.i64_type().const_int(*n as u64, true).into()),
            ConstBool(b) => Some(self.ctx.bool_type().const_int(*b as u64, false).into()),
            ConstFloat(x) => Some(self.ctx.f64_type().const_float(*x).into()),

            LoadLocal(l) => {
                let ptr = self.locals[l];
                let ty = self.llvm_type(instr.ty);
                Some(self.builder.build_load(ty, ptr, "load").unwrap())
            }

            StoreLocal(l, v) => {
                let val = self.values[v];
                self.builder.build_store(self.locals[l], val).unwrap();
                None // Effect only
            }

            Binary { op, l, r } => {
                let lv = self.values[l].into_int_value();
                let rv = self.values[r].into_int_value();
                let res = match op {
                    BinOp::Add => self.builder.build_int_add(lv, rv, "add").unwrap(),
                    BinOp::Sub => self.builder.build_int_sub(lv, rv, "sub").unwrap(),
                    BinOp::Mul => self.builder.build_int_mul(lv, rv, "mul").unwrap(),
                    BinOp::Div => self.builder.build_int_signed_div(lv, rv, "div").unwrap(),
                    BinOp::Rem => self.builder.build_int_signed_rem(lv, rv, "rem").unwrap(),
                    BinOp::Pow => todo!("Implement builder calls to stdlib"),
                    BinOp::Eq => self.builder.build_int_compare(IntPredicate::EQ, lv, rv, "eq").unwrap(),
                    BinOp::NotEq => self.builder.build_int_compare(IntPredicate::NE, lv, rv, "neq").unwrap(),
                    BinOp::Lt => self.builder.build_int_compare(IntPredicate::SLT, lv, rv, "lt").unwrap(),
                    BinOp::Gt => self.builder.build_int_compare(IntPredicate::SGT, lv, rv, "gt").unwrap(),
                    BinOp::LtEq => self.builder.build_int_compare(IntPredicate::SLE, lv, rv, "lteq").unwrap(),
                    BinOp::GtEq => self.builder.build_int_compare(IntPredicate::SGE, lv, rv, "gteq").unwrap()
                };

                Some(res.into())
            }

            Unary { op, v } => {
                let val = self.values[v].into_int_value();
                let res = match op {
                    UnOp::Neg => self.builder.build_int_neg(val, "neg").unwrap(),
                    UnOp::Not => self.builder.build_not(val, "not").unwrap()
                };

                Some(res.into())
            }

            Call { func, args } => {
                let fv = self.func_vals[func];
                let argv: Vec<_> = args.iter().map(|a| self.values[a].into()).collect();
                let call = self.builder.build_call(fv, &argv, "call").unwrap();
                call.try_as_basic_value().basic()
            }

            ConstUnit => {
                Some(self.ctx.i64_type().const_int(0, false).into()) // Placeholder, never used
            }

            ConstChar(v) => {
                Some(self.ctx.i32_type().const_int(*v as _, false).into())
            }

            ConstStr(v) => {
                // TODO: This will need to change to stdlib struct once structs are working
                Some(self.ctx.const_string(v.as_bytes(), false).into())
            }

            ConstDuration(v) => {
                Some(self.ctx.i64_type().const_int(*v, false).into())
            }

            RangeNew { .. } => {
                todo!("Range as a value needs structs (stdlib Range type) - not yet")
            }

            ScopeEnter | ScopeExit => {
                todo!("scope codegen requires artezia_rt (not yet built)")
            }

            Spawn { .. } => {
                todo!("spawn codegen requires artezia_rt (not yet built)")
            }

            WithinEnter { .. } | WithinExit => {
                todo!("within codegen requires artezia_rt (not yet built)")
            }
        };

        if let (Some(dest), Some(val)) = (instr.dest, v) {
            self.values.insert(dest, val);
        }
    }

    fn emit_terminator(&mut self, term: &tir::Terminator) {
        match term {
            Terminator::Return(Some(v)) => {
                self.builder.build_return(Some(&self.values[v])).unwrap();
            }

            Terminator::Return(None) => {
                self.builder.build_return(None).unwrap();
            }

            Terminator::Jump(b) => {
                self.builder.build_unconditional_branch(self.blocks[b]).unwrap();
            }

            Terminator::Branch { cond, then_bb, else_bb } => {
                let c = self.values[cond].into_int_value();
                self.builder.build_conditional_branch(c, self.blocks[then_bb], self.blocks[else_bb]).unwrap();
            }

            Terminator::Unfinished | Terminator::Unreachable => unreachable!("TIR verifier should have caught this")
        }
    }
}