//   lexer (tokens) -> parser (ast, spans + NodeIds, no meaning)
//          -> analysis stage (this file + the three passes)
//            pass A: name resolution -> fills defs, definitions, symbols
//            pass B: literal decoding -> fills values
//            pass C: type checking -> fills types, def_types, type_table
//          -> lowering (AST + Analysis -> TIR)
//          -> codegen (TIR -> LLVM IR)
//
// Every pass writes its conclusions into the tables below, keyed by NodeId.
// This file defines those tables and the id/catalog types they're built from.
// The recurring pattern everywhere:
// replace a big/variable-sized thing with a small Copy id + a table the id indexes into

use std::collections::HashMap;
use artezia_diag::Diagnostic;
use crate::parser::Span;
use crate::ast::NodeId;

/// An interned string. `Symbol(5)` means "the spelling that was interned 5th"
/// - e.g. the name "x". IMPORTANT: a Symbol is a spelling, not a variable.
/// Ten different variables all named `x` share one Symbol; telling them apart
/// is DefId's job. Symbol answers "same name?"; DefId answers "same thing?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(pub u32);

/// Deduplicating string table. Intern "x" once -> Symbol(5); every later
/// intern of "x" - from anywhere - returns the same Symbol(5). Name equality
/// becomes a u32 compare instead of a string compare.
#[derive(Default)]
pub struct Interner {
    map: HashMap<String, Symbol>,
    strings: Vec<String>,          // strings[sym.0] = the text (reverse lookup)
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) {
            return sym;
        }
        let sym = Symbol(self.strings.len() as u32);
        self.strings.push(s.to_owned());
        self.map.insert(s.to_owned(), sym);
        sym
    }

    /// For diagnostics: Symbol -> the actual text
    pub fn resolve(&self, sym: Symbol) -> &str {
        &self.strings[sym.0 as usize]
    }
}

/// A unique id for one defintion - any place the program introduces a named thing:
/// a function, a parameter, a `let` binding. Refers to "the 7th named thing"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

/// What type of thing was defined. Passes branch on this constantly:
/// you can't assign to a function; codegen emits params, locals and funcs completely differently
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Func,     // `func add(...)`
    Param,    // `a` in `func add(a: Int)`
    Local,    // `let x = ...` and `for x in ...` loop variables
    Import,   // v0: the name an `import` introduces
    // TODO: Struct, Field, EnumVariant, TypeAlias, ...
}

/// Everything known about a definition. One record per DefId, stored in definitions below
/// Created by pass A at each definition site. Read by:
///  - pass A itself (duplicate detection: "first defined here" labels)
///  - pass C (mutability checks on assignment; kind checks)
///  - codegen (DefKind decides emission strategy)
///  - every diagnostic that names a definition, forever.
#[derive(Debug)]
pub struct DefInfo {
    pub kind: DefKind,
    pub name: Symbol, // interned name
    pub node: NodeId, // the AST node that defined it
    pub name_span: Span, // the identifier's span
}

/// Pplain vector indexed by DefId to store definitions
#[derive(Default)]
pub struct Definitions {
    defs: Vec<DefInfo>,
}

impl Definitions {
    pub fn create(&mut self, info: DefInfo) -> DefId {
        self.defs.push(info);
        DefId(self.defs.len() as u32 - 1)
    }

    pub fn info(&self, id: DefId) -> &DefInfo {
        &self.defs[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }
}

/// A handle to one distinct type. Because the TypeTable dedupes, two TypeIds
/// are equal and identical (u32 compare, no tree walking)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// The structure of a type 0 the semantic kind, not to be confused with ast::Type (which is the syntax the user wrote;
/// `ast::Type::Named("Int")` gets lowered to `Type::Int` here by the checker's lower_type)
/// Note recursion goes through TypeId, not Box<Type>: children are table references
/// Keeps Type small, hashable, and every distinct type stored exactly once
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Int,
    Float,
    Bool,
    Str,
    Char,
    Duration,
    Unit,
    Func { params: Vec<TypeId>, ret: TypeId },
    /// When an expression can't be typed it gets Error, and every rule accepts Error silently: Error + Int = Error, NO new diagnostic
    Error,
    // TODO: Struct(DefId), List(TypeId), Optional(TypeId), Range, ...
}

/// The type catalog: interning table like the Interner, for types
#[derive(Default)]
pub struct TypeTable {
    types: Vec<Type>,
    lookup: HashMap<Type, TypeId>,
}

impl TypeTable {
    pub fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(&id) = self.lookup.get(&ty) {
            return id;
        }
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty.clone());
        self.lookup.insert(ty, id);
        id
    }

    pub fn get(&self, id: TypeId) -> &Type {
        &self.types[id.0 as usize]
    }

    /// Render for diagnostics: "Int", "func(Int, Bool) -> Unit".
    pub fn display(&self, id: TypeId) -> String {
        match self.get(id) {
            Type::Int => "Int".into(),
            Type::Float => "Float".into(),
            Type::Bool => "Bool".into(),
            Type::Str => "String".into(),
            Type::Char => "Char".into(),
            Type::Duration => "Duration".into(),
            Type::Unit => "Unit".into(),
            Type::Error => "<error>".into(),
            Type::Func { params, ret } => {
                let ps: Vec<String> =
                    params.iter().map(|p| self.display(*p)).collect();
                format!("func({}) -> {}", ps.join(", "), self.display(*ret))
            }
        }
    }
}

