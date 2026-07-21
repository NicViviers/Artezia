use crate::analysis::{Analysis, DefId, TypeId};
use crate::ast::{Func, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub u32); // a temporary value used many times

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32); // a memory slot (param, let, loop var)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32); // a basic block within one function

#[derive(Debug)]
pub struct Program {
    pub funcs: Vec<Function> // indexed by FuncId order = DefId discovery order
}

#[derive(Debug)]
pub struct Function {
    pub name: String, // resolved from the internet at lowering time
    pub def: DefId,
    pub params: Vec<LocalId>,
    pub locals: Vec<LocalInfo>,
    pub blocks: Vec<Block>,
    pub ret_ty: TypeId,
}


#[derive(Debug)]
pub struct LocalInfo {
    pub ty: TypeId,
    pub name: Option<String> // for dumps/debug info
}

#[derive(Debug)]
pub struct Block {
    pub instrs: Vec<Instr>,
    pub term: Terminator
}

#[derive(Debug)]
pub enum Terminator {
    Return(Option<ValueId>), // None = return unit
    Jump(BlockId),
    Branch { cond: ValueId, then_bb: BlockId, else_bb: BlockId },
    Unfinished // Placeholder during construction only; lowering must replace every instance
}

impl Terminator {
    pub fn targets(&self) -> Vec<BlockId> {
        match self {
            Terminator::Return(_) | Terminator::Unfinished => Vec::new(),
            Terminator::Jump(b) => vec![*b],
            Terminator::Branch { then_bb, else_bb, .. } => vec![*then_bb, *else_bb]
        }
    }

    pub fn used_values(&self) -> Vec<ValueId> {
        match self {
            Terminator::Return(Some(v)) => vec![*v],
            Terminator::Branch { cond, .. } => vec![*cond],
            Terminator::Return(None) | Terminator::Jump(_) | Terminator::Unfinished => vec![],
        }
    }
}

#[derive(Debug)]
pub struct Instr {
    pub dest: Option<ValueId>, // the value this defines
    pub ty: TypeId, // dest's type
    pub kind: InstrKind,
    pub origin: NodeId, // every instruction traces to a source node -> span -> messages
}

#[derive(Debug)]
pub enum InstrKind {
    // Valuse
    ConstInt(i64),
    ConstFloat(f64),
    ConstBool(bool),
    ConstStr(String),
    ConstChar(char),
    ConstDuration(u64), // nanoseconds
    ConstUnit,

    // Memory
    LoadLocal(LocalId),
    StoreLocal(LocalId, ValueId),

    // Computation
    Binary {
        op: BinOp,
        l: ValueId,
        r: ValueId
    }, // No and/or here, they lower to branches

    Unary {
        op: UnOp,
        v: ValueId
    },

    RangeNew {
        lo: ValueId,
        hi: ValueId,
        inclusive: bool
    },

    // Calls
    Call {
        func: DefId,
        args: Vec<ValueId>
    },

    // Concurrency - real instructions node, codegen rejects them until artezia_rt exists ("scope is not yet supported by codegen")
    ScopeEnter,
    ScopeExit,
    Spawn {
        func: DefId,
        args: Vec<ValueId>
    },

    WithinEnter {
        dur: ValueId
    },

    WithinExit
}

