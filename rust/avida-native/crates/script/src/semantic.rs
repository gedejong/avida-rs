//! `cSemanticASTVisitor`: type checking and symbol resolution.
//!
//! Every diagnostic carries the C++ `__LINE__` of the originating
//! `SEMANTIC_ERROR`/`SEMANTIC_WARNING` site (`cSemanticASTVisitor.cc` of the
//! tree in `avida-core/source/script`), because the reference prints it.

use std::rc::Rc;

use crate::ast::*;
use crate::library::ASLibrary;
use crate::output;
use crate::symtab::{SymRef, SymbolTable};
use crate::types::{map_builtin, map_token, map_ty, map_type, AsType, BuiltIn, Tok, TypeInfo};

/// `valid_cast[in][out]`, indexed by [`AsType`] discriminants.
pub static VALID_CAST: [[bool; 13]; 13] = {
    const T: bool = true;
    const F: bool = false;
    [
        // ARRAY, BOOL , CHAR , DICT , FLOAT, INT  , @OBJ , MATRX, STRNG, VAR  , RUNTM, VOID , INVLD
        [T, T, F, T, F, F, F, F, T, T, T, F, F], // ARRAY
        [F, T, T, F, T, T, F, T, T, T, T, F, F], // BOOL
        [F, T, T, F, T, T, F, T, T, T, T, F, F], // CHAR
        [F, F, F, T, F, F, F, F, T, T, T, F, F], // DICT
        [F, T, F, F, T, T, F, T, T, T, T, F, F], // FLOAT
        [F, T, T, F, T, T, F, T, T, T, T, F, F], // INT
        [F, T, F, F, F, F, T, F, T, T, F, F, F], // OBJECT_REF
        [T, T, F, F, F, F, F, T, T, T, T, F, F], // MATRIX
        [T, T, F, F, T, T, F, F, T, T, T, F, F], // STRING
        [T, T, T, T, T, T, T, T, T, T, T, F, F], // VAR
        [T, T, T, T, T, T, T, T, T, T, T, F, F], // RUNTIME
        [F, F, F, F, F, F, F, F, F, F, F, F, F], // VOID
        [F, F, F, F, F, F, F, F, F, F, F, F, F], // INVALID
    ]
};

pub fn valid_cast(i: AsType, o: AsType) -> bool {
    VALID_CAST[i as usize][o as usize]
}

/// Diagnostics (`ASSemanticError_t` with arguments).
enum SemErr {
    LossOfPrecision(String, String),
    NoDimensions,
    NoReturn,
    Unreachable,
    ArgumentDefaultRequired(Bytes, Bytes),
    ArgumentMissingRequired(Bytes),
    BuiltinCallSignatureMismatch(String),
    CannotCast(String, String),
    CannotCompare,
    CannotOverrideLibFunction(Bytes),
    FunctionCallSignatureMismatch(Bytes),
    FunctionDefaultCallInvalid,
    FunctionDefaultVariableRefInvalid,
    FunctionRedefinition(Bytes),
    FunctionRtypeMismatch(Bytes),
    FunctionSignatureMismatch(Bytes),
    FunctionUndeclared(Bytes),
    FunctionUndefined(Bytes),
    InvalidAssignmentTarget,
    InvalidCharLiteral,
    TooManyArguments,
    UndefinedTypeOp(String, String),
    UnpackWildNonarray(Bytes),
    VariableDimensionsInvalid(Bytes, String),
    VariableUndefined(Bytes),
    VariableRedefinition(Bytes),
    Internal,
}

/// `__LINE__` values of the C++ sites. Named after the construct.
mod ln {
    pub const ASSIGN_CAST: u32 = 93;
    pub const ASSIGN_UNDEF: u32 = 96;
    pub const ARGLIST_INTERNAL: u32 = 104;
    pub const OBJASSIGN_TARGET: u32 = 114;
    pub const RETURN_CAST: u32 = 124;
    pub const UNREACHABLE: u32 = 149;
    pub const STMT_FUNC_UNDEF: u32 = 157;
    pub const FOREACH_CAST: u32 = 167;
    pub const FOREACH_FUNC_UNDEF: u32 = 180;
    pub const IF_COND: u32 = 189;
    pub const ELIF_COND: u32 = 198;
    pub const WHILE_COND: u32 = 210;
    pub const FUNDEF_LIB: u32 = 219;
    pub const FUNDEF_DEFAULT_REQ: u32 = 249;
    pub const FUNDEF_RTYPE: u32 = 262;
    pub const FUNDEF_SIG1: u32 = 279;
    pub const FUNDEF_SIG2: u32 = 284;
    pub const FUNDEF_SIG3: u32 = 287;
    pub const FUNDEF_NO_RETURN: u32 = 297;
    pub const FUNDEF_REDEF: u32 = 303;
    pub const FUNDEF_FUNC_UNDEF: u32 = 309;
    pub const VARDEF_REDEF: u32 = 325;
    pub const VARDEF_DIM_CAST: u32 = 336;
    pub const VARDEF_NO_DIMS: u32 = 340;
    pub const VARDEF_TOO_MANY: u32 = 345;
    pub const VARDEF_DIMS_INVALID1: u32 = 346;
    pub const VARDEF_DIMS_INVALID2: u32 = 349;
    pub const VARDEF_ASSIGN_CAST: u32 = 357;
    pub const VARDEFLIST_INTERNAL: u32 = 365;
    pub const IDX_CAST: u32 = 381;
    pub const IDX_UNDEF: u32 = 392;
    pub const RANGE_L: u32 = 398;
    pub const RANGE_R: u32 = 399;
    pub const EXPAN_R: u32 = 403;
    pub const BIT_L: u32 = 413;
    pub const BIT_R: u32 = 417;
    pub const LOGIC_L: u32 = 426;
    pub const LOGIC_R: u32 = 427;
    pub const EQ_CMP: u32 = 441;
    pub const REL_CMP: u32 = 453;
    pub const ADD_L: u32 = 463;
    pub const ADD_R: u32 = 464;
    pub const SUBMUL_L: u32 = 473;
    pub const SUBMUL_R: u32 = 474;
    pub const DIVMOD_L: u32 = 483;
    pub const DIVMOD_R: u32 = 484;
    pub const BIN_INTERNAL1: u32 = 489;
    pub const BIN_INTERNAL2: u32 = 494;
    pub const UN_BITNOT: u32 = 508;
    pub const UN_LOGIC: u32 = 512;
    pub const UN_SUB: u32 = 526;
    pub const UN_INTERNAL: u32 = 532;
    pub const BCAST_MISMATCH: u32 = 541;
    pub const BCAST_CAST: u32 = 545;
    pub const IS_MISMATCH: u32 = 572;
    pub const CLEAR_TYPE: u32 = 584;
    pub const CLEAR_ARGS: u32 = 586;
    pub const COPY_TYPE: u32 = 595;
    pub const COPY_ARGS: u32 = 597;
    pub const HASKEY_CAST: u32 = 603;
    pub const HASKEY_ARGS: u32 = 604;
    pub const KEYS_CAST: u32 = 615;
    pub const KEYS_ARGS: u32 = 616;
    pub const LEN_CAST: u32 = 622;
    pub const LEN_ARGS: u32 = 623;
    pub const RM_TYPE: u32 = 637;
    pub const RM_ARR_CAST: u32 = 643;
    pub const RM_ARR_MISMATCH: u32 = 645;
    pub const RM_DICT_MISMATCH: u32 = 652;
    pub const RM_MAT_CAST1: u32 = 659;
    pub const RM_MAT_CAST2: u32 = 662;
    pub const RM_MAT_MISMATCH: u32 = 664;
    pub const BUILTIN_INTERNAL: u32 = 674;
    pub const FCALL_DEFAULT: u32 = 685;
    pub const FCALL_LIB_PREC: u32 = 708;
    pub const FCALL_LIB_SIG: u32 = 711;
    pub const FCALL_LIB_CAST: u32 = 714;
    pub const FCALL_LIB_ARITY: u32 = 718;
    pub const FCALL_USR_PREC: u32 = 743;
    pub const FCALL_USR_SIG: u32 = 746;
    pub const FCALL_USR_CAST: u32 = 749;
    pub const FCALL_USR_MISSING: u32 = 754;
    pub const FCALL_USR_SIG2: u32 = 756;
    pub const FCALL_USR_SIG3: u32 = 760;
    pub const FCALL_UNDECLARED: u32 = 773;
    pub const LITERAL_CHAR: u32 = 781;
    pub const OBJCALL_CAST: u32 = 813;
    pub const OBJREF_CAST: u32 = 826;
    pub const VARREF_DEFAULT: u32 = 833;
    pub const VARREF_UNDEF: u32 = 843;
    pub const UNPACK_CAST: u32 = 853;
    pub const UNPACK_UNDEF: u32 = 862;
    pub const UNPACK_WILD: u32 = 869;
    pub const POSTCHECK_FUNC_UNDEF: u32 = 880;
}

