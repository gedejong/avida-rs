//! Types, token kinds, exit codes and name tables of AvidaScript.
//!
//! Ported from `avida-core/source/script/AvidaScript.h` and `AvidaScript.cc`.

/// `eASTypes` (`ASType_t`). The discriminants index [`crate::semantic::VALID_CAST`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AsType {
    Array = 0,
    Bool,
    Char,
    Dict,
    Float,
    Int,
    ObjectRef,
    Matrix,
    String,
    Var,
    Runtime,
    Void,
    Invalid,
}

/// `sASTypeInfo`: a type plus the object class name for `ObjectRef`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TypeInfo {
    pub ty: AsType,
    pub info: String,
}

impl TypeInfo {
    pub fn new(ty: AsType) -> Self {
        TypeInfo {
            ty,
            info: String::new(),
        }
    }
    pub fn with_info(ty: AsType, info: &str) -> Self {
        TypeInfo {
            ty,
            info: info.to_string(),
        }
    }
    pub fn invalid() -> Self {
        TypeInfo::new(AsType::Invalid)
    }
    /// `AvidaScript::mapType`.
    pub fn map(&self) -> String {
        map_type(self)
    }
}

impl From<AsType> for TypeInfo {
    fn from(t: AsType) -> Self {
        TypeInfo::new(t)
    }
}

/// `ASToken_t`. `Eof` is the `0` returned by `yylex` at end of input and
/// `Invalid` the "no token buffered" marker of `cParser`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Eof = 0,
    Suppress,
    Endl,
    Comma,
    OpBitNot,
    OpBitAnd,
    OpBitOr,
    OpLogicNot,
    OpLogicAnd,
    OpLogicOr,
    OpAdd,
    OpSub,
    OpMul,
    OpDiv,
    OpMod,
    Dot,
    Assign,
    Ref,
    OpEq,
    OpLe,
    OpGe,
    OpLt,
    OpGt,
    OpNeq,
    PrecOpen,
    PrecClose,
    IdxOpen,
    IdxClose,
    ArrOpen,
    ArrClose,
    ArrRange,
    ArrExpan,
    ArrWild,
    LiteralDict,
    DictMapping,
    LiteralMatrix,
    TypeArray,
    TypeBool,
    TypeChar,
    TypeDict,
    TypeFloat,
    TypeInt,
    TypeMatrix,
    TypeString,
    TypeVar,
    TypeVoid,
    CmdIf,
    CmdElse,
    CmdElseIf,
    CmdWhile,
    CmdForeach,
    CmdFunction,
    CmdReturn,
    BuiltinCall,
    BuiltinMethod,
    Id,
    Float,
    Int,
    Str,
    Char,
    Bool,
    Unknown,
    Invalid,
}

/// `ASBuiltIn_t`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuiltIn {
    CastBool,
    CastChar,
    CastInt,
    CastFloat,
    CastString,
    IsArray,
    IsBool,
    IsChar,
    IsDict,
    IsInt,
    IsFloat,
    IsMatrix,
    IsString,
    Clear,
    Copy,
    HasKey,
    Keys,
    Len,
    Remove,
    Resize,
    Values,
    Unknown,
}

/// `eASExitCodes`.
pub const AS_EXIT_OK: i32 = 0;
pub const AS_EXIT_FILE_NOT_FOUND: i32 = 200;
pub const AS_EXIT_FAIL_PARSE: i32 = 201;
pub const AS_EXIT_FAIL_SEMANTIC: i32 = 202;
pub const AS_EXIT_FAIL_INTERPRET: i32 = 203;
pub const AS_EXIT_INTERNAL_ERROR: i32 = 204;
pub const AS_EXIT_UNKNOWN: i32 = 205;

/// `AvidaScript::mapBuiltIn`.
pub fn map_builtin(b: BuiltIn) -> &'static str {
    match b {
        BuiltIn::CastBool => "asbool",
        BuiltIn::CastChar => "aschar",
        BuiltIn::CastInt => "asint",
        BuiltIn::CastFloat => "asfloat",
        BuiltIn::CastString => "asstring",
        BuiltIn::IsArray => "is_array",
        BuiltIn::IsBool => "is_bool",
        BuiltIn::IsChar => "is_char",
        BuiltIn::IsDict => "is_dict",
        BuiltIn::IsInt => "is_int",
        BuiltIn::IsFloat => "is_float",
        BuiltIn::IsMatrix => "is_matrix",
        BuiltIn::IsString => "is_string",
        BuiltIn::Clear => "clear",
        BuiltIn::Copy => "copy",
        BuiltIn::HasKey => "haskey",
        BuiltIn::Keys => "keys",
        BuiltIn::Len => "len",
        BuiltIn::Remove => "remove",
        BuiltIn::Resize => "resize",
        BuiltIn::Values => "values",
        BuiltIn::Unknown => "?",
    }
}

/// `AvidaScript::mapToken`.
pub fn map_token(t: Tok) -> &'static str {
    match t {
        Tok::OpBitNot => "~",
        Tok::OpBitAnd => "&",
        Tok::OpBitOr => "|",
        Tok::OpLogicNot => "!",
        Tok::OpLogicAnd => "&&",
        Tok::OpLogicOr => "||",
        Tok::OpAdd => "+",
        Tok::OpSub => "-",
        Tok::OpMul => "*",
        Tok::OpDiv => "/",
        Tok::OpMod => "%",
        Tok::Dot => ".",
        Tok::OpEq => "==",
        Tok::OpLe => "<=",
        Tok::OpGe => ">=",
        Tok::OpLt => "<",
        Tok::OpGt => ">",
        Tok::OpNeq => "!=",
        Tok::ArrRange => ":",
        Tok::ArrExpan => "^",
        Tok::IdxOpen => "[]",
        _ => "?",
    }
}

/// `AvidaScript::mapType`.
pub fn map_type(t: &TypeInfo) -> String {
    match t.ty {
        AsType::Array => "array".into(),
        AsType::Bool => "bool".into(),
        AsType::Char => "char".into(),
        AsType::Dict => "dict".into(),
        AsType::Float => "float".into(),
        AsType::Int => "int".into(),
        AsType::Matrix => "matrix".into(),
        AsType::ObjectRef => t.info.clone(),
        AsType::String => "string".into(),
        AsType::Var => "var".into(),
        AsType::Runtime => "-runtime-".into(),
        AsType::Void => "void".into(),
        AsType::Invalid => "*INVALID*".into(),
    }
}

/// `mapType` of a bare type (empty `info`).
pub fn map_ty(t: AsType) -> String {
    map_type(&TypeInfo::new(t))
}