impl InstrKind {
    pub fn used_values(&self) -> Vec<ValueId> {
        use InstrKind::*;

        match self {
            ConstInt(_) | ConstFloat(_) | ConstBool(_) | ConstStr(_) | ConstChar(_) | ConstDuration(_) | ConstUnit => Vec::new(),
            LoadLocal(_) => Vec::new(),
            StoreLocal(_, v) => vec![*v],
            Binary { l, r, .. } => vec![*l, *r],
            Unary { v, .. } => vec![*v],
            RangeNew { lo, hi, .. } => vec![*lo, *hi],
            Call { args, .. } => args.clone(),
            ScopeEnter | ScopeExit | WithinExit => Vec::new(),
            Spawn { args, .. } => args.clone(),
            WithinEnter { dur } => vec![*dur]
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq
}

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Neg,
    Not
}

pub fn dump(f: &Function, a: &Analysis) -> String {
    let mut out = String::new();
 
    // signature line
    let params: Vec<String> = f
        .params
        .iter()
        .map(|l| format!("%{}", l.0))
        .collect();

    out.push_str(&format!(
        "func {}({}) -> {} {{\n",
        f.name,
        params.join(", "),
        a.type_table.display(f.ret_ty)
    ));
 
    // locals header
    for (i, l) in f.locals.iter().enumerate() {
        let name = l
            .name
            .as_deref()
            .map(|n| format!(" ({n})"))
            .unwrap_or_default();

        out.push_str(&format!(
            "  local %{}: {}{}\n",
            i,
            a.type_table.display(l.ty),
            name
        ));
    }
 
    // blocks
    for (bi, b) in f.blocks.iter().enumerate() {
        out.push_str(&format!("  bb{bi}:\n"));
        for ins in &b.instrs {
            out.push_str("    ");
            if let Some(d) = ins.dest {
                out.push_str(&format!("v{} = ", d.0));
            }
            out.push_str(&fmt_instr(&ins.kind, a));
            out.push('\n');
        }
        out.push_str(&format!("    {}\n", fmt_term(&b.term)));
    }
 
    out.push_str("}\n");
    out
}
 
fn fmt_instr(k: &InstrKind, a: &Analysis) -> String {
    use InstrKind::*;

    match k {
        ConstInt(v) => format!("const.int {v}"),
        ConstFloat(v) => format!("const.float {v}"),
        ConstBool(v) => format!("const.bool {v}"),
        ConstStr(s) => format!("const.str {s:?}"),
        ConstChar(c) => format!("const.char {c:?}"),
        ConstDuration(ns) => format!("const.duration {ns}ns"),
        ConstUnit => "const.unit".to_string(),
 
        LoadLocal(l) => format!("load %{}", l.0),
        StoreLocal(l, v) => format!("store %{}, v{}", l.0, v.0),
 
        Binary { op, l, r } => {
            format!("{} v{}, v{}", fmt_binop(*op), l.0, r.0)
        }

        Unary { op, v } => format!("{} v{}", fmt_unop(*op), v.0),
        RangeNew { lo, hi, inclusive } => format!(
            "range v{} {} v{}",
            lo.0,
            if *inclusive { "..=" } else { ".." },
            hi.0
        ),
 
        Call { func, args } => format!(
            "call {} ({})",
            fmt_def(*func, a),
            fmt_values(args)
        ),
 
        ScopeEnter => "scope.enter".to_string(),
        ScopeExit => "scope.exit".to_string(),
        Spawn { func, args } => format!(
            "spawn {} ({})",
            fmt_def(*func, a),
            fmt_values(args)
        ),

        WithinEnter { dur } => format!("within.enter v{}", dur.0),
        WithinExit => "within.exit".to_string(),
    }
}
 
fn fmt_term(t: &Terminator) -> String {
    match t {
        Terminator::Return(None) => "return".to_string(),
        Terminator::Return(Some(v)) => format!("return v{}", v.0),
        Terminator::Jump(b) => format!("jump bb{}", b.0),
        Terminator::Branch { cond, then_bb, else_bb } => {
            format!("branch v{} -> bb{}, bb{}", cond.0, then_bb.0, else_bb.0)
        }
        // Loud on purpose: an Unfinished terminator surviving to a dump is a
        // lowering bug, and it should scream in the snapshot, not hide
        Terminator::Unfinished => "!!! UNFINISHED !!!".to_string(),
    }
}
 
fn fmt_values(vs: &[ValueId]) -> String {
    vs.iter()
        .map(|v| format!("v{}", v.0))
        .collect::<Vec<_>>()
        .join(", ")
}
 
fn fmt_def(def: crate::analysis::DefId, a: &Analysis) -> String {
    a.symbols.resolve(a.definitions.info(def).name).to_string()
}
 
fn fmt_binop(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "add", Sub => "sub", Mul => "mul", Div => "div",
        Rem => "rem", Pow => "pow",
        Eq => "cmp.eq", NotEq => "cmp.ne",
        Lt => "cmp.lt", Gt => "cmp.gt", LtEq => "cmp.le", GtEq => "cmp.ge",
    }
}
 
fn fmt_unop(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "neg",
        UnOp::Not => "not",
    }
}

/// Performs a few checks:
/// 1) Every ValueId i used (in instructions and terminators), walking blocks in vec order
/// 2) No Terminator::Unfinished values
/// 3) Every jump target is a real BlockId
/// 4) Destination ValueIds are unique (each value defined exactly once)
pub fn verify(f: &Function) {
    // Number of values that exist
    let value_count = f.blocks
        .iter()
        .flat_map(|b| b.instrs.iter())
        .filter_map(|i| i.dest)
        .map(|v| v.0 as usize + 1)
        .max()
        .unwrap_or(0);

    let mut defined = vec![false; value_count];

    let check_uses = |defined: &[bool], used: &[ValueId], ctx: &str| {
        for v in used {
            assert!(
                (v.0 as usize) < defined.len() && defined[v.0 as usize],
                "TIR verify [{}]: {ctx}: use of undefined v{}",
                f.name,
                v.0
            )
        }
    };

    for (bi, b) in f.blocks.iter().enumerate() {
        // Checks (1) and (4)
        for (ii, ins) in b.instrs.iter().enumerate() {
            check_uses(
                &defined,
                &ins.kind.used_values(),
                &format!("bb{bi} instr {ii}")
            );

            if let Some(d) = ins.dest {
                assert!(
                    !defined[d.0 as usize],
                    "TIR verify [{}]: bb{bi} instr {ii}: v{} defined twice",
                    f.name,
                    d.0
                );

                defined[d.0 as usize] = true;
            }
        }

        // Terminator uses (branch conditions & return values)
        check_uses(&defined, &b.term.used_values(), &format!("bb{bi} terminator"));

        // Check (2)
        assert!(
            !matches!(b.term, Terminator::Unfinished),
            "TIR verify [{}]: bb{bi} was left Unfinished",
            f.name
        );

        // Check (3)
        for t in b.term.targets() {
            assert!(
                (t.0 as usize) < f.blocks.len(),
                "TIR verify [{}]: bb{bi} jumps to nonexistent bb{}",
                f.name,
                t.0
            );
        }
    }

    // Locals referenced must exist (cheap check while we are already busy)
    for (bi, b) in f.blocks.iter().enumerate() {
        for ins in &b.instrs {
            let local = match &ins.kind {
                InstrKind::LoadLocal(l) | InstrKind::StoreLocal(l, _) => Some(*l),
                _ => None
            };

            if let Some(l) = local {
                assert!(
                    (l.0 as usize) < f.locals.len(),
                    "TIR verify [{}]: bb{bi}: reference to nonexistent local %{}",
                    f.name,
                    l.0
                );
            }
        }
    }
}