fn args_of(n: &Option<NodeRef>) -> Option<&Vec<NodeRef>> {
    match n {
        Some(a) => match &a.kind {
            Kind::ArgumentList(v) => Some(v),
            _ => None,
        },
        None => None,
    }
}

fn is_ty(t: &TypeInfo, a: AsType) -> bool {
    t.ty == a && t.info.is_empty()
}

pub struct SemanticVisitor<'a> {
    library: &'a ASLibrary,
    global: SymRef,
    parent_scope: SymRef,
    fun_id: i32,
    cur: SymRef,
    fun_stack: Vec<(SymRef, i32)>,
    success: bool,
    fun_def: bool,
    fun_def_arg: bool,
    top_level: bool,
    obj_assign: bool,
    main_pos: FilePos,
}

impl<'a> SemanticVisitor<'a> {
    /// `cSemanticASTVisitor(lib, global_symtbl, main)`.
    pub fn new(lib: &'a ASLibrary, global: SymRef, main: &NodeRef) -> Self {
        {
            let mut g = global.borrow_mut();
            let _ = g.add_function(b"__asmain", &TypeInfo::new(AsType::Int));
            g.set_function_definition(0, Some(main.clone()));
        }
        SemanticVisitor {
            library: lib,
            parent_scope: global.clone(),
            cur: global.clone(),
            global,
            fun_id: 0,
            fun_stack: Vec::new(),
            success: true,
            fun_def: false,
            fun_def_arg: false,
            top_level: true,
            obj_assign: false,
            main_pos: main.pos.clone(),
        }
    }

    pub fn was_successful(&self) -> bool {
        self.success
    }

    // ---- diagnostics --------------------------------------------------------

    fn report(&mut self, e: SemErr, fp: &FilePos, line: u32) {
        let warn = matches!(
            e,
            SemErr::LossOfPrecision(..)
                | SemErr::NoDimensions
                | SemErr::NoReturn
                | SemErr::Unreachable
        );
        let mut m: Vec<u8> = Vec::new();
        m.extend_from_slice(fp.filename.as_bytes());
        m.push(b':');
        m.extend_from_slice(fp.line.to_string().as_bytes());
        if warn {
            m.extend_from_slice(b": warning: ");
        } else {
            self.success = false;
            m.extend_from_slice(b": error: ");
        }
        let suffix = format!("  (cSemanticASTVisitor.cc:{line})\n");
        let mut body: Vec<u8> = Vec::new();
        let b = |body: &mut Vec<u8>, s: &str| body.extend_from_slice(s.as_bytes());
        match &e {
            SemErr::LossOfPrecision(t1, t2) => b(
                &mut body,
                &format!("loss of precision occuring in cast of {t1} to {t2}"),
            ),
            SemErr::NoDimensions => b(&mut body, "no dimensions specified"),
            SemErr::NoReturn => b(&mut body, "control reaches the end of non-void function"),
            SemErr::Unreachable => b(&mut body, "unreachable statement(s)"),
            SemErr::ArgumentDefaultRequired(a, f) => {
                b(&mut body, "'");
                body.extend_from_slice(a);
                b(&mut body, "' argument of '");
                body.extend_from_slice(f);
                b(&mut body, "()' requires a default value");
            }
            SemErr::ArgumentMissingRequired(a) => {
                b(&mut body, "required argument ");
                body.extend_from_slice(a);
                b(&mut body, " not found");
            }
            SemErr::BuiltinCallSignatureMismatch(n) => b(
                &mut body,
                &format!("invalid invocation of builtin method '{n}()'"),
            ),
            SemErr::CannotCast(t1, t2) => b(&mut body, &format!("cannot cast {t1} to {t2}")),
            SemErr::CannotCompare => b(&mut body, "cannot compare values"),
            SemErr::CannotOverrideLibFunction(n) => {
                b(&mut body, "cannot override library method '");
                body.extend_from_slice(n);
                b(&mut body, "()'");
            }
            SemErr::FunctionCallSignatureMismatch(n) => {
                b(&mut body, "invalid call signature for '");
                body.extend_from_slice(n);
                b(&mut body, "()'");
            }
            SemErr::FunctionDefaultCallInvalid => {
                b(&mut body, "function call invalid in default value")
            }
            SemErr::FunctionDefaultVariableRefInvalid => {
                b(&mut body, "variable reference invalid in default value")
            }
            SemErr::FunctionRedefinition(n) => {
                b(&mut body, "redefinition of '");
                body.extend_from_slice(n);
                b(&mut body, "()'");
            }
            SemErr::FunctionRtypeMismatch(n) => {
                b(&mut body, "return type of '");
                body.extend_from_slice(n);
                b(&mut body, "()' does not match declaration");
            }
            SemErr::FunctionSignatureMismatch(n) => {
                b(&mut body, "call signature of '");
                body.extend_from_slice(n);
                b(&mut body, "()' does not match declaration");
            }
            SemErr::FunctionUndeclared(n) => {
                b(&mut body, "'");
                body.extend_from_slice(n);
                b(&mut body, "()' undeclared");
                let nm1 = self.cur.borrow().function_near_match(n);
                let nm2 = self.global.borrow().function_near_match(n);
                if !nm1.is_empty() {
                    b(&mut body, " - possible match '");
                    body.extend_from_slice(&nm1);
                    b(&mut body, "()'");
                }
                if !nm2.is_empty() && nm1 != nm2 {
                    b(&mut body, " or '");
                    body.extend_from_slice(&nm2);
                    b(&mut body, "()'");
                }
            }
            SemErr::FunctionUndefined(n) => {
                b(&mut body, "'");
                body.extend_from_slice(n);
                b(&mut body, "()' declared but not defined");
            }
            SemErr::InvalidAssignmentTarget => b(&mut body, "invalid assignment target"),
            SemErr::InvalidCharLiteral => b(&mut body, "invalid char literal"),
            SemErr::TooManyArguments => b(&mut body, "too many arguments"),
            SemErr::UndefinedTypeOp(op, t) => b(
                &mut body,
                &format!("'{op}' operation undefined for type '{t}'"),
            ),
            SemErr::UnpackWildNonarray(n) => {
                b(&mut body, "cannot unpack .. items into '");
                body.extend_from_slice(n);
                b(&mut body, "', variable must be an array");
            }
            SemErr::VariableDimensionsInvalid(v, t) => {
                b(&mut body, "dimensions of '");
                body.extend_from_slice(v);
                b(&mut body, &format!("' invalid for type {t}"));
            }
            SemErr::VariableUndefined(n) => {
                b(&mut body, "'");
                body.extend_from_slice(n);
                b(&mut body, "' undefined");
                let nm = self.cur.borrow().variable_near_match(n);
                if !nm.is_empty() {
                    b(&mut body, " - possible match '");
                    body.extend_from_slice(&nm);
                    b(&mut body, "'");
                }
            }
            SemErr::VariableRedefinition(n) => {
                b(&mut body, "redefining variable '");
                body.extend_from_slice(n);
                b(&mut body, "'");
            }
            SemErr::Internal => {
                b(
                    &mut body,
                    &format!("internal semantic analysis error at cSemanticASTVisitor.cc:{line}\n"),
                );
                m.extend_from_slice(&body);
                output::err_write(&m);
                return;
            }
        }
        m.extend_from_slice(&body);
        m.extend_from_slice(suffix.as_bytes());
        output::err_write(&m);
    }

