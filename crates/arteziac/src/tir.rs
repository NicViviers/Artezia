use crate::analysis::{Analysis, DefId, TypeId};
use crate::ast::NodeId;

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

// TODO: Step 1.4 and lowering for TIR