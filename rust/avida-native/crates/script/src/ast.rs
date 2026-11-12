//! The abstract syntax tree (`ASTree.h`, `ASTree.cc`).
//!
//! Nodes are immutable `Rc<Node>` trees whose semantic annotations (types,
//! variable and function ids) live in `Cell`/`RefCell` fields, so the symbol
//! table can share function bodies with the main tree the way the C++
//! `cSymbolTable` stores raw pointers to them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::library::ASFunction;
use crate::types::{AsType, BuiltIn, Tok, TypeInfo};

/// Script text (identifiers, literals) is bytes, like `cString`.
pub type Bytes = Vec<u8>;

/// `cASFilePosition`.
#[derive(Clone, Debug)]
pub struct FilePos {
    pub filename: Rc<str>,
    pub line: i32,
}

pub type NodeRef = Rc<Node>;

/// `cASTNode`.
pub struct Node {
    pub pos: FilePos,
    /// `SuppressOutput()` flag (`;` terminated statement).
    pub suppressed: Cell<bool>,
    pub kind: Kind,
}

/// One entry of a literal dict (`cASTLiteralDict::sMapping`).
pub struct Mapping {
    pub idx: NodeRef,
    pub val: NodeRef,
}

/// One `else if` clause (`cASTIfBlock::cElseIf`).
pub struct ElseIf {
    pub cond: NodeRef,
    pub code: NodeRef,
}

/// One name of an unpack target (`cASTUnpackTarget::sUnpackNode`).
pub struct UnpackNode {
    pub name: Bytes,
    pub var_id: Cell<i32>,
    pub global: Cell<bool>,
    pub ty: RefCell<TypeInfo>,
}

pub enum Kind {
    /// Placeholder for a C++ NULL child in a tree whose parse failed.
    Null,
    Assignment {
        var: Bytes,
        expr: NodeRef,
        id: Cell<i32>,
        global: Cell<bool>,
    },
    ArgumentList(Vec<NodeRef>),
    ObjectAssignment {
        target: NodeRef,
        expr: NodeRef,
    },
    Return(NodeRef),
    StatementList(Vec<NodeRef>),
    Foreach {
        var: NodeRef,
        values: NodeRef,
        code: NodeRef,
    },
    If {
        cond: NodeRef,
        code: NodeRef,
        elifs: Vec<ElseIf>,
        else_code: Option<NodeRef>,
    },
    While {
        cond: NodeRef,
        code: NodeRef,
    },
    FunctionDefinition {
        ty: TypeInfo,
        name: Bytes,
        args: RefCell<Option<NodeRef>>,
        code: RefCell<Option<NodeRef>>,
    },
    VariableDefinition {
        ty: TypeInfo,
        name: Bytes,
        assign: Option<NodeRef>,
        dims: Option<NodeRef>,
        id: Cell<i32>,
    },
    VariableDefinitionList(Vec<NodeRef>),
    Binary {
        op: Tok,
        left: NodeRef,
        right: NodeRef,
        ty: RefCell<TypeInfo>,
        compare_ty: RefCell<TypeInfo>,
    },
    Unary {
        op: Tok,
        expr: NodeRef,
        ty: RefCell<TypeInfo>,
    },
    BuiltInCall {
        args: Option<NodeRef>,
        ty: RefCell<TypeInfo>,
        builtin: BuiltIn,
        target: Option<NodeRef>,
    },
    FunctionCall {
        name: Bytes,
        args: Option<NodeRef>,
        ty: RefCell<TypeInfo>,
        id: Cell<i32>,
        global: Cell<bool>,
        func: RefCell<Option<Rc<dyn ASFunction>>>,
    },
    Literal {
        ty: TypeInfo,
        value: Bytes,
    },
    LiteralArray {
        values: Option<NodeRef>,
        is_matrix: bool,
    },
    LiteralDict(Vec<Mapping>),
    ObjectCall {
        object: NodeRef,
        name: Bytes,
        args: Option<NodeRef>,
    },
    ObjectReference {
        object: NodeRef,
        name: Bytes,
    },
    VariableReference {
        name: Bytes,
        ty: RefCell<TypeInfo>,
        id: Cell<i32>,
        global: Cell<bool>,
    },
    UnpackTarget {
        nodes: Vec<UnpackNode>,
        last_wild: bool,
        last_named: bool,
        expr: NodeRef,
    },
}

impl Node {
    pub fn new(pos: FilePos, kind: Kind) -> NodeRef {
        Rc::new(Node {
            pos,
            suppressed: Cell::new(false),
            kind,
        })
    }

    /// A NULL child (only present in trees whose parse failed).
    pub fn null(pos: FilePos) -> NodeRef {
        Node::new(pos, Kind::Null)
    }

    pub fn is_null(&self) -> bool {
        matches!(self.kind, Kind::Null)
    }

    /// `GetType()`: the virtual with the base `s_invalid_type` default.
    pub fn get_type(&self) -> TypeInfo {
        match &self.kind {
            Kind::FunctionDefinition { ty, .. } => ty.clone(),
            Kind::VariableDefinition { ty, .. } => ty.clone(),
            Kind::Binary { ty, .. } => ty.borrow().clone(),
            Kind::Unary { ty, .. } => ty.borrow().clone(),
            Kind::BuiltInCall { ty, .. } => ty.borrow().clone(),
            Kind::FunctionCall { ty, .. } => ty.borrow().clone(),
            Kind::Literal { ty, .. } => ty.clone(),
            Kind::LiteralArray { is_matrix, .. } => TypeInfo::new(if *is_matrix {
                AsType::Matrix
            } else {
                AsType::Array
            }),
            Kind::LiteralDict(_) => TypeInfo::new(AsType::Dict),
            Kind::ObjectCall { .. } | Kind::ObjectReference { .. } => {
                TypeInfo::new(AsType::Runtime)
            }
            Kind::VariableReference { ty, .. } => ty.borrow().clone(),
            _ => TypeInfo::invalid(),
        }
    }
}

/// `cASTBuiltInCall::cASTBuiltInCall`: name to builtin id.
pub fn builtin_from_name(name: &[u8]) -> BuiltIn {
    match name {
        b"asbool" => BuiltIn::CastBool,
        b"aschar" => BuiltIn::CastChar,
        b"asint" => BuiltIn::CastInt,
        b"asfloat" => BuiltIn::CastFloat,
        b"asstring" => BuiltIn::CastString,
        b"isarray" => BuiltIn::IsArray,
        b"isbool" => BuiltIn::IsBool,
        b"ischar" => BuiltIn::IsChar,
        b"isdict" => BuiltIn::IsDict,
        b"isint" => BuiltIn::IsInt,
        b"isfloat" => BuiltIn::IsFloat,
        b"ismatrix" => BuiltIn::IsMatrix,
        b"isstring" => BuiltIn::IsString,
        b"clear" => BuiltIn::Clear,
        b"copy" => BuiltIn::Copy,
        b"haskey" => BuiltIn::HasKey,
        b"keys" => BuiltIn::Keys,
        b"len" => BuiltIn::Len,
        b"remove" => BuiltIn::Remove,
        b"resize" => BuiltIn::Resize,
        b"values" => BuiltIn::Values,
        _ => BuiltIn::Unknown,
    }
}