    /// The `checkCast(in, out)` macro.
    fn check_cast(&mut self, in_t: &TypeInfo, out_t: &TypeInfo, fp: &FilePos, line: u32) {
        if valid_cast(in_t.ty, out_t.ty) {
            if (in_t.ty == AsType::Float && out_t.ty == AsType::Int)
                || (in_t.ty == AsType::Int && out_t.ty == AsType::Char)
            {
                self.report(
                    SemErr::LossOfPrecision(map_type(in_t), map_type(out_t)),
                    fp,
                    line,
                );
            }
        } else {
            self.report(
                SemErr::CannotCast(map_type(in_t), map_type(out_t)),
                fp,
                line,
            );
        }
    }

    fn lookup_variable(&self, name: &[u8]) -> Option<(i32, bool)> {
        if let Some(id) = self.cur.borrow().lookup_variable(name) {
            return Some((id, false));
        }
        if !Rc::ptr_eq(&self.cur, &self.global) {
            if let Some(id) = self.global.borrow().lookup_variable(name) {
                return Some((id, true));
            }
        }
        None
    }

    fn lookup_function(&self, name: &[u8]) -> Option<(i32, bool)> {
        if let Some(id) = self.cur.borrow().lookup_function(name) {
            return Some((id, false));
        }
        if !Rc::ptr_eq(&self.cur, &self.global) {
            if let Some(id) = self.global.borrow().lookup_function(name) {
                return Some((id, true));
            }
        }
        None
    }

    fn sym(&self, global: bool) -> SymRef {
        if global {
            self.global.clone()
        } else {
            self.cur.clone()
        }
    }

    fn function_undefined_check(&mut self, symtbl: &SymRef, fp: &FilePos, line: u32) {
        let fns = symtbl.borrow().active_functions();
        for (has_code, name) in fns {
            if !has_code {
                self.report(SemErr::FunctionUndefined(name), fp, line);
            }
        }
    }

    // ---- visitors -----------------------------------------------------------

