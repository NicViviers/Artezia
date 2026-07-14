// TODO: This is unfinished and should be completed once AST step is done
// this is needed due to the compilation order: lexer (tokens) -> parser (ast) -> intermediate checks -> TIR -> LLVM IR
use std::collections::HashMap;
use super::ast::NodeId;

/// Symbols answer "is this the same name?"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(pub u32);

/// Reverse lookup for strings (Anything named "x" -> Symbol(5), strings[5] == "x" for reverse lookup)
pub struct Interner {
    map: HashMap<String, Symbol>,
    strings: Vec<String>
}

impl Interner {
    pub fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) { return sym };

        let sym = Symbol(self.strings.len() as u32);

        self.strings.push(s.to_owned());
        self.map.insert(s.to_owned(), sym);
        sym
    }

    pub fn resolve(&self, sym: Symbol) -> &str {
        &self.strings[sym.0 as usize]
    }
}

/// A unique number for one definition. "The 7th named thing in this program."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

/// WHAT KIND of thing was defined. Important because codegen emits different IR for each
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Func,
    Param,
    Local,
    Class,
    Import
}

/// Everything known about one definition
#[derive(Debug)]
pub struct DefInfo {
    pub kind: DefKind,
    pub name: Symbol,
    pub node: NodeId,
    // TODO: ty: Option<TypeId> soon?
}

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
}