/// Pre-interned primitives so common checks are constant comparisons (`ty == prims.boolean`) with no interning on hot paths
/// Built once in Analysis::new(); the fixed interning order gives stable TypeIds
#[derive(Debug, Clone, Copy)]
pub struct Prims {
    pub int: TypeId,
    pub float: TypeId,
    pub boolean: TypeId,
    pub str_: TypeId,
    pub char_: TypeId,
    pub duration: TypeId,
    pub unit: TypeId,
    pub error: TypeId,
}

/// The decoded value of a literal. Exists because parser is span-only
/// `Expr::Int { span }` carries no number - pass B extracts &src[span] and decodes it here
/// Content errors ("integer too large","unknown escape \q") are diagnosed by pass B with the node's span;
/// on error it stores a sane default so the table always has an entry for every literal node (codegen never handles "missing")
/// Read by codegen (constants) and, later, anything doing const-eval
#[derive(Debug, Clone)]
pub enum LitValue {
    Int(i64),
    Float(f64),
    Str(String),        // escape sequences already processed
    Char(char),
    Duration(u64),      // normalized to NANOSECONDS: "5s" → 5_000_000_000
    Bool(bool),
}

/// The output of the analysis stage alongside the AST
/// It is the input to lowering and codegen. Two kinds of content:
///
///  CATALOGS - append-only stores that give ids meaning:
///    symbols (Symbol -> text), definitions (DefId -> DefInfo) and type_table (TypeId -> Type)
///
///  FACT TABLES — NodeId-keyed conclusions about the tree:
///    defs, values, types (+ def_types, keyed by DefId)
pub struct Analysis {
    // catalogs
    pub symbols: Interner,
    pub definitions: Definitions,
    pub type_table: TypeTable,
    pub prims: Prims,

    // fact tables
    /// Use-site OR definition-site node -> DefId.
    /// CONVENTION (pass A must follow it): `declare()` also inserts the
    /// DEFINING node's entry (defs[func_node] = its own DefId), so pass C
    /// can find "this Func item's DefId" in one lookup. A Var node with no
    /// entry here = resolution failed and already diagnosed (poison rule)
    pub defs: HashMap<NodeId, DefId>,

    /// Literal node -> decoded value. Filled by pass B; total over literals.
    pub values: HashMap<NodeId, LitValue>,

    /// Every expression node -> its type. Filled by pass C; total over exprs
    /// (Error-typed where broken). Codegen reads this for all of them
    pub types: HashMap<NodeId, TypeId>,

    /// Definition -> its type (a Local's inferred type, a Func's signature)
    pub def_types: HashMap<DefId, TypeId>,
}

impl Analysis {
    pub fn new() -> Self {
        let mut type_table = TypeTable::default();
        let prims = Prims {
            int: type_table.intern(Type::Int),
            float: type_table.intern(Type::Float),
            boolean: type_table.intern(Type::Bool),
            str_: type_table.intern(Type::Str),
            char_: type_table.intern(Type::Char),
            duration: type_table.intern(Type::Duration),
            unit: type_table.intern(Type::Unit),
            error: type_table.intern(Type::Error),
        };
        Analysis {
            symbols: Interner::default(),
            definitions: Definitions::default(),
            type_table,
            prims,
            defs: HashMap::new(),
            values: HashMap::new(),
            types: HashMap::new(),
            def_types: HashMap::new(),
        }
    }
}

/// Stage entry point — what the pipeline calls between parsing and lowering.
/// The three passes live in their own modules (resolve.rs, literals.rs,
/// typeck.rs) and are built in that order; see the analysis guide.
pub fn analyze(
    file: &crate::ast::File,
    src: &str,
) -> (Analysis, Vec<Diagnostic>) {
    let mut a = Analysis::new();
    let mut diags = Vec::new();
    crate::passes::resolve::resolve(file, src, &mut a, &mut diags);       // pass A
    // crate::literals::decode_literals(file, src, &mut a, &mut diags); // pass B
    // crate::typeck::typecheck(file, &mut a, &mut diags);           // pass C
    (a, diags)
}

// ============================================================================
// Sanity tests for the foundations themselves (the passes get their own)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interner_dedupes() {
        let mut i = Interner::default();
        let a = i.intern("x");
        let b = i.intern("y");
        let c = i.intern("x");
        assert_eq!(a, c);
        assert_ne!(a, b);
        assert_eq!(i.resolve(a), "x");
    }

    #[test]
    fn type_table_dedupes_structurally() {
        let mut t = TypeTable::default();
        let int = t.intern(Type::Int);
        let f1 = t.intern(Type::Func { params: vec![int], ret: int });
        let f2 = t.intern(Type::Func { params: vec![int], ret: int });
        assert_eq!(f1, f2);                       // same structure = same id
        assert_eq!(t.display(f1), "func(Int) -> Int");
    }

    #[test]
    fn prims_are_stable() {
        let a = Analysis::new();
        assert_eq!(a.prims.int, TypeId(0));       // fixed interning order
        assert_eq!(a.type_table.display(a.prims.unit), "Unit");
    }
}