    pub fn visit(&mut self, node: &NodeRef) {
        let fp = node.pos.clone();
        match &node.kind {
            Kind::Null => {}
            Kind::Assignment {
                var,
                expr,
                id,
                global,
            } => {
                self.visit(expr);
                match self.lookup_variable(var) {
                    Some((var_id, g)) => {
                        let vt = self.sym(g).borrow().variable_type(var_id);
                        self.check_cast(&expr.get_type(), &vt, &fp, ln::ASSIGN_CAST);
                        id.set(var_id);
                        global.set(g);
                    }
                    None => self.report(
                        SemErr::VariableUndefined(var.clone()),
                        &fp,
                        ln::ASSIGN_UNDEF,
                    ),
                }
            }
            Kind::ArgumentList(_) => self.report(SemErr::Internal, &fp, ln::ARGLIST_INTERNAL),
            Kind::ObjectAssignment { target, expr } => {
                self.obj_assign = true;
                self.visit(target);
                self.obj_assign = false;

                if target.get_type().ty != AsType::ObjectRef {
                    self.report(SemErr::InvalidAssignmentTarget, &fp, ln::OBJASSIGN_TARGET);
                }
                self.visit(expr);
            }
            Kind::Return(expr) => {
                self.visit(expr);
                let rt = self.parent_scope.borrow().function_rtype(self.fun_id);
                self.check_cast(&rt, &expr.get_type(), &fp, ln::RETURN_CAST);
                self.cur.borrow_mut().set_scope_return();
            }
            Kind::StatementList(stmts) => self.visit_statement_list(&fp, stmts),
            Kind::Foreach { var, values, code } => {
                self.visit(values);
                self.check_cast(
                    &values.get_type(),
                    &TypeInfo::new(AsType::Array),
                    &fp,
                    ln::FOREACH_CAST,
                );

                self.cur.borrow_mut().push_scope();
                self.visit(var);

                self.top_level = true;
                self.visit(code);

                let cur = self.cur.clone();
                self.function_undefined_check(&cur, &fp, ln::FOREACH_FUNC_UNDEF);
                self.cur.borrow_mut().pop_scope();
            }
            Kind::If {
                cond,
                code,
                elifs,
                else_code,
            } => {
                self.visit(cond);
                self.check_cast(
                    &cond.get_type(),
                    &TypeInfo::new(AsType::Bool),
                    &fp,
                    ln::IF_COND,
                );
                self.visit(code);
                for ei in elifs {
                    self.visit(&ei.cond);
                    self.check_cast(
                        &ei.cond.get_type(),
                        &TypeInfo::new(AsType::Bool),
                        &fp,
                        ln::ELIF_COND,
                    );
                    self.visit(&ei.code);
                }
                if let Some(ec) = else_code {
                    self.visit(ec);
                }
            }
            Kind::While { cond, code } => {
                self.visit(cond);
                self.check_cast(
                    &cond.get_type(),
                    &TypeInfo::new(AsType::Bool),
                    &fp,
                    ln::WHILE_COND,
                );
                self.visit(code);
            }
            Kind::FunctionDefinition {
                ty,
                name,
                args,
                code,
            } => self.visit_function_definition(&fp, ty, name, args, code),
            Kind::VariableDefinition {
                ty,
                name,
                assign,
                dims,
                id,
            } => {
                let added = self.cur.borrow_mut().add_variable(name, ty);
                match added {
                    Ok(var_id) => id.set(var_id),
                    Err(()) => self.report(
                        SemErr::VariableRedefinition(name.clone()),
                        &fp,
                        ln::VARDEF_REDEF,
                    ),
                }

                // Process matrix/array dimensions
                if let Some(al) = dims {
                    let items = args_of(&Some(al.clone())).cloned().unwrap_or_default();
                    if ty.ty == AsType::Matrix || ty.ty == AsType::Array {
                        for an in &items {
                            self.visit(an);
                            self.check_cast(
                                &an.get_type(),
                                &TypeInfo::new(AsType::Int),
                                &fp,
                                ln::VARDEF_DIM_CAST,
                            );
                        }
                        if items.is_empty() {
                            self.report(SemErr::NoDimensions, &fp, ln::VARDEF_NO_DIMS);
                        }
                        if (ty.ty == AsType::Array && items.len() > 1)
                            || (ty.ty == AsType::Matrix && items.len() > 2)
                        {
                            self.report(SemErr::TooManyArguments, &fp, ln::VARDEF_TOO_MANY);
                            self.report(
                                SemErr::VariableDimensionsInvalid(name.clone(), map_type(ty)),
                                &fp,
                                ln::VARDEF_DIMS_INVALID1,
                            );
                        }
                    } else {
                        self.report(
                            SemErr::VariableDimensionsInvalid(name.clone(), map_type(ty)),
                            &fp,
                            ln::VARDEF_DIMS_INVALID2,
                        );
                    }
                }

                if let Some(ae) = assign {
                    self.visit(ae);
                    self.check_cast(&ae.get_type(), ty, &fp, ln::VARDEF_ASSIGN_CAST);
                }
            }
            Kind::VariableDefinitionList(_) => {
                self.report(SemErr::Internal, &fp, ln::VARDEFLIST_INTERNAL)
            }
            Kind::Binary {
                op,
                left,
                right,
                ty,
                compare_ty,
            } => self.visit_binary(&fp, *op, left, right, ty, compare_ty),
            Kind::Unary { op, expr, ty } => {
                self.visit(expr);
                match op {
                    Tok::OpBitNot => {
                        if valid_bitwise(&expr.get_type()) {
                            *ty.borrow_mut() = expr.get_type();
                        } else {
                            self.report(
                                SemErr::UndefinedTypeOp(
                                    map_token(Tok::OpBitNot).into(),
                                    map_type(&expr.get_type()),
                                ),
                                &fp,
                                ln::UN_BITNOT,
                            );
                        }
                    }
                    Tok::OpLogicNot => {
                        self.check_cast(
                            &expr.get_type(),
                            &TypeInfo::new(AsType::Bool),
                            &fp,
                            ln::UN_LOGIC,
                        );
                        *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
                    }
                    Tok::OpSub => match expr.get_type().ty {
                        AsType::Char | AsType::Float | AsType::Int | AsType::Runtime => {
                            *ty.borrow_mut() = expr.get_type();
                        }
                        _ => self.report(
                            SemErr::UndefinedTypeOp(
                                map_token(Tok::OpSub).into(),
                                map_type(&expr.get_type()),
                            ),
                            &fp,
                            ln::UN_SUB,
                        ),
                    },
                    _ => self.report(SemErr::Internal, &fp, ln::UN_INTERNAL),
                }
            }
            Kind::BuiltInCall {
                args,
                ty,
                builtin,
                target,
            } => self.visit_builtin(&fp, args, ty, *builtin, target),
            Kind::FunctionCall {
                name,
                args,
                ty,
                id,
                global,
                func,
            } => self.visit_function_call(&fp, name, args, ty, id, global, func),
            Kind::Literal { ty, value } => {
                if is_ty(ty, AsType::Char) && value.len() != 1 {
                    self.report(SemErr::InvalidCharLiteral, &fp, ln::LITERAL_CHAR);
                }
            }
            Kind::LiteralArray { values, .. } => {
                if let Some(al) = args_of(values) {
                    for n in al {
                        self.visit(n);
                    }
                }
            }
            Kind::LiteralDict(maps) => {
                for m in maps {
                    self.visit(&m.idx);
                    self.visit(&m.val);
                }
            }
            Kind::ObjectCall { object, args, .. } => {
                self.visit(object);
                self.check_cast(
                    &object.get_type(),
                    &TypeInfo::new(AsType::ObjectRef),
                    &fp,
                    ln::OBJCALL_CAST,
                );
                if let Some(al) = args_of(args) {
                    for n in al {
                        self.visit(n);
                    }
                }
            }
            Kind::ObjectReference { object, .. } => {
                self.visit(object);
                self.check_cast(
                    &object.get_type(),
                    &TypeInfo::new(AsType::ObjectRef),
                    &fp,
                    ln::OBJREF_CAST,
                );
            }
            Kind::VariableReference {
                name,
                ty,
                id,
                global,
            } => {
                if self.fun_def_arg {
                    self.report(
                        SemErr::FunctionDefaultVariableRefInvalid,
                        &fp,
                        ln::VARREF_DEFAULT,
                    );
                    return;
                }
                match self.lookup_variable(name) {
                    Some((var_id, g)) => {
                        id.set(var_id);
                        global.set(g);
                        // QUIRK: the type is read from the *current* table even
                        // for a global variable id (wrong table; the C++ reads
                        // out of bounds when the id exceeds the local count).
                        let t = {
                            let c = self.cur.borrow();
                            if var_id < c.num_variables() {
                                c.variable_type(var_id)
                            } else {
                                output::crash()
                            }
                        };
                        *ty.borrow_mut() = t;
                    }
                    None => self.report(
                        SemErr::VariableUndefined(name.clone()),
                        &fp,
                        ln::VARREF_UNDEF,
                    ),
                }
            }
            Kind::UnpackTarget {
                nodes,
                last_named,
                expr,
                ..
            } => {
                self.visit(expr);
                self.check_cast(
                    &expr.get_type(),
                    &TypeInfo::new(AsType::Array),
                    &fp,
                    ln::UNPACK_CAST,
                );

                for un in nodes {
                    match self.lookup_variable(&un.name) {
                        Some((var_id, g)) => {
                            let t = self.sym(g).borrow().variable_type(var_id);
                            un.var_id.set(var_id);
                            un.global.set(g);
                            *un.ty.borrow_mut() = t;
                        }
                        None => self.report(
                            SemErr::VariableUndefined(un.name.clone()),
                            &fp,
                            ln::UNPACK_UNDEF,
                        ),
                    }
                }

                if let Some(last) = nodes.last() {
                    if *last_named
                        && last.var_id.get() != -1
                        && !is_ty(&last.ty.borrow(), AsType::Array)
                    {
                        self.report(
                            SemErr::UnpackWildNonarray(last.name.clone()),
                            &fp,
                            ln::UNPACK_WILD,
                        );
                    }
                }
            }
        }
    }

