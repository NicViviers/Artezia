use super::parser::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

// ============================================================================
// Top level
// ============================================================================

#[derive(Debug, PartialEq)]
pub struct File {
    pub items: Vec<Item>,
}

#[derive(Debug, PartialEq)]
pub enum Item {
    Func(Func),
    Import(Import)
    // v-next: Struct(Struct), Enum(Enum), Impl(Impl), TypeAlias(TypeAlias)
}

#[derive(Debug, PartialEq)]
pub struct Import {
    pub id: NodeId,
    /// `import std::io` -> [span("std"), span("io")]. Spans, not names - resolution interns them.
    pub path: Vec<Span>,
    pub span: Span,
}

#[derive(Debug, PartialEq)]
pub struct Func {
    pub id: NodeId,
    pub name_span: Span,          // the identifier's span; name interned later
    pub params: Vec<Param>,
    pub ret: Option<Type>,        // None = Unit
    pub body: Block,
    pub span: Span,               // `func` keyword through closing `}`
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub id: NodeId,               // params are DEFINITIONS → they need ids
                                  // (resolution creates a DefId per param)
    pub name_span: Span,
    pub ty: Type,                 // mandatory on params (per language ref)
    pub span: Span,
}

// ============================================================================
// Types (SYNTAX of types — not to be confused with the checker's TypeId/Type;
// this is what the user WROTE, the checker computes what it MEANS)
// ============================================================================

#[derive(Debug, PartialEq)]
pub enum Type {
    /// `Int`, `std::collections::HashMap` — a (possibly dotted) path.
    Named { id: NodeId, path: Vec<Span>, span: Span },
    // v-next:
    // Optional { id, inner: Box<Type>, span },          // T?
    // List     { id, elem: Box<Type>, span },           // [T]
    // Func     { id, params: Vec<Type>, ret: Option<Box<Type>>, span },
}

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Named { span, .. } => span.clone(),
            // future variants each carry span - add arms as they appear
        }
    }
}

// ============================================================================
// Statements & blocks
// ============================================================================

#[derive(Debug, PartialEq)]
pub struct Block {
    pub id: NodeId,               // blocks are scopes → resolution pushes/pops
                                  // on them, and blocks-as-expressions have types
    pub stmts: Vec<Stmt>,
    pub span: Span,               // `{` through `}`
}

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Let {
        id: NodeId,               // the stmt id doubles as the DEFINITION site id
        name_span: Span,
        ty: Option<Type>,         // the optional `: Type` annotation
        init: Expr,
        span: Span,
    },
    /// Any expression in statement position: calls, assignments, `if`, `scope{}`.
    Expr(Expr),
    While { id: NodeId, cond: Expr, body: Block, span: Span },
    For {
        id: NodeId,
        var_span: Span,           // loop variable — also a definition site
        iter: Expr,
        body: Block,
        span: Span,
    },
    Return { id: NodeId, value: Option<Expr>, span: Span },
    Break { id: NodeId, span: Span },
    Continue { id: NodeId, span: Span },
    // v-next: Defer { id, expr: Expr, span }
}

// ============================================================================
// Expressions
// ============================================================================

#[derive(Debug, PartialEq)]
pub enum Expr {
    // ---- literals: span-only; values decoded into Analysis.values later ----
    Int      { id: NodeId, span: Span },
    Float    { id: NodeId, span: Span },
    Str      { id: NodeId, span: Span },
    Char     { id: NodeId, span: Span },
    Duration { id: NodeId, span: Span },
    Bool     { id: NodeId, span: Span },

    /// A name in expression position. WHICH x? → Analysis.defs[id] after
    /// resolution.
    Var { id: NodeId, span: Span },

    Unary {
        id: NodeId,
        op: UnOp,
        rhs: Box<Expr>,
        span: Span,
    },
    
    Binary {
        id: NodeId,
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },

    /// Assignment is NOT a BinOp: its lhs is a *place* (validated in a later
    /// pass), it evaluates to Unit, and typeck treats it completely
    /// differently. Modeling it separately now saves un-tangling later.
    Assign {
        id: NodeId,
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },

    Call {
        id: NodeId,
        callee: Box<Expr>,        // an Expr, not a name: `f(x)(y)`, `a.b(c)`
        args: Vec<Arg>,
        span: Span,
    },

    Field {
        id: NodeId,
        obj: Box<Expr>,
        name_span: Span,
        span: Span,
    },

    Index {
        id: NodeId,
        obj: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },

    Range {
        id: NodeId,
        lo: Box<Expr>,
        hi: Box<Expr>,
        inclusive: bool,          // `..` vs `..=` — token identity, like Bool
        span: Span,
    },

    // ---- control flow (expressions in Artezia) ----
    If {
        id: NodeId,
        cond: Box<Expr>,
        then: Block,
        /// None = no else. Some(Expr) is either Expr::Block (plain else)
        /// or Expr::If (else-if chain) — one field handles both.
        els: Option<Box<Expr>>,
        span: Span,
    },
    Block(Block),                 // a bare `{ ... }` in expression position

    // ---- concurrency ----
    Scope  { id: NodeId, body: Block, span: Span },
    Spawn  { id: NodeId, call: Box<Expr>, span: Span },
    Within {
        id: NodeId,
        dur: Box<Expr>,
        body: Block,
        els: Option<Block>,
        span: Span,
    },

    /// Error-recovery hole: a diagnostic was already emitted; later passes
    /// skip these silently (type = Type::Error, codegen emits nothing).
    Error { id: NodeId, span: Span },
}

#[derive(Debug, PartialEq)]
pub struct Arg {
    pub id: NodeId,
    pub name_span: Option<Span>,  // Some for named args: `retry(attempts: 3)`
    pub value: Expr,
    pub span: Span,
}

// ============================================================================
// Operators — pure token identity, no spans needed (the Expr carries the span)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp { Neg, Not }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Rem, Pow,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

// ============================================================================
// The two accessors every pass needs. (This is the full extent of AST
// "methods" — anything smarter belongs in a pass.)
// ============================================================================

impl Expr {
    pub fn id(&self) -> NodeId {
        use Expr::*;
        match self {
            Int{id,..} | Float{id,..} | Str{id,..} | Char{id,..} | Duration{id,..}
            | Bool{id,..} | Var{id,..} | Unary{id,..} | Binary{id,..} | Assign{id,..}
            | Call{id,..} | Field{id,..} | Index{id,..} | Range{id,..} | If{id,..}
            | Scope{id,..} | Spawn{id,..} | Within{id,..} | Error{id,..} => *id,
            Block(b) => b.id,
        }
    }

    pub fn span(&self) -> Span {
        use Expr::*;
        match self {
            Int{span,..} | Float{span,..} | Str{span,..} | Char{span,..}
            | Duration{span,..} | Bool{span,..} | Var{span,..} | Unary{span,..}
            | Binary{span,..} | Assign{span,..} | Call{span,..} | Field{span,..}
            | Index{span,..} | Range{span,..} | If{span,..} | Scope{span,..}
            | Spawn{span,..} | Within{span,..} | Error{span,..} => span.clone(),
            Block(b) => b.span.clone(),
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        use Stmt::*;
        match self {
            Let{span,..} | While{span,..} | For{span,..} | Return{span,..}
            | Break{span,..} | Continue{span,..} => span.clone(),
            Expr(e) => e.span(),
        }
    }
}