    fn visit_statement_list(&mut self, fp: &FilePos, stmts: &[NodeRef]) {
        let mut should_pop = false;
        if self.top_level {
            self.top_level = false;
        } else {
            self.cur.borrow_mut().push_scope();
            should_pop = true;
        }

        let mut has_return = false;
        let mut warn_unreach = false;
        for stmt in stmts {
            self.fun_def = false;
            self.visit(stmt);
            let shr = self.cur.borrow().scope_has_return();
            if !has_return && shr {
                has_return = true;
            } else if has_return && !self.fun_def && !warn_unreach {
                self.report(SemErr::Unreachable, &stmt.pos, ln::UNREACHABLE);
                warn_unreach = true;
            }
        }

        if should_pop {
            let cur = self.cur.clone();
            self.function_undefined_check(&cur, fp, ln::STMT_FUNC_UNDEF);
            self.cur.borrow_mut().pop_scope();
        }
    }

    fn visit_function_definition(
        &mut self,
        fp: &FilePos,
        ty: &TypeInfo,
        name: &Bytes,
        args: &std::cell::RefCell<Option<NodeRef>>,
        code: &std::cell::RefCell<Option<NodeRef>>,
    ) {
        if self.library.has_function(name) {
            self.report(
                SemErr::CannotOverrideLibFunction(name.clone()),
                fp,
                ln::FUNDEF_LIB,
            );
            return;
        }

        let added = self.cur.borrow_mut().add_function(name, ty);
        let fun_id = match added {
            Ok(i) | Err(i) => i,
        };

        self.fun_stack
            .push((self.parent_scope.clone(), self.fun_id));
        self.parent_scope = self.cur.clone();
        self.fun_id = fun_id;

        if added.is_ok() {
            // Create new symtbl and scope
            self.cur = SymbolTable::new_ref();
            self.parent_scope
                .borrow_mut()
                .set_function_symtbl(fun_id, self.cur.clone());

            // Check call signature
            let a = args.borrow().clone();
            if let Some(a) = a {
                // Move the args to the symbol table entry
                self.parent_scope
                    .borrow_mut()
                    .set_function_signature(fun_id, Some(a.clone()));
                *args.borrow_mut() = None;

                let mut def_val_start = false;
                if let Kind::VariableDefinitionList(list) = &a.kind {
                    for vd in list {
                        self.visit(vd);
                        let (vname, has_assign, assign) = match &vd.kind {
                            Kind::VariableDefinition { name, assign, .. } => {
                                (name.clone(), assign.is_some(), assign.clone())
                            }
                            _ => (Vec::new(), false, None),
                        };
                        if def_val_start {
                            if !has_assign {
                                self.report(
                                    SemErr::ArgumentDefaultRequired(vname, name.clone()),
                                    fp,
                                    ln::FUNDEF_DEFAULT_REQ,
                                );
                            }
                        } else if has_assign {
                            def_val_start = true;
                            self.fun_def_arg = true;
                            if let Some(ae) = assign {
                                self.visit(&ae);
                            }
                            self.fun_def_arg = false;
                        }
                    }
                }
            }
        } else {
            let rt = self.parent_scope.borrow().function_rtype(fun_id);
            if rt != *ty {
                self.report(
                    SemErr::FunctionRtypeMismatch(name.clone()),
                    fp,
                    ln::FUNDEF_RTYPE,
                );
            }

            // Lookup previously defined symbol table
            let st = self.parent_scope.borrow().function_symtbl(fun_id);
            if let Some(st) = st {
                self.cur = st;
            }

            // Verify that the call signatures match
            let ssig = self.parent_scope.borrow().function_signature(fun_id);
            let nargs = args.borrow().clone();
            match (&ssig, &nargs) {
                (Some(s), Some(n)) => {
                    let sl = match &s.kind {
                        Kind::VariableDefinitionList(l) => l.clone(),
                        _ => Vec::new(),
                    };
                    let nl = match &n.kind {
                        Kind::VariableDefinitionList(l) => l.clone(),
                        _ => Vec::new(),
                    };
                    if sl.len() == nl.len() {
                        for (svd, nvd) in sl.iter().zip(nl.iter()) {
                            let (sn, st_, _) = vd_info(svd);
                            let (nn, nt, na) = vd_info(nvd);
                            if sn != nn || st_ != nt || na {
                                self.report(
                                    SemErr::FunctionSignatureMismatch(name.clone()),
                                    fp,
                                    ln::FUNDEF_SIG1,
                                );
                                break;
                            }
                        }
                    } else {
                        self.report(
                            SemErr::FunctionSignatureMismatch(name.clone()),
                            fp,
                            ln::FUNDEF_SIG2,
                        );
                    }
                }
                (Some(_), None) | (None, Some(_)) => {
                    self.report(
                        SemErr::FunctionSignatureMismatch(name.clone()),
                        fp,
                        ln::FUNDEF_SIG3,
                    );
                }
                (None, None) => {}
            }
        }

        // If this is the definition process the code
        let c = code.borrow().clone();
        if let Some(c) = c {
            let has_def = self
                .parent_scope
                .borrow()
                .function_definition(fun_id)
                .is_some();
            if !has_def {
                self.top_level = true;
                self.visit(&c);

                if ty.ty != AsType::Void && !self.cur.borrow().scope_has_return() {
                    self.report(SemErr::NoReturn, fp, ln::FUNDEF_NO_RETURN);
                }

                // Move the code to the symbol table entry
                self.parent_scope
                    .borrow_mut()
                    .set_function_definition(fun_id, Some(c));
                *code.borrow_mut() = None;
            } else {
                self.report(
                    SemErr::FunctionRedefinition(name.clone()),
                    fp,
                    ln::FUNDEF_REDEF,
                );
            }
        }

        // Check all functions in the current scope level and make sure they have been defined
        let cur = self.cur.clone();
        self.function_undefined_check(&cur, fp, ln::FUNDEF_FUNC_UNDEF);

        // Pop function stack
        let (prev_scope, prev_id) = self.fun_stack.pop().expect("function stack");
        self.cur = self.parent_scope.clone();
        self.parent_scope = prev_scope;
        self.fun_id = prev_id;

        self.fun_def = true;
    }

    fn visit_binary(
        &mut self,
        fp: &FilePos,
        op: Tok,
        left: &NodeRef,
        right: &NodeRef,
        ty: &std::cell::RefCell<TypeInfo>,
        compare_ty: &std::cell::RefCell<TypeInfo>,
    ) {
        self.visit(left);
        self.visit(right);

        let lt = left.get_type();
        let rt = right.get_type();
        let int_t = TypeInfo::new(AsType::Int);
        let bool_t = TypeInfo::new(AsType::Bool);
        let op_s = map_token(op).to_string();

        match op {
            Tok::IdxOpen => {
                let mut err = false;
                match lt.ty {
                    AsType::Array | AsType::Matrix | AsType::String => {
                        self.check_cast(&rt, &int_t, fp, ln::IDX_CAST);
                    }
                    AsType::Dict | AsType::Runtime => {}
                    AsType::ObjectRef => {
                        if !self.obj_assign {
                            err = true;
                        }
                    }
                    _ => err = true,
                }
                if err {
                    self.report(
                        SemErr::UndefinedTypeOp(op_s, map_type(&lt)),
                        fp,
                        ln::IDX_UNDEF,
                    );
                }
                *ty.borrow_mut() = TypeInfo::new(if self.obj_assign {
                    AsType::ObjectRef
                } else {
                    AsType::Runtime
                });
            }
            Tok::ArrRange => {
                self.check_cast(&lt, &int_t, fp, ln::RANGE_L);
                self.check_cast(&rt, &int_t, fp, ln::RANGE_R);
                *ty.borrow_mut() = TypeInfo::new(AsType::Array);
            }
            Tok::ArrExpan => {
                self.check_cast(&rt, &int_t, fp, ln::EXPAN_R);
                *ty.borrow_mut() = TypeInfo::new(AsType::Array);
            }
            Tok::OpBitAnd | Tok::OpBitOr => {
                let mut valid = true;
                if !valid_bitwise(&lt) {
                    valid = false;
                    self.report(
                        SemErr::UndefinedTypeOp(op_s.clone(), map_type(&lt)),
                        fp,
                        ln::BIT_L,
                    );
                }
                if !valid_bitwise(&rt) {
                    valid = false;
                    self.report(SemErr::UndefinedTypeOp(op_s, map_type(&rt)), fp, ln::BIT_R);
                }
                if valid {
                    *ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                }
            }
            Tok::OpLogicAnd | Tok::OpLogicOr => {
                self.check_cast(&lt, &bool_t, fp, ln::LOGIC_L);
                self.check_cast(&rt, &bool_t, fp, ln::LOGIC_R);
                *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
            }
            Tok::OpEq | Tok::OpNeq => {
                if (valid_arithmetic(&lt, true) && valid_arithmetic(&rt, true))
                    || (lt.ty == AsType::String && rt.ty == AsType::String)
                {
                    *compare_ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                    *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
                } else if lt.ty == AsType::Bool || rt.ty == AsType::Bool {
                    *compare_ty.borrow_mut() = TypeInfo::new(AsType::Bool);
                    *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
                } else {
                    self.report(SemErr::CannotCompare, fp, ln::EQ_CMP);
                }
            }
            Tok::OpLe | Tok::OpGe | Tok::OpLt | Tok::OpGt => {
                if valid_arithmetic(&lt, true) && valid_arithmetic(&rt, true) {
                    *compare_ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                    *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
                } else {
                    self.report(SemErr::CannotCompare, fp, ln::REL_CMP);
                }
            }
            Tok::OpAdd => {
                if (valid_arithmetic(&lt, true) && valid_arithmetic(&rt, true))
                    || (lt.ty == AsType::String && rt.ty == AsType::String)
                    || (lt.ty == AsType::Array && rt.ty == AsType::Array)
                {
                    *ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                } else {
                    self.report(
                        SemErr::UndefinedTypeOp(op_s.clone(), map_type(&lt)),
                        fp,
                        ln::ADD_L,
                    );
                    self.report(SemErr::UndefinedTypeOp(op_s, map_type(&rt)), fp, ln::ADD_R);
                }
            }
            Tok::OpSub | Tok::OpMul => {
                if valid_arithmetic(&lt, true) && valid_arithmetic(&rt, true) {
                    *ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                } else {
                    self.report(
                        SemErr::UndefinedTypeOp(op_s.clone(), map_type(&lt)),
                        fp,
                        ln::SUBMUL_L,
                    );
                    self.report(
                        SemErr::UndefinedTypeOp(op_s, map_type(&rt)),
                        fp,
                        ln::SUBMUL_R,
                    );
                }
            }
            Tok::OpDiv | Tok::OpMod => {
                if valid_arithmetic(&lt, false) && valid_arithmetic(&rt, false) {
                    *ty.borrow_mut() = TypeInfo::new(consensus_type(&lt, &rt));
                } else {
                    self.report(
                        SemErr::UndefinedTypeOp(op_s.clone(), map_type(&lt)),
                        fp,
                        ln::DIVMOD_L,
                    );
                    self.report(
                        SemErr::UndefinedTypeOp(op_s, map_type(&rt)),
                        fp,
                        ln::DIVMOD_R,
                    );
                }
            }
            _ => self.report(SemErr::Internal, fp, ln::BIN_INTERNAL1),
        }

        if ty.borrow().ty == AsType::Invalid && self.success {
            self.report(SemErr::Internal, fp, ln::BIN_INTERNAL2);
        }
    }

    fn check_builtin_cast(
        &mut self,
        fp: &FilePos,
        args: &Option<NodeRef>,
        ty: &std::cell::RefCell<TypeInfo>,
        t: AsType,
    ) {
        match args_of(args) {
            Some(a) if a.len() == 1 => {
                let argn = a[0].clone();
                self.visit(&argn);
                self.check_cast(&argn.get_type(), &TypeInfo::new(t), fp, ln::BCAST_CAST);
            }
            _ => self.report(
                SemErr::BuiltinCallSignatureMismatch(map_builtin(builtin_of_cast(t)).to_string()),
                fp,
                ln::BCAST_MISMATCH,
            ),
        }
        *ty.borrow_mut() = TypeInfo::new(t);
    }

    fn visit_builtin(
        &mut self,
        fp: &FilePos,
        args: &Option<NodeRef>,
        ty: &std::cell::RefCell<TypeInfo>,
        builtin: BuiltIn,
        target: &Option<NodeRef>,
    ) {
        let bname = map_builtin(builtin).to_string();
        let mismatch = |s: &mut Self, line: u32| {
            s.report(
                SemErr::BuiltinCallSignatureMismatch(bname.clone()),
                fp,
                line,
            )
        };
        let argv = args_of(args).cloned();
        let trgt = target.clone();
        let int_t = TypeInfo::new(AsType::Int);
        let mut allow_dict = false;

        match builtin {
            BuiltIn::CastBool => self.check_builtin_cast(fp, args, ty, AsType::Bool),
            BuiltIn::CastChar => self.check_builtin_cast(fp, args, ty, AsType::Char),
            BuiltIn::CastInt => self.check_builtin_cast(fp, args, ty, AsType::Int),
            BuiltIn::CastFloat => self.check_builtin_cast(fp, args, ty, AsType::Float),
            BuiltIn::CastString => self.check_builtin_cast(fp, args, ty, AsType::String),

            BuiltIn::IsArray
            | BuiltIn::IsBool
            | BuiltIn::IsChar
            | BuiltIn::IsDict
            | BuiltIn::IsInt
            | BuiltIn::IsFloat
            | BuiltIn::IsMatrix
            | BuiltIn::IsString => {
                match &argv {
                    Some(a) if a.len() == 1 => self.visit(&a[0].clone()),
                    _ => mismatch(self, ln::IS_MISMATCH),
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
            }

            BuiltIn::Clear => {
                let t = trgt.expect("builtin target");
                self.visit(&t);
                let tt = t.get_type().ty;
                if tt != AsType::Array
                    && tt != AsType::Dict
                    && tt != AsType::Matrix
                    && tt != AsType::Runtime
                {
                    mismatch(self, ln::CLEAR_TYPE);
                }
                if args.is_some() {
                    mismatch(self, ln::CLEAR_ARGS);
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Void);
            }

            BuiltIn::Copy => {
                let t = trgt.expect("builtin target");
                self.visit(&t);
                let tt = t.get_type().ty;
                if tt != AsType::Array && tt != AsType::Runtime {
                    mismatch(self, ln::COPY_TYPE);
                }
                if args.is_some() {
                    mismatch(self, ln::COPY_ARGS);
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Void);
            }

            BuiltIn::HasKey => {
                let t = trgt.expect("builtin target");
                self.visit(&t);
                self.check_cast(
                    &t.get_type(),
                    &TypeInfo::new(AsType::Dict),
                    fp,
                    ln::HASKEY_CAST,
                );
                match &argv {
                    Some(a) if a.len() == 1 => self.visit(&a[0].clone()),
                    _ => mismatch(self, ln::HASKEY_ARGS),
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Bool);
            }

            BuiltIn::Keys | BuiltIn::Values => {
                let t = trgt.expect("builtin target");
                self.visit(&t);
                self.check_cast(
                    &t.get_type(),
                    &TypeInfo::new(AsType::Dict),
                    fp,
                    ln::KEYS_CAST,
                );
                if args.is_some() {
                    mismatch(self, ln::KEYS_ARGS);
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Array);
            }

            BuiltIn::Len => {
                let t = trgt.expect("builtin target");
                self.visit(&t);
                self.check_cast(
                    &t.get_type(),
                    &TypeInfo::new(AsType::Array),
                    fp,
                    ln::LEN_CAST,
                );
                if args.is_some() {
                    mismatch(self, ln::LEN_ARGS);
                }
                *ty.borrow_mut() = TypeInfo::new(AsType::Int);
            }

            BuiltIn::Remove | BuiltIn::Resize => {
                if builtin == BuiltIn::Remove {
                    allow_dict = true;
                }
                let t = trgt.expect("builtin target");
                self.visit(&t);

                let tt = t.get_type().ty;
                if (allow_dict
                    && tt != AsType::Array
                    && tt != AsType::Dict
                    && tt != AsType::Runtime)
                    || (!allow_dict
                        && tt != AsType::Array
                        && tt != AsType::Matrix
                        && tt != AsType::Runtime)
                {
                    mismatch(self, ln::RM_TYPE);
                }

                // QUIRK: `args->GetSize()` dereferences NULL when there are
                // no arguments (SIGSEGV in the C++).
                if tt == AsType::Array {
                    let a = match &argv {
                        Some(a) => a.clone(),
                        None => output::crash(),
                    };
                    if a.len() == 1 {
                        self.visit(&a[0]);
                        self.check_cast(&a[0].get_type(), &int_t, fp, ln::RM_ARR_CAST);
                    } else {
                        mismatch(self, ln::RM_ARR_MISMATCH);
                    }
                } else if tt == AsType::Dict {
                    let a = match &argv {
                        Some(a) => a.clone(),
                        None => output::crash(),
                    };
                    if a.len() == 1 {
                        self.visit(&a[0]);
                    } else {
                        mismatch(self, ln::RM_DICT_MISMATCH);
                    }
                } else if tt == AsType::Matrix {
                    let a = match &argv {
                        Some(a) => a.clone(),
                        None => output::crash(),
                    };
                    if a.len() == 2 {
                        self.visit(&a[0]);
                        self.check_cast(&a[0].get_type(), &int_t, fp, ln::RM_MAT_CAST1);
                        self.visit(&a[1]);
                        self.check_cast(&a[1].get_type(), &int_t, fp, ln::RM_MAT_CAST2);
                    } else {
                        mismatch(self, ln::RM_MAT_MISMATCH);
                    }
                }

                *ty.borrow_mut() = TypeInfo::new(AsType::Void);
            }

            BuiltIn::Unknown => self.report(SemErr::Internal, fp, ln::BUILTIN_INTERNAL),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_function_call(
        &mut self,
        fp: &FilePos,
        name: &Bytes,
        args: &Option<NodeRef>,
        ty: &std::cell::RefCell<TypeInfo>,
        id: &std::cell::Cell<i32>,
        global: &std::cell::Cell<bool>,
        func: &std::cell::RefCell<Option<Rc<dyn crate::library::ASFunction>>>,
    ) {
        if self.fun_def_arg {
            self.report(SemErr::FunctionDefaultCallInvalid, fp, ln::FCALL_DEFAULT);
            return;
        }

        let argv = args_of(args).cloned();

        if let Some(libfun) = self.library.lookup_function(name) {
            // Check function parameters for match to the signature
            match &argv {
                Some(a) if libfun.arity() == a.len() => {
                    let mut err = false;
                    for (i, an) in a.iter().enumerate().take(libfun.arity()) {
                        self.visit(an);
                        let in_t = an.get_type().ty;
                        let out_t = libfun.argument_type(i).ty;
                        if valid_cast(in_t, out_t) {
                            if (in_t == AsType::Float && out_t == AsType::Int)
                                || (in_t == AsType::Int && out_t == AsType::Char)
                            {
                                self.report(
                                    SemErr::LossOfPrecision(map_ty(in_t), map_ty(out_t)),
                                    fp,
                                    ln::FCALL_LIB_PREC,
                                );
                            }
                        } else {
                            if !err {
                                self.report(
                                    SemErr::FunctionCallSignatureMismatch(name.clone()),
                                    fp,
                                    ln::FCALL_LIB_SIG,
                                );
                                err = true;
                            }
                            self.report(
                                SemErr::CannotCast(map_ty(in_t), map_ty(out_t)),
                                fp,
                                ln::FCALL_LIB_CAST,
                            );
                        }
                    }
                }
                _ => {
                    if libfun.arity() != 0 {
                        self.report(
                            SemErr::FunctionCallSignatureMismatch(name.clone()),
                            fp,
                            ln::FCALL_LIB_ARITY,
                        );
                    }
                }
            }

            *ty.borrow_mut() = libfun.return_type().clone();
            *func.borrow_mut() = Some(libfun);
        } else if let Some((fun_id, g)) = self.lookup_function(name) {
            let symtbl = self.sym(g);
            let sig = symtbl.borrow().function_signature(fun_id);
            let sigv: Option<Vec<NodeRef>> = sig.as_ref().and_then(|s| match &s.kind {
                Kind::VariableDefinitionList(l) => Some(l.clone()),
                _ => None,
            });

            // Check function parameters for match to the signature
            if let Some(a) = &argv {
                match &sigv {
                    Some(sv) if a.len() <= sv.len() => {
                        let mut err = false;
                        let mut consumed = 0usize;
                        for an in a {
                            self.visit(an);
                            let in_t = an.get_type().ty;
                            let out_t = vd_info(&sv[consumed]).1.ty;
                            consumed += 1;
                            if valid_cast(in_t, out_t) {
                                if (in_t == AsType::Float && out_t == AsType::Int)
                                    || (in_t == AsType::Int && out_t == AsType::Char)
                                {
                                    self.report(
                                        SemErr::LossOfPrecision(map_ty(in_t), map_ty(out_t)),
                                        fp,
                                        ln::FCALL_USR_PREC,
                                    );
                                }
                            } else {
                                if !err {
                                    self.report(
                                        SemErr::FunctionCallSignatureMismatch(name.clone()),
                                        fp,
                                        ln::FCALL_USR_SIG,
                                    );
                                    err = true;
                                }
                                self.report(
                                    SemErr::CannotCast(map_ty(in_t), map_ty(out_t)),
                                    fp,
                                    ln::FCALL_USR_CAST,
                                );
                            }
                        }

                        // `!sit.AtEnd() && !sit.Next()->GetAssignmentExpression()`
                        if consumed < sv.len() {
                            let (nm, _, has_assign) = vd_info(&sv[consumed]);
                            if !has_assign {
                                self.report(
                                    SemErr::ArgumentMissingRequired(nm),
                                    fp,
                                    ln::FCALL_USR_MISSING,
                                );
                            }
                        }
                    }
                    _ => self.report(
                        SemErr::FunctionCallSignatureMismatch(name.clone()),
                        fp,
                        ln::FCALL_USR_SIG2,
                    ),
                }
            } else if let Some(sv) = &sigv {
                if !sv.is_empty() && !vd_info(&sv[0]).2 {
                    self.report(
                        SemErr::FunctionCallSignatureMismatch(name.clone()),
                        fp,
                        ln::FCALL_USR_SIG3,
                    );
                    // QUIRK: the following `while ((vd == it.Next()))` compares
                    // instead of assigning and never runs its body.
                }
            }

            id.set(fun_id);
            global.set(g);
            *ty.borrow_mut() = symtbl.borrow().function_rtype(fun_id);
        } else {
            self.report(
                SemErr::FunctionUndeclared(name.clone()),
                fp,
                ln::FCALL_UNDECLARED,
            );
        }
    }

    /// `PostCheck()`.
    pub fn post_check(&mut self) {
        let fns = self.global.borrow().active_functions();
        for (has_code, name) in fns {
            if !has_code {
                let fp = self.main_pos.clone();
                self.report(
                    SemErr::FunctionUndefined(name),
                    &fp,
                    ln::POSTCHECK_FUNC_UNDEF,
                );
            }
        }
    }
}

fn builtin_of_cast(t: AsType) -> BuiltIn {
    match t {
        AsType::Bool => BuiltIn::CastBool,
        AsType::Char => BuiltIn::CastChar,
        AsType::Int => BuiltIn::CastInt,
        AsType::Float => BuiltIn::CastFloat,
        _ => BuiltIn::CastString,
    }
}

/// `(name, type, has default)` of a variable definition node.
fn vd_info(n: &NodeRef) -> (Bytes, TypeInfo, bool) {
    match &n.kind {
        Kind::VariableDefinition {
            name, ty, assign, ..
        } => (name.clone(), ty.clone(), assign.is_some()),
        _ => (Vec::new(), TypeInfo::invalid(), false),
    }
}

/// `validArithmeticType` (the original `switch`, see `git show ef5c6d948`).
pub fn valid_arithmetic(t: &TypeInfo, allow_matrix: bool) -> bool {
    match t.ty {
        AsType::Matrix => allow_matrix,
        AsType::Runtime | AsType::Int | AsType::Char | AsType::Float => true,
        _ => false,
    }
}

/// `validBitwiseType`.
pub fn valid_bitwise(t: &TypeInfo) -> bool {
    matches!(t.ty, AsType::Runtime | AsType::Int | AsType::Char)
}

/// `getConsensusType`.
pub fn consensus_type(left: &TypeInfo, right: &TypeInfo) -> AsType {
    use AsType::*;
    match left.ty {
        Array => Array,
        Bool => match right.ty {
            Array | Bool | Char | Float | Int | Matrix | ObjectRef | String => Bool,
            Var | Runtime => Runtime,
            _ => Invalid,
        },
        Char => match right.ty {
            Array => Array,
            Bool => Char,
            Char => Char,
            Float => Float,
            Int => Int,
            Matrix => Matrix,
            String => String,
            Var => Runtime,
            Runtime => Runtime,
            _ => Invalid,
        },
        Float => match right.ty {
            Array => Array,
            Bool => Float,
            Char => Float,
            Float => Float,
            Int => Float,
            Matrix => Matrix,
            String => Float,
            Var => Runtime,
            Runtime => Runtime,
            _ => Invalid,
        },
        Int => match right.ty {
            Array => Array,
            Bool => Int,
            Char => Int,
            Float => Float,
            Int => Int,
            Matrix => Matrix,
            String => Int,
            Var => Runtime,
            Runtime => Runtime,
            _ => Invalid,
        },
        Matrix => Matrix,
        String => String,
        Var => Runtime,
        Runtime => Runtime,
        _ => Invalid,
    }
}
