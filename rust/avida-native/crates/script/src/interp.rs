//! `cDirectInterpretASTVisitor`: direct execution of the AST.
//!
//! The C++ keeps values in a union plus a separate type tag and manages
//! arrays, dicts and matrices with hand written reference counts that the
//! language semantics observe (`remove()` and `resize()` do nothing on an
//! unshared aggregate). Values are therefore modelled as an enum whose
//! aggregate variants are `Rc<RefCell<..>>` objects carrying an explicit
//! `rc` field that is incremented and decremented at exactly the places the
//! C++ calls `GetReference()` / `RemoveReference()`. A count reaching zero
//! runs the destructor cascade (decrementing nested counts) but never frees
//! (Rust owns the memory), so the use-after-free paths of the C++ keep
//! working here.
//!
//! Runtime errors print `file:line: error: ...  (cDirectInterpretASTVisitor.cc:N)`
//! and end the process with `AS_EXIT_FAIL_INTERPRET`.

use std::cell::RefCell;
use std::rc::Rc;

use avida_apto::fmt::fmt_fixed;
use avida_config::string_util::{as_double, as_int};

use crate::ast::*;
use crate::native::{CppParam, NativeObject};
use crate::oldmap::{MapKey, OldMap};
use crate::output;
use crate::symtab::SymRef;
use crate::types::{
    map_token, map_ty, map_type, AsType, BuiltIn, Tok, TypeInfo, AS_EXIT_FAIL_INTERPRET,
};

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// A heap `cString*` of the C++: identity matters (values share the pointer,
/// `delete` frees it for every holder).
pub struct StrObj {
    bytes: RefCell<Bytes>,
    freed: std::cell::Cell<bool>,
}
pub type StrRef = Rc<StrObj>;

/// `new cString(bytes)`.
pub fn snew(b: Bytes) -> StrRef {
    Rc::new(StrObj {
        bytes: RefCell::new(b),
        freed: std::cell::Cell::new(false),
    })
}

/// Read `*p`. QUIRK: reading a deleted string is a use-after-free in the C++,
/// which in practice ends in SIGSEGV.
fn sread(s: &StrRef) -> Bytes {
    if s.freed.get() {
        output::crash();
    }
    s.bytes.borrow().clone()
}

/// `delete p` (a second delete is a double free: crash).
fn sdelete(s: &StrRef) {
    if s.freed.get() {
        output::crash();
    }
    s.freed.set(true);
}

pub type ArrRef = Rc<RefCell<LocalArray>>;
pub type DictRef = Rc<RefCell<LocalDict>>;
pub type MatRef = Rc<RefCell<LocalMatrix>>;

/// `uAnyType` + `sASTypeInfo` of the C++ (`m_rvalue`/`m_rtype`, stack slots,
/// array elements, dict keys and values).
#[derive(Clone)]
pub enum Value {
    /// Type tag `AS_TYPE_INVALID` (default-constructed `sAggregateValue`).
    Invalid,
    /// Type tag `AS_TYPE_VOID`.
    Void,
    /// A NULL pointer of a string, matrix or object variable.
    Null,
    Bool(bool),
    Char(i8),
    Int(i32),
    Float(f64),
    Str(StrRef),
    Array(ArrRef),
    Dict(DictRef),
    Matrix(MatRef),
    /// `cObjectRef`: an assignable location (only while `m_obj_assign`).
    Ref(Rc<ObjRef>),
    /// A native object with its declared class name (`m_rtype.info`).
    NObj(Rc<dyn NativeObject>, String),
}

impl Value {
    pub fn ty(&self) -> AsType {
        match self {
            Value::Invalid | Value::Null => AsType::Invalid,
            Value::Void => AsType::Void,
            Value::Bool(_) => AsType::Bool,
            Value::Char(_) => AsType::Char,
            Value::Int(_) => AsType::Int,
            Value::Float(_) => AsType::Float,
            Value::Str(_) => AsType::String,
            Value::Array(_) => AsType::Array,
            Value::Dict(_) => AsType::Dict,
            Value::Matrix(_) => AsType::Matrix,
            Value::Ref(_) | Value::NObj(..) => AsType::ObjectRef,
        }
    }

    pub fn type_info(&self) -> TypeInfo {
        match self {
            Value::NObj(_, info) => TypeInfo::with_info(AsType::ObjectRef, info),
            v => TypeInfo::new(v.ty()),
        }
    }
}

/// `sAggregateValue::Cleanup`.
fn cleanup(v: &Value) {
    match v {
        Value::Array(a) => arr_remove_ref(a),
        Value::Dict(d) => dict_remove_ref(d),
        Value::Matrix(m) => mat_remove_ref(m),
        Value::Str(s) => sdelete(s),
        _ => {}
    }
}

fn arr_get_ref(a: &ArrRef) -> ArrRef {
    a.borrow_mut().rc += 1;
    a.clone()
}

fn arr_remove_ref(a: &ArrRef) {
    let destroy = {
        let mut b = a.borrow_mut();
        b.rc -= 1;
        b.rc == 0
    };
    if destroy {
        destroy_array(a);
    }
}

/// `~cLocalArray`: release the elements; a second destruction is a double
/// free in the C++.
fn destroy_array(a: &ArrRef) {
    if a.borrow().freed {
        output::crash();
    }
    let elems = a.borrow().storage.clone();
    for e in &elems {
        cleanup(e);
    }
    a.borrow_mut().freed = true;
}

fn dict_get_ref(d: &DictRef) -> DictRef {
    d.borrow_mut().rc += 1;
    d.clone()
}

fn dict_remove_ref(d: &DictRef) {
    let destroy = {
        let mut b = d.borrow_mut();
        b.rc -= 1;
        b.rc == 0
    };
    if destroy {
        if d.borrow().freed {
            output::crash();
        }
        dict_cleanup_all(d);
        d.borrow_mut().freed = true;
    }
}

fn dict_cleanup_all(d: &DictRef) {
    let all: Vec<(AggKey, Value)> = d
        .borrow()
        .mp()
        .iter()
        .map(|e| (e.0.clone(), e.1.clone()))
        .collect();
    for (k, _) in &all {
        cleanup(&k.0);
    }
    for (_, v) in &all {
        cleanup(v);
    }
}

fn mat_get_ref(m: &MatRef) -> MatRef {
    m.borrow_mut().rc += 1;
    m.clone()
}

fn mat_remove_ref(m: &MatRef) {
    let destroy = {
        let mut b = m.borrow_mut();
        b.rc -= 1;
        b.rc == 0
    };
    if destroy {
        if m.borrow().freed {
            output::crash();
        }
        let rows = m.borrow().rows.clone();
        for r in &rows {
            destroy_array(r);
        }
        m.borrow_mut().freed = true;
    }
}

/// `cLocalArray`.
pub struct LocalArray {
    pub storage: Vec<Value>,
    pub rc: i32,
    pub resizable: bool,
    /// Set once the reference count reached zero (the C++ `delete this`).
    pub freed: bool,
}

impl LocalArray {
    /// Contents; QUIRK: touching a destroyed object is a use-after-free in
    /// the C++, which in practice ends in SIGSEGV.
    fn st(&self) -> &Vec<Value> {
        if self.freed {
            output::crash();
        }
        &self.storage
    }
    fn st_mut(&mut self) -> &mut Vec<Value> {
        if self.freed {
            output::crash();
        }
        &mut self.storage
    }
}

impl LocalArray {
    fn new_ref() -> ArrRef {
        Rc::new(RefCell::new(LocalArray {
            storage: Vec::new(),
            rc: 1,
            resizable: true,
            freed: false,
        }))
    }
    fn sized(sz: usize) -> ArrRef {
        Rc::new(RefCell::new(LocalArray {
            storage: vec![Value::Invalid; sz],
            rc: 1,
            resizable: true,
            freed: false,
        }))
    }
}

/// `cLocalArray(cLocalArray*)`: copy constructor.
fn arr_copy_new(src: &ArrRef) -> ArrRef {
    let n = src.borrow().st().len();
    let a = LocalArray::sized(n);
    let elems = src.borrow().st().clone();
    arr_copy_into(&a, 0, &elems);
    a
}

/// `cLocalArray(arr1, arr2)`: concatenation.
fn arr_concat_new(a1: &ArrRef, a2: &ArrRef) -> ArrRef {
    let e1 = a1.borrow().st().clone();
    let e2 = a2.borrow().st().clone();
    let a = LocalArray::sized(e1.len() + e2.len());
    arr_copy_into(&a, 0, &e1);
    arr_copy_into(&a, e1.len(), &e2);
    a
}

/// `cLocalArray::copy`.
fn arr_copy_into(a: &ArrRef, offset: usize, src: &[Value]) {
    for (i, v) in src.iter().enumerate() {
        let nv = match v {
            Value::Bool(_) | Value::Char(_) | Value::Int(_) | Value::Float(_) => v.clone(),
            Value::Str(s) => Value::Str(snew(sread(s))),
            Value::Array(x) => Value::Array(arr_get_ref(x)),
            Value::Dict(x) => Value::Dict(dict_get_ref(x)),
            Value::Matrix(x) => Value::Matrix(mat_get_ref(x)),
            _ => Value::Invalid,
        };
        a.borrow_mut().st_mut()[i + offset] = nv;
    }
}

/// `cLocalArray::Set(i, type, value)`.
fn arr_set(a: &ArrRef, i: usize, v: &Value) {
    let old = a.borrow().st()[i].clone();
    cleanup(&old);
    let nv = match v {
        Value::Bool(_) | Value::Char(_) | Value::Int(_) | Value::Float(_) => v.clone(),
        Value::Str(s) => Value::Str(snew(sread(s))),
        Value::Array(x) => Value::Array(arr_get_ref(x)),
        Value::Dict(x) => Value::Dict(dict_get_ref(x)),
        Value::Matrix(x) => Value::Matrix(mat_get_ref(x)),
        _ => Value::Invalid,
    };
    a.borrow_mut().st_mut()[i] = nv;
}

/// `cLocalArray::Resize`.
fn arr_resize(a: &ArrRef, sz: i32) {
    if sz < 0 {
        // QUIRK: `Apto::Array::Resize(-n)` throws bad_array_new_length (abort).
        output::crash();
    }
    let sz = sz as usize;
    let o_sz = a.borrow().st().len();
    for i in sz..o_sz {
        let e = a.borrow().st()[i].clone();
        cleanup(&e);
    }
    let mut b = a.borrow_mut();
    b.storage.resize(sz, Value::Invalid);
    for i in o_sz..sz {
        b.storage[i] = Value::Int(0);
    }
}

/// `cLocalArray::SetWithArray`.
fn arr_set_with_array(a: &ArrRef, src: &ArrRef) {
    let n = src.borrow().st().len();
    arr_resize(a, n as i32);
    for i in 0..n {
        let v = src.borrow().st()[i].clone();
        arr_set(a, i, &v);
    }
}

/// Post-processing shared by `SetWithKeys` / `SetWithValues`.
fn arr_set_with_entries(a: &ArrRef, entries: Vec<Value>) {
    arr_resize(a, 0);
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        out.push(match &e {
            Value::Str(s) => Value::Str(snew(sread(s))),
            Value::Array(x) => {
                x.borrow_mut().rc += 1;
                e.clone()
            }
            Value::Dict(x) => {
                x.borrow_mut().rc += 1;
                e.clone()
            }
            Value::Matrix(x) => {
                x.borrow_mut().rc += 1;
                e.clone()
            }
            _ => e.clone(),
        });
    }
    *a.borrow_mut().st_mut() = out;
}

/// Dict key wrapper: `sAggregateValue` with the `operator==` and
/// `nHashTable::HashKey` specialisation of the C++.
#[derive(Clone)]
pub struct AggKey(pub Value);

impl MapKey for AggKey {
    fn hash23(&self) -> i32 {
        let h = |k: i32| (k % 23).abs();
        match &self.0 {
            Value::Bool(b) => h(*b as i32),
            Value::Char(c) => h(*c as i32),
            Value::Int(i) => h(*i),
            Value::Float(f) => h(*f as i32),
            Value::Str(s) => sread(s).hash23(),
            // HashKey<void*>: address based, not reproducible.
            _ => 0,
        }
    }
    fn key_eq(&self, o: &Self) -> bool {
        match (&self.0, &o.0) {
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => sread(a) == sread(b),
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Matrix(a), Value::Matrix(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// `cLocalDict`.
pub struct LocalDict {
    pub map: OldMap<AggKey, Value>,
    pub rc: i32,
    pub freed: bool,
}

impl LocalDict {
    fn mp(&self) -> &OldMap<AggKey, Value> {
        if self.freed {
            output::crash();
        }
        &self.map
    }
    fn mp_mut(&mut self) -> &mut OldMap<AggKey, Value> {
        if self.freed {
            output::crash();
        }
        &mut self.map
    }
}

fn new_dict() -> DictRef {
    Rc::new(RefCell::new(LocalDict {
        map: OldMap::new(),
        rc: 1,
        freed: false,
    }))
}

/// `cLocalDict::Set`.
fn dict_set(d: &DictRef, idx: &Value, val: &Value) {
    let key = AggKey(idx.clone());
    let existing = d.borrow().mp().find(&key).cloned();
    if let Some(o) = existing {
        cleanup(&o);
    }
    d.borrow_mut().mp_mut().set_value(key, val.clone());
}

fn dict_get(d: &DictRef, idx: &Value) -> Option<Value> {
    d.borrow().mp().find(&AggKey(idx.clone())).cloned()
}

fn dict_clear(d: &DictRef) {
    dict_cleanup_all(d);
    d.borrow_mut().mp_mut().clear();
}

/// `cLocalMatrix`.
pub struct LocalMatrix {
    pub rows: Vec<ArrRef>,
    pub sz_y: i32,
    pub rc: i32,
    pub freed: bool,
}

impl LocalMatrix {
    fn rw(&self) -> &Vec<ArrRef> {
        if self.freed {
            output::crash();
        }
        &self.rows
    }
    #[allow(dead_code)]
    fn rw_mut(&mut self) -> &mut Vec<ArrRef> {
        if self.freed {
            output::crash();
        }
        &mut self.rows
    }
}

fn new_matrix() -> MatRef {
    Rc::new(RefCell::new(LocalMatrix {
        rows: Vec::new(),
        sz_y: 0,
        rc: 1,
        freed: false,
    }))
}

/// `cLocalMatrix::Resize`.
fn mat_resize(m: &MatRef, sz_x: i32, sz_y: i32) {
    if sz_x < 0 {
        output::crash();
    }
    let sz_x = sz_x as usize;
    let o_sz = m.borrow().rw().len();
    {
        let mut b = m.borrow_mut();
        if sz_x < o_sz {
            let dropped: Vec<ArrRef> = b.rows.drain(sz_x..).collect();
            drop(b);
            for r in dropped {
                let elems = r.borrow().st().clone();
                for e in &elems {
                    cleanup(e);
                }
            }
        } else {
            b.rows.resize_with(sz_x, LocalArray::new_ref);
        }
    }
    let start = if m.borrow().sz_y == sz_y { o_sz } else { 0 };
    for i in start..sz_x {
        let row = m.borrow().rw()[i].clone();
        row.borrow_mut().resizable = false;
        arr_resize(&row, sz_y);
    }
    m.borrow_mut().sz_y = sz_y;
}

fn mat_set_row(m: &MatRef, i: usize, arr: &ArrRef) {
    let row = m.borrow().rw()[i].clone();
    arr_set_with_array(&row, arr);
}

fn mat_num_rows(m: &MatRef) -> i32 {
    m.borrow().rw().len() as i32
}

// ---------------------------------------------------------------------------
// Object references (`cObjectRef` and subclasses)
// ---------------------------------------------------------------------------

pub enum ObjRef {
    ArrayVar(usize),
    DictVar(usize),
    MatrixVar(usize),
    NativeVar(usize),
    StringVar(usize),
    Index(Rc<ObjRef>, Value),
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum IErr {
    CannotResizeMatrixRow,
    DivisionByZero,
    IndexOutOfBounds,
    IndexError,
    InvalidArraySize,
    KeyNotFound,
    MatrixOpTypeMismatch(String, String),
    MatrixSizeMismatch(String),
    NobjMethodLookupFailed(String, String),
    NobjTypeMismatch(String, String),
    ObjectAssignFail,
    TypeCast(String, String),
    UndefinedTypeOp(String, String),
    UnpackValueTooLarge,
    UnpackValueTooSmall,
    Internal,
    /// Failure reported by native code (not a C++ error kind).
    Native(String),
}

/// `__LINE__` values of the `INTERPRET_ERROR` sites in
/// `cDirectInterpretASTVisitor.cc`.
mod ln {
    pub const ASSIGN_DEFAULT: u32 = 156;
    pub const ARGLIST: u32 = 164;
    pub const OBJ_ASSIGN_FAIL: u32 = 180;
    pub const FOREACH_DEFAULT: u32 = 253;
    pub const VARDEF_DEFAULT: u32 = 332;
    pub const VARDEF_DIMS: u32 = 353;
    pub const VARDEFLIST: u32 = 364;
    pub const RANGE_INVALID: u32 = 409;
    pub const BIT_UNDEF: u32 = 448;
    pub const EQ_UNDEF: u32 = 499;
    pub const REL_INTERNAL1: u32 = 528;
    pub const REL_INTERNAL2: u32 = 542;
    pub const REL_UNDEF: u32 = 548;
    pub const ADD_UNDEF: u32 = 590;
    pub const SUB_UNDEF: u32 = 611;
    pub const MUL_UNDEF: u32 = 632;
    pub const DIV_ZERO_CHAR: u32 = 650;
    pub const DIV_ZERO_INT: u32 = 657;
    pub const DIV_ZERO_FLOAT: u32 = 664;
    pub const DIV_UNDEF: u32 = 670;
    pub const MOD_ZERO_CHAR: u32 = 688;
    pub const MOD_ZERO_INT: u32 = 695;
    pub const MOD_ZERO_FLOAT: u32 = 702;
    pub const MOD_UNDEF: u32 = 708;
    pub const IDX_INDEX_ERROR: u32 = 723;
    pub const IDX_OBJ_UNDEF: u32 = 734;
    pub const IDX_ARR_OOB: u32 = 747;
    pub const IDX_KEY: u32 = 765;
    pub const IDX_MAT_OOB: u32 = 782;
    pub const IDX_STR_OOB: u32 = 795;
    pub const IDX_CAST: u32 = 804;
    pub const BIN_INTERNAL: u32 = 813;
    pub const UN_BITNOT_UNDEF: u32 = 833;
    pub const UN_SUB_UNDEF: u32 = 855;
    pub const UN_INTERNAL: u32 = 860;
    pub const CLEAR_UNDEF: u32 = 995;
    pub const COPY_UNDEF: u32 = 1006;
    pub const HASKEY_UNDEF: u32 = 1023;
    pub const KEYS_UNDEF: u32 = 1037;
    pub const VALUES_UNDEF: u32 = 1050;
    pub const REMOVE_OOB: u32 = 1095;
    pub const REMOVE_UNDEF: u32 = 1100;
    pub const RESIZE_MAT_SIZE: u32 = 1109;
    pub const RESIZE_ROW: u32 = 1127;
    pub const RESIZE_ARR_SIZE: u32 = 1129;
    pub const RESIZE_UNDEF: u32 = 1142;
    pub const BUILTIN_INTERNAL: u32 = 1147;
    pub const LIBCALL_ARG: u32 = 1177;
    pub const LIBCALL_RET: u32 = 1196;
    pub const LIBCALL_CLEAN: u32 = 1212;
    pub const USRCALL_ARG: u32 = 1278;
    pub const USRCALL_RET: u32 = 1303;
    pub const LITERAL_INTERNAL: u32 = 1356;
    pub const LITMAT_SIZE: u32 = 1383;
    pub const OBJCALL_CAST: u32 = 1439;
    pub const OBJCALL_LOOKUP: u32 = 1445;
    pub const OBJCALL_ARG: u32 = 1465;
    pub const OBJCALL_RET: u32 = 1485;
    pub const OBJREF_INTERNAL: u32 = 1508;
    pub const VARREF_OBJASSIGN: u32 = 1525;
    pub const VARREF_INTERNAL: u32 = 1551;
    pub const UNPACK_SMALL: u32 = 1564;
    pub const UNPACK_LARGE: u32 = 1565;
    pub const UNPACK_DEFAULT: u32 = 1610;
    pub const AS_ARRAY: u32 = 1665;
    pub const AS_BOOL_OBJ: u32 = 1712;
    pub const AS_BOOL: u32 = 1715;
    pub const AS_CHAR: u32 = 1733;
    pub const AS_DICT: u32 = 1764;
    pub const AS_INT: u32 = 1789;
    pub const AS_FLOAT: u32 = 1815;
    pub const AS_MATRIX: u32 = 1842;
    pub const AS_NOBJ_MISMATCH: u32 = 1854;
    pub const AS_NOBJ: u32 = 1858;
    pub const AS_STRING: u32 = 1901;
    pub const MAT_ADD: u32 = 1917;
    pub const MAT_SUB: u32 = 1923;
    pub const MATMUL_CHECK1: u32 = 1946;
    pub const MATMUL_CHECK2: u32 = 1965;
    pub const MATMUL_SIZE: u32 = 1987;
    pub const SCALAR_INT_S: u32 = 2007;
    pub const SCALAR_INT_M: u32 = 2021;
    pub const SCALAR_FLOAT_S: u32 = 2059;
    pub const SCALAR_FLOAT_M: u32 = 2074;
    pub const FULL_INT_1: u32 = 2114;
    pub const FULL_INT_2: u32 = 2129;
    pub const FULL_FLOAT_1: u32 = 2172;
    pub const FULL_FLOAT_2: u32 = 2188;
}

fn val_info(v: &Value) -> TypeInfo {
    v.type_info()
}

/// Which C++ conversion a runtime type name request refers to.
fn cname(t: AsType) -> String {
    map_ty(t)
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

pub struct Interpreter {
    global: SymRef,
    cur: SymRef,
    rvalue: Value,
    stack: Vec<Value>,
    sp: usize,
    has_returned: bool,
    obj_assign: bool,
}

/// `getRuntimeType`: the original body (`git show ef5c6d948`).
pub fn get_runtime_type(l: AsType, r: AsType, allow_str: bool) -> AsType {
    use AsType::*;
    match l {
        Array => Array,
        Bool => match r {
            Array | Bool | Char | Float | Int | Matrix | ObjectRef | String => Bool,
            _ => Invalid,
        },
        Char => match r {
            Array => Array,
            Bool => Char,
            Char => Char,
            Float => Float,
            Int => Int,
            Matrix => Matrix,
            String if allow_str => String,
            _ => Invalid,
        },
        Dict => Dict,
        Float => match r {
            Array => Array,
            Bool => Float,
            Char => Float,
            Float => Float,
            Int => Float,
            Matrix => Matrix,
            String if allow_str => Float,
            _ => Invalid,
        },
        Int => match r {
            Array => Array,
            Bool => Int,
            Char => Int,
            Float => Float,
            Int => Int,
            Matrix => Matrix,
            String if allow_str => Int,
            _ => Invalid,
        },
        Matrix => Matrix,
        String => {
            if allow_str {
                String
            } else {
                Invalid
            }
        }
        _ => Invalid,
    }
}

fn args_vec(n: &Option<NodeRef>) -> Option<Vec<NodeRef>> {
    match n {
        Some(a) => match &a.kind {
            Kind::ArgumentList(v) => Some(v.clone()),
            _ => None,
        },
        None => None,
    }
}

/// `idxi` conversion shared by the object references.
fn idx_to_int(idx: &Value) -> i32 {
    match idx {
        Value::Bool(b) => *b as i32,
        Value::Char(c) => *c as i32,
        Value::Int(i) => *i,
        Value::Float(f) => *f as i32,
        Value::Str(s) => as_int(&String::from_utf8_lossy(&sread(s))),
        _ => -1,
    }
}

impl Interpreter {
    /// `cDirectInterpretASTVisitor(global_symtbl)`.
    pub fn new(global: SymRef) -> Self {
        let n = global.borrow().num_variables();
        let mut stack = vec![Value::Invalid; n as usize];
        for i in 0..n {
            let t = global.borrow().variable_type(i);
            match t.ty {
                AsType::Array => stack[i as usize] = Value::Array(LocalArray::new_ref()),
                AsType::Bool => stack[i as usize] = Value::Bool(false),
                AsType::Char => stack[i as usize] = Value::Char(0),
                AsType::Dict => stack[i as usize] = Value::Dict(new_dict()),
                AsType::Int => stack[i as usize] = Value::Int(0),
                AsType::Float => stack[i as usize] = Value::Float(0.0),
                AsType::Matrix | AsType::String | AsType::ObjectRef => {
                    stack[i as usize] = Value::Null
                }
                _ => {}
            }
        }
        Interpreter {
            cur: global.clone(),
            global,
            rvalue: Value::Invalid,
            stack,
            sp: 0,
            has_returned: false,
            obj_assign: false,
        }
    }

    /// `Interpret(node)`: the exit code of the script.
    pub fn interpret(&mut self, node: &Node) -> i32 {
        self.visit(node);
        if self.has_returned {
            let v = self.rvalue.clone();
            return self.as_int(&v, node);
        }
        0
    }

    // ---- errors ---------------------------------------------------------------

    fn err(&self, e: IErr, node: &Node, line: u32) -> ! {
        let mut m: Vec<u8> = Vec::new();
        m.extend_from_slice(node.pos.filename.as_bytes());
        m.push(b':');
        m.extend_from_slice(node.pos.line.to_string().as_bytes());
        m.extend_from_slice(b": error: ");
        let suffix = format!("  (cDirectInterpretASTVisitor.cc:{line})\n");
        let body = match &e {
            IErr::CannotResizeMatrixRow => "cannot directly resize matrix row".to_string(),
            IErr::DivisionByZero => "division by zero".to_string(),
            IErr::IndexError => "index operation failed".to_string(),
            IErr::IndexOutOfBounds => "array index out of bounds".to_string(),
            IErr::InvalidArraySize => "invalid array dimension".to_string(),
            IErr::KeyNotFound => "key not found".to_string(),
            IErr::MatrixOpTypeMismatch(op, t) => {
                format!("matrix '{op}' undefined for contained value of type '{t}'")
            }
            IErr::MatrixSizeMismatch(op) => format!("matrix size mismatch for '{op}' operation"),
            IErr::NobjMethodLookupFailed(m_, t) => format!("method '{m_}' not supported by '{t}'"),
            IErr::NobjTypeMismatch(o, t) => {
                format!("expected object of type '{o}', received '{t}'")
            }
            IErr::ObjectAssignFail => "aggregate assignment failed".to_string(),
            IErr::TypeCast(a, b) => format!("cannot convert '{a}' to '{b}'"),
            IErr::UndefinedTypeOp(op, t) => format!("'{op}' operation undefined for type '{t}'"),
            IErr::UnpackValueTooLarge => "unpack value too large".to_string(),
            IErr::UnpackValueTooSmall => "unpack value too small".to_string(),
            IErr::Native(s) => s.clone(),
            IErr::Internal => {
                m.extend_from_slice(
                    format!("internal interpreter error at cDirectInterpretASTVisitor.cc:{line}\n")
                        .as_bytes(),
                );
                output::err_write(&m);
                output::exit(AS_EXIT_FAIL_INTERPRET);
            }
        };
        m.extend_from_slice(body.as_bytes());
        if matches!(e, IErr::Native(_)) {
            m.push(b'\n');
        } else {
            m.extend_from_slice(suffix.as_bytes());
        }
        output::err_write(&m);
        output::exit(AS_EXIT_FAIL_INTERPRET)
    }

    fn tc(&self, from: &TypeInfo, to: AsType, node: &Node, line: u32) -> ! {
        self.err(IErr::TypeCast(map_type(from), map_ty(to)), node, line)
    }

    // ---- conversions (`asArray`, `asBool`, ...) ---------------------------------

    fn as_array(&mut self, v: &Value, node: &Node) -> ArrRef {
        match v {
            Value::Array(a) => a.clone(),
            Value::Matrix(m) => {
                let n = mat_num_rows(m);
                let arr = LocalArray::sized(n as usize);
                for i in 0..n as usize {
                    let row = m.borrow().rw()[i].clone();
                    let copy = arr_copy_new(&row);
                    arr_set(&arr, i, &Value::Array(copy));
                }
                mat_remove_ref(m);
                arr
            }
            Value::Str(s) => {
                let b = sread(s);
                let arr = LocalArray::sized(b.len());
                for (i, &c) in b.iter().enumerate() {
                    arr_set(&arr, i, &Value::Char(c as i8));
                }
                sdelete(s);
                arr
            }
            _ => self.tc(&val_info(v), AsType::Array, node, ln::AS_ARRAY),
        }
    }

    fn as_bool(&mut self, v: &Value, node: &Node) -> bool {
        match v {
            Value::Array(a) => {
                let r = !a.borrow().st().is_empty();
                arr_remove_ref(a);
                r
            }
            Value::Dict(d) => {
                let r = !d.borrow().mp().is_empty();
                dict_remove_ref(d);
                r
            }
            Value::Matrix(m) => {
                let r = mat_num_rows(m) != 0 && m.borrow().sz_y != 0;
                mat_remove_ref(m);
                r
            }
            Value::Bool(b) => *b,
            Value::Char(c) => *c != 0,
            Value::Float(f) => *f != 0.0,
            Value::Int(i) => *i != 0,
            Value::Str(s) => {
                let r = !sread(s).is_empty();
                sdelete(s);
                r
            }
            Value::Ref(_) | Value::NObj(..) => self.err(IErr::Internal, node, ln::AS_BOOL_OBJ),
            _ => self.tc(&val_info(v), AsType::Bool, node, ln::AS_BOOL),
        }
    }

    fn as_char(&mut self, v: &Value, node: &Node) -> i8 {
        match v {
            Value::Bool(b) => *b as i8,
            Value::Char(c) => *c,
            Value::Int(i) => *i as i8,
            _ => self.tc(&val_info(v), AsType::Char, node, ln::AS_CHAR),
        }
    }

    fn as_dict(&mut self, v: &Value, node: &Node) -> DictRef {
        match v {
            Value::Dict(d) => d.clone(),
            Value::Array(a) => {
                let dict = new_dict();
                let n = a.borrow().st().len();
                for i in 0..n {
                    let e = a.borrow().st()[i].clone();
                    dict_set(&dict, &Value::Int(i as i32), &e);
                }
                arr_remove_ref(a);
                dict
            }
            // QUIRK: the message names `char` as the target type.
            _ => self.tc(&val_info(v), AsType::Char, node, ln::AS_DICT),
        }
    }

    fn as_int(&mut self, v: &Value, node: &Node) -> i32 {
        match v {
            Value::Bool(b) => *b as i32,
            Value::Char(c) => *c as i32,
            Value::Int(i) => *i,
            Value::Float(f) => *f as i32,
            Value::Str(s) => {
                let r = as_int(&String::from_utf8_lossy(&sread(s)));
                sdelete(s);
                r
            }
            _ => self.tc(&val_info(v), AsType::Int, node, ln::AS_INT),
        }
    }

    fn as_float(&mut self, v: &Value, node: &Node) -> f64 {
        match v {
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Char(c) => *c as f64,
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            Value::Str(s) => {
                let r = as_double(&String::from_utf8_lossy(&sread(s)));
                sdelete(s);
                r
            }
            _ => self.tc(&val_info(v), AsType::Float, node, ln::AS_FLOAT),
        }
    }

    fn as_matrix(&mut self, v: &Value, node: &Node) -> MatRef {
        match v {
            Value::Bool(_) | Value::Char(_) | Value::Float(_) | Value::Int(_) => {
                let mat = new_matrix();
                mat_resize(&mat, 1, 1);
                let row = mat.borrow().rw()[0].clone();
                arr_set(&row, 0, v);
                mat
            }
            Value::Matrix(m) => m.clone(),
            _ => self.tc(&val_info(v), AsType::Matrix, node, ln::AS_MATRIX),
        }
    }

    fn as_native(&mut self, info: &str, v: &Value, node: &Node) -> (Rc<dyn NativeObject>, String) {
        match v {
            Value::NObj(o, i) => {
                if i != info {
                    self.err(
                        IErr::NobjTypeMismatch(info.to_string(), i.clone()),
                        node,
                        ln::AS_NOBJ_MISMATCH,
                    );
                }
                (o.clone(), i.clone())
            }
            _ => self.tc(&val_info(v), AsType::ObjectRef, node, ln::AS_NOBJ),
        }
    }

    fn as_string(&mut self, v: &Value, node: &Node) -> StrRef {
        match v {
            Value::Bool(b) => snew((if *b { "1" } else { "0" }).as_bytes().to_vec()),
            Value::Char(c) => snew(vec![*c as u8]),
            Value::Int(i) => snew(i.to_string().into_bytes()),
            Value::Float(f) => snew(fmt_fixed(*f, 6).into_bytes()),
            Value::Str(s) => s.clone(),
            Value::Array(a) => {
                let s = format!("< array({}) >", a.borrow().st().len());
                arr_remove_ref(a);
                snew(s.into_bytes())
            }
            Value::Dict(d) => {
                let s = format!("< dict({}) >", d.borrow().mp().len());
                dict_remove_ref(d);
                snew(s.into_bytes())
            }
            Value::Matrix(m) => {
                snew(format!("< matrix({}, {}) >", mat_num_rows(m), m.borrow().sz_y).into_bytes())
            }
            Value::NObj(o, _) => snew(
                format!(
                    "< {} object @ {:p} >",
                    o.type_name(),
                    Rc::as_ptr(o) as *const u8
                )
                .into_bytes(),
            ),
            _ => self.tc(&val_info(v), AsType::String, node, ln::AS_STRING),
        }
    }

    // ---- statements ---------------------------------------------------------------

    pub fn visit(&mut self, node: &Node) {
        match &node.kind {
            Kind::Null => output::crash(),
            Kind::Assignment {
                expr, id, global, ..
            } => self.visit_assignment(node, expr, id.get(), global.get()),
            Kind::ArgumentList(_) => self.err(IErr::Internal, node, ln::ARGLIST),
            Kind::ObjectAssignment { target, expr } => {
                self.obj_assign = true;
                self.visit(target);
                self.obj_assign = false;
                let obj = match &self.rvalue {
                    Value::Ref(r) => r.clone(),
                    _ => output::crash(),
                };

                self.visit(expr);

                let val = self.rvalue.clone();
                if !self.ref_set_val(&obj, &val) {
                    cleanup(&val);
                    self.err(IErr::ObjectAssignFail, node, ln::OBJ_ASSIGN_FAIL);
                }
            }
            Kind::Return(e) => {
                self.visit(e);
                self.has_returned = true;
            }
            Kind::StatementList(stmts) => {
                for s in stmts {
                    if self.has_returned {
                        break;
                    }
                    self.visit(s);
                }
            }
            Kind::Foreach { var, values, code } => self.visit_foreach(node, var, values, code),
            Kind::If {
                cond,
                code,
                elifs,
                else_code,
            } => {
                self.visit(cond);
                let c = self.rvalue.clone();
                if self.as_bool(&c, node) {
                    self.visit(code);
                } else {
                    let mut exec = false;
                    for ei in elifs {
                        self.visit(&ei.cond);
                        let c = self.rvalue.clone();
                        if self.as_bool(&c, node) {
                            exec = true;
                            self.visit(&ei.code);
                            break;
                        }
                    }
                    if !exec {
                        if let Some(ec) = else_code {
                            self.visit(ec);
                        }
                    }
                }
            }
            Kind::While { cond, code } => {
                self.visit(cond);
                loop {
                    let c = self.rvalue.clone();
                    if !self.as_bool(&c, node) {
                        break;
                    }
                    self.visit(code);
                    self.visit(cond);
                }
            }
            Kind::FunctionDefinition { .. } => {}
            Kind::VariableDefinition {
                ty,
                assign,
                dims,
                id,
                ..
            } => self.visit_vardef(node, ty, assign, dims, id.get()),
            Kind::VariableDefinitionList(_) => self.err(IErr::Internal, node, ln::VARDEFLIST),
            Kind::Binary {
                op,
                left,
                right,
                ty,
                compare_ty,
            } => self.visit_binary(
                node,
                *op,
                left,
                right,
                ty.borrow().clone(),
                compare_ty.borrow().clone(),
            ),
            Kind::Unary { op, expr, .. } => self.visit_unary(node, *op, expr),
            Kind::BuiltInCall {
                args,
                builtin,
                target,
                ..
            } => self.visit_builtin(node, args, *builtin, target),
            Kind::FunctionCall {
                args,
                ty,
                id,
                global,
                func,
                ..
            } => self.visit_function_call(
                node,
                args,
                ty.borrow().clone(),
                id.get(),
                global.get(),
                func,
            ),
            Kind::Literal { ty, value } => self.visit_literal(node, ty, value),
            Kind::LiteralArray { values, is_matrix } => {
                self.visit_literal_array(node, values, *is_matrix)
            }
            Kind::LiteralDict(maps) => {
                let dict = new_dict();
                for m in maps {
                    self.visit(&m.idx);
                    let idx = self.rvalue.clone();
                    self.visit(&m.val);
                    let val = self.rvalue.clone();
                    dict_set(&dict, &idx, &val);
                }
                self.rvalue = Value::Dict(dict);
            }
            Kind::ObjectCall { object, name, args } => {
                self.visit_object_call(node, object, name, args)
            }
            Kind::ObjectReference { .. } => self.err(IErr::Internal, node, ln::OBJREF_INTERNAL),
            Kind::VariableReference { id, global, ty, .. } => {
                self.visit_varref(node, id.get(), global.get(), ty.borrow().clone())
            }
            Kind::UnpackTarget {
                nodes,
                last_wild,
                last_named,
                expr,
            } => self.visit_unpack(node, nodes, *last_wild, *last_named, expr),
        }
    }

    fn slot(&self, global: bool, id: i32) -> usize {
        (if global { 0 } else { self.sp }) + id as usize
    }

    fn sym(&self, global: bool) -> SymRef {
        if global {
            self.global.clone()
        } else {
            self.cur.clone()
        }
    }

    fn visit_assignment(&mut self, node: &Node, expr: &NodeRef, var_id: i32, global: bool) {
        let symtbl = self.sym(global);
        let sp = if global { 0 } else { self.sp };
        let slot = sp + var_id as usize;
        self.visit(expr);
        let rv = self.rvalue.clone();
        let vt = symtbl.borrow().variable_type(var_id);
        match vt.ty {
            AsType::Bool => {
                let v = self.as_bool(&rv, node);
                self.stack[slot] = Value::Bool(v);
            }
            AsType::Char => {
                let v = self.as_char(&rv, node);
                self.stack[slot] = Value::Char(v);
            }
            AsType::Float => {
                let v = self.as_float(&rv, node);
                self.stack[slot] = Value::Float(v);
            }
            AsType::Int => {
                let v = self.as_int(&rv, node);
                self.stack[slot] = Value::Int(v);
            }
            AsType::ObjectRef => {
                // The old object is released, then the C++ falls through into
                // the VAR case and stores the raw value and type.
                if matches!(self.stack[slot], Value::Null) {
                    output::crash();
                }
                let _ = self.as_native(&vt.info, &rv, node);
                self.stack[slot] = rv;
            }
            AsType::Var => {
                self.stack[slot] = rv;
            }
            AsType::Array => {
                if let Value::Array(a) = self.stack[slot].clone() {
                    arr_remove_ref(&a);
                }
                let a = self.as_array(&rv, node);
                self.stack[slot] = Value::Array(a);
            }
            AsType::Dict => {
                if let Value::Dict(d) = self.stack[slot].clone() {
                    dict_remove_ref(&d);
                }
                let d = self.as_dict(&rv, node);
                self.stack[slot] = Value::Dict(d);
            }
            AsType::Matrix => {
                match self.stack[slot].clone() {
                    Value::Matrix(m) => mat_remove_ref(&m),
                    _ => output::crash(),
                }
                let m = self.as_matrix(&rv, node);
                self.stack[slot] = Value::Matrix(m);
            }
            AsType::String => {
                if let Value::Str(old) = self.stack[slot].clone() {
                    sdelete(&old);
                }
                let s = self.as_string(&rv, node);
                self.stack[slot] = Value::Str(s);
            }
            _ => self.err(IErr::Internal, node, ln::ASSIGN_DEFAULT),
        }
    }

    /// Store `val` (converted to `var_type`) into a variable slot; the shared
    /// tail of foreach and unpack (`asX(val.type, val.value)`).
    fn assign_converted(
        &mut self,
        slot: usize,
        var_type: &TypeInfo,
        val: &Value,
        node: &Node,
        default_line: u32,
    ) {
        match var_type.ty {
            AsType::Bool => {
                let v = self.as_bool(val, node);
                self.stack[slot] = Value::Bool(v);
            }
            AsType::Char => {
                let v = self.as_char(val, node);
                self.stack[slot] = Value::Char(v);
            }
            AsType::Float => {
                let v = self.as_float(val, node);
                self.stack[slot] = Value::Float(v);
            }
            AsType::Int => {
                let v = self.as_int(val, node);
                self.stack[slot] = Value::Int(v);
            }
            AsType::ObjectRef => {
                if matches!(self.stack[slot], Value::Null) {
                    output::crash();
                }
                let (o, i) = self.as_native(&var_type.info, val, node);
                self.stack[slot] = Value::NObj(o, i);
            }
            AsType::Array => {
                if let Value::Array(a) = self.stack[slot].clone() {
                    arr_remove_ref(&a);
                }
                let a = self.as_array(val, node);
                self.stack[slot] = Value::Array(a);
            }
            AsType::Dict => {
                if let Value::Dict(d) = self.stack[slot].clone() {
                    dict_remove_ref(&d);
                }
                let d = self.as_dict(val, node);
                self.stack[slot] = Value::Dict(d);
            }
            AsType::Var => {
                self.stack[slot] = val.clone();
            }
            AsType::Matrix => {
                match self.stack[slot].clone() {
                    Value::Matrix(m) => mat_remove_ref(&m),
                    _ => output::crash(),
                }
                let m = self.as_matrix(val, node);
                self.stack[slot] = Value::Matrix(m);
            }
            AsType::String => {
                if let Value::Str(old) = self.stack[slot].clone() {
                    sdelete(&old);
                }
                let s = self.as_string(val, node);
                self.stack[slot] = Value::Str(s);
            }
            _ => self.err(IErr::Internal, node, default_line),
        }
    }

    fn visit_foreach(&mut self, node: &Node, var: &NodeRef, values: &NodeRef, code: &NodeRef) {
        let (var_id, var_type) = match &var.kind {
            Kind::VariableDefinition { id, ty, .. } => (id.get(), ty.clone()),
            _ => output::crash(),
        };
        self.visit(values);
        let rv = self.rvalue.clone();
        let arr = self.as_array(&rv, node);

        let var_idx = self.sp + var_id as usize;
        let mut i = 0usize;
        while i < arr.borrow().st().len() {
            let val = arr.borrow().st()[i].clone();
            self.assign_converted(var_idx, &var_type, &val, node, ln::FOREACH_DEFAULT);
            self.visit(code);
            i += 1;
        }
        arr_remove_ref(&arr);
    }

    fn visit_vardef(
        &mut self,
        node: &Node,
        ty: &TypeInfo,
        assign: &Option<NodeRef>,
        dims: &Option<NodeRef>,
        var_id: i32,
    ) {
        let slot = self.sp + var_id as usize;
        if let Some(ae) = assign {
            self.visit(ae);
            let rv = self.rvalue.clone();
            match ty.ty {
                AsType::Array => {
                    let a = self.as_array(&rv, node);
                    self.stack[slot] = Value::Array(a);
                }
                AsType::Bool => {
                    let v = self.as_bool(&rv, node);
                    self.stack[slot] = Value::Bool(v);
                }
                AsType::Char => {
                    let v = self.as_char(&rv, node);
                    self.stack[slot] = Value::Char(v);
                }
                AsType::Dict => {
                    let d = self.as_dict(&rv, node);
                    self.stack[slot] = Value::Dict(d);
                }
                AsType::Float => {
                    let v = self.as_float(&rv, node);
                    self.stack[slot] = Value::Float(v);
                }
                AsType::Int => {
                    let v = self.as_int(&rv, node);
                    self.stack[slot] = Value::Int(v);
                }
                AsType::ObjectRef => {
                    let (o, i) = self.as_native(&ty.info, &rv, node);
                    self.stack[slot] = Value::NObj(o, i);
                }
                AsType::Matrix => {
                    let m = self.as_matrix(&rv, node);
                    self.stack[slot] = Value::Matrix(m);
                }
                AsType::String => {
                    if let Value::Str(old) = self.stack[slot].clone() {
                        sdelete(&old);
                    }
                    let s = self.as_string(&rv, node);
                    self.stack[slot] = Value::Str(s);
                }
                AsType::Var => {
                    self.stack[slot] = rv;
                }
                _ => self.err(IErr::Internal, node, ln::VARDEF_DEFAULT),
            }
        } else if let Some(d) = dims {
            let items = args_vec(&Some(d.clone())).unwrap_or_default();
            if ty.ty == AsType::Array {
                self.visit(&items[0]);
                let arr = LocalArray::new_ref();
                let rv = self.rvalue.clone();
                let n = self.as_int(&rv, node);
                arr_resize(&arr, n);
                self.stack[slot] = Value::Array(arr);
            } else if ty.ty == AsType::Matrix {
                self.visit(&items[0]);
                let rv = self.rvalue.clone();
                let sz_x = self.as_int(&rv, node);
                // QUIRK: a single dimension makes `it.Next()` NULL (SIGSEGV).
                let second = match items.get(1) {
                    Some(s) => s.clone(),
                    None => output::crash(),
                };
                self.visit(&second);
                let rv = self.rvalue.clone();
                let sz_y = self.as_int(&rv, node);
                let mat = new_matrix();
                mat_resize(&mat, sz_x, sz_y);
                self.stack[slot] = Value::Matrix(mat);
            } else {
                self.err(IErr::Internal, node, ln::VARDEF_DIMS);
            }
        }
        // else: native object ref (@AS_TODO in the C++) or nothing to do
    }

    // ---- expressions ---------------------------------------------------------------

    fn visit_binary(
        &mut self,
        node: &Node,
        op: Tok,
        left: &NodeRef,
        right: &NodeRef,
        ty: TypeInfo,
        compare_ty: TypeInfo,
    ) {
        self.visit(left);
        let lval = self.rvalue.clone();
        self.visit(right);
        let rval = self.rvalue.clone();
        let ltype = lval.ty();
        let rtype = rval.ty();

        match op {
            Tok::ArrRange => {
                let l = self.as_int(&lval, node);
                let r = self.as_int(&rval, node);
                let sz = r.wrapping_sub(l).wrapping_abs().wrapping_add(1);
                let arr = LocalArray::sized(sz.max(0) as usize);
                if r > l {
                    for i in 0..sz {
                        arr_set(&arr, i as usize, &Value::Int(l.wrapping_add(i)));
                    }
                } else {
                    for i in 0..sz {
                        arr_set(&arr, i as usize, &Value::Int(l.wrapping_sub(i)));
                    }
                }
                self.rvalue = Value::Array(arr);
            }
            Tok::ArrExpan => {
                let n = self.as_int(&rval, node);
                if n < 0 {
                    self.err(IErr::InvalidArraySize, node, ln::RANGE_INVALID);
                }
                let arr = LocalArray::sized(n as usize);
                for i in 0..n as usize {
                    arr_set(&arr, i, &lval);
                }
                self.rvalue = Value::Array(arr);
            }
            Tok::OpLogicAnd | Tok::OpLogicOr => {
                let l = self.as_bool(&lval, node);
                let r = self.as_bool(&rval, node);
                self.rvalue = Value::Bool(if op == Tok::OpLogicAnd {
                    l && r
                } else {
                    l || r
                });
            }
            Tok::OpBitAnd | Tok::OpBitOr => {
                let mut rettype = ty.clone();
                if is_plain(&rettype, AsType::Runtime) {
                    rettype = TypeInfo::new(get_runtime_type(ltype, rtype, false));
                }
                if is_plain(&rettype, AsType::Char) {
                    let l = self.as_char(&lval, node) as i32;
                    let r = self.as_char(&rval, node) as i32;
                    let v = if op == Tok::OpBitAnd { l & r } else { l | r };
                    self.rvalue = Value::Char(v as i8);
                } else if is_plain(&rettype, AsType::Int) {
                    let l = self.as_int(&lval, node);
                    let r = self.as_int(&rval, node);
                    self.rvalue = Value::Int(if op == Tok::OpBitAnd { l & r } else { l | r });
                } else {
                    self.err(
                        IErr::UndefinedTypeOp(map_token(op).into(), map_type(&rettype)),
                        node,
                        ln::BIT_UNDEF,
                    );
                }
            }
            Tok::OpEq | Tok::OpNeq => {
                let mut comptype = compare_ty.ty;
                if comptype == AsType::Runtime {
                    comptype = get_runtime_type(ltype, rtype, false);
                }
                let eq = op == Tok::OpEq;
                let res = match comptype {
                    AsType::Bool => {
                        let l = self.as_bool(&lval, node);
                        let r = self.as_bool(&rval, node);
                        if eq {
                            l == r
                        } else {
                            l != r
                        }
                    }
                    AsType::Char | AsType::Int => {
                        let l = self.as_int(&lval, node);
                        let r = self.as_int(&rval, node);
                        if eq {
                            l == r
                        } else {
                            l != r
                        }
                    }
                    AsType::Float => {
                        let l = self.as_float(&lval, node);
                        let r = self.as_float(&rval, node);
                        if eq {
                            l == r
                        } else {
                            l != r
                        }
                    }
                    AsType::String => {
                        let l = self.as_string(&lval, node);
                        let r = self.as_string(&rval, node);
                        let (lb, rb) = (sread(&l), sread(&r));
                        sdelete(&l);
                        sdelete(&r);
                        if eq {
                            lb == rb
                        } else {
                            lb != rb
                        }
                    }
                    _ => self.err(
                        IErr::UndefinedTypeOp(map_token(op).into(), map_ty(comptype)),
                        node,
                        ln::EQ_UNDEF,
                    ),
                };
                self.rvalue = Value::Bool(res);
            }
            Tok::OpLe | Tok::OpGe | Tok::OpLt | Tok::OpGt => {
                let mut comptype = compare_ty.ty;
                if comptype == AsType::Runtime {
                    comptype = get_runtime_type(ltype, rtype, false);
                }
                let res = match comptype {
                    AsType::Char | AsType::Int => {
                        let l = self.as_int(&lval, node);
                        let r = self.as_int(&rval, node);
                        match op {
                            Tok::OpLe => l <= r,
                            Tok::OpGe => l >= r,
                            Tok::OpLt => l < r,
                            Tok::OpGt => l > r,
                            _ => self.err(IErr::Internal, node, ln::REL_INTERNAL1),
                        }
                    }
                    AsType::Float => {
                        let l = self.as_float(&lval, node);
                        let r = self.as_float(&rval, node);
                        match op {
                            Tok::OpLe => l <= r,
                            Tok::OpGe => l >= r,
                            Tok::OpLt => l < r,
                            Tok::OpGt => l > r,
                            _ => self.err(IErr::Internal, node, ln::REL_INTERNAL2),
                        }
                    }
                    _ => self.err(
                        IErr::UndefinedTypeOp(map_token(op).into(), map_ty(comptype)),
                        node,
                        ln::REL_UNDEF,
                    ),
                };
                self.rvalue = Value::Bool(res);
            }
            Tok::OpAdd => {
                let mut rettype = ty.ty;
                if rettype == AsType::Runtime {
                    rettype = get_runtime_type(ltype, rtype, true);
                }
                let v = match rettype {
                    AsType::Char => {
                        let l = self.as_char(&lval, node) as i32;
                        let r = self.as_char(&rval, node) as i32;
                        Value::Char((l + r) as i8)
                    }
                    AsType::Int => {
                        let l = self.as_int(&lval, node);
                        let r = self.as_int(&rval, node);
                        Value::Int(l.wrapping_add(r))
                    }
                    AsType::Float => {
                        let l = self.as_float(&lval, node);
                        let r = self.as_float(&rval, node);
                        Value::Float(l + r)
                    }
                    AsType::Matrix => {
                        let _ = self.as_matrix(&lval, node);
                        let _ = self.as_matrix(&rval, node);
                        self.err(IErr::Internal, node, ln::MAT_ADD)
                    }
                    AsType::String => {
                        let l = self.as_string(&lval, node);
                        let r = self.as_string(&rval, node);
                        let mut lb = sread(&l);
                        lb.extend_from_slice(&sread(&r));
                        sdelete(&l);
                        sdelete(&r);
                        Value::Str(snew(lb))
                    }
                    AsType::Array => {
                        let l = self.as_array(&lval, node);
                        let r = self.as_array(&rval, node);
                        let a = arr_concat_new(&l, &r);
                        arr_remove_ref(&l);
                        arr_remove_ref(&r);
                        Value::Array(a)
                    }
                    _ => self.err(
                        IErr::UndefinedTypeOp(map_token(Tok::OpAdd).into(), map_ty(rettype)),
                        node,
                        ln::ADD_UNDEF,
                    ),
                };
                self.rvalue = v;
            }
            Tok::OpSub | Tok::OpMul => {
                let mut rettype = ty.ty;
                if rettype == AsType::Runtime {
                    rettype = get_runtime_type(ltype, rtype, false);
                }
                let sub = op == Tok::OpSub;
                let v = match rettype {
                    AsType::Char => {
                        let l = self.as_char(&lval, node) as i32;
                        let r = self.as_char(&rval, node) as i32;
                        Value::Char(if sub {
                            l.wrapping_sub(r)
                        } else {
                            l.wrapping_mul(r)
                        } as i8)
                    }
                    AsType::Int => {
                        let l = self.as_int(&lval, node);
                        let r = self.as_int(&rval, node);
                        Value::Int(if sub {
                            l.wrapping_sub(r)
                        } else {
                            l.wrapping_mul(r)
                        })
                    }
                    AsType::Float => {
                        let l = self.as_float(&lval, node);
                        let r = self.as_float(&rval, node);
                        Value::Float(if sub { l - r } else { l * r })
                    }
                    AsType::Matrix => {
                        let m1 = self.as_matrix(&lval, node);
                        let m2 = self.as_matrix(&rval, node);
                        if sub {
                            self.err(IErr::Internal, node, ln::MAT_SUB)
                        } else {
                            self.matrix_multiply(&m1, &m2, node)
                        }
                    }
                    _ => self.err(
                        IErr::UndefinedTypeOp(map_token(Tok::OpAdd).into(), map_ty(rettype)),
                        node,
                        if sub { ln::SUB_UNDEF } else { ln::MUL_UNDEF },
                    ),
                };
                self.rvalue = v;
            }
            Tok::OpDiv | Tok::OpMod => {
                let mut rettype = ty.ty;
                if rettype == AsType::Runtime {
                    rettype = get_runtime_type(ltype, rtype, false);
                }
                let div = op == Tok::OpDiv;
                let v = match rettype {
                    AsType::Char => {
                        let r = self.as_char(&rval, node) as i32;
                        if r == 0 {
                            self.err(
                                IErr::DivisionByZero,
                                node,
                                if div {
                                    ln::DIV_ZERO_CHAR
                                } else {
                                    ln::MOD_ZERO_CHAR
                                },
                            );
                        }
                        let l = self.as_char(&lval, node) as i32;
                        Value::Char(if div { l / r } else { l % r } as i8)
                    }
                    AsType::Int => {
                        let r = self.as_int(&rval, node);
                        if r == 0 {
                            self.err(
                                IErr::DivisionByZero,
                                node,
                                if div {
                                    ln::DIV_ZERO_INT
                                } else {
                                    ln::MOD_ZERO_INT
                                },
                            );
                        }
                        let l = self.as_int(&lval, node);
                        Value::Int(if div {
                            l.wrapping_div(r)
                        } else {
                            l.wrapping_rem(r)
                        })
                    }
                    AsType::Float => {
                        let r = self.as_float(&rval, node);
                        if r == 0.0 {
                            self.err(
                                IErr::DivisionByZero,
                                node,
                                if div {
                                    ln::DIV_ZERO_FLOAT
                                } else {
                                    ln::MOD_ZERO_FLOAT
                                },
                            );
                        }
                        let l = self.as_float(&lval, node);
                        Value::Float(if div { l / r } else { l % r })
                    }
                    _ => self.err(
                        IErr::UndefinedTypeOp(map_token(Tok::OpAdd).into(), map_ty(rettype)),
                        node,
                        if div { ln::DIV_UNDEF } else { ln::MOD_UNDEF },
                    ),
                };
                self.rvalue = v;
            }
            Tok::IdxOpen => self.visit_index(node, &lval, &rval),
            _ => self.err(IErr::Internal, node, ln::BIN_INTERNAL),
        }
    }

    fn visit_index(&mut self, node: &Node, lval: &Value, rval: &Value) {
        if self.obj_assign {
            let obj = match lval {
                Value::Ref(r) => r.clone(),
                _ => output::crash(),
            };
            let idx = rval.clone();
            let o_val = match self.ref_get_val(&obj) {
                Some(v) => v,
                None => {
                    cleanup(&idx);
                    self.err(IErr::IndexError, node, ln::IDX_INDEX_ERROR)
                }
            };
            match o_val {
                Value::Array(_) | Value::Dict(_) | Value::Matrix(_) | Value::Str(_) => {}
                _ => self.err(
                    IErr::UndefinedTypeOp(
                        map_token(Tok::IdxOpen).into(),
                        map_type(&val_info(&o_val)),
                    ),
                    node,
                    ln::IDX_OBJ_UNDEF,
                ),
            }
            self.rvalue = Value::Ref(Rc::new(ObjRef::Index(obj, idx)));
            return;
        }

        match lval {
            Value::Array(arr) => {
                let idx = self.as_int(rval, node);
                if idx < 0 || idx as usize >= arr.borrow().st().len() {
                    self.err(IErr::IndexOutOfBounds, node, ln::IDX_ARR_OOB);
                }
                let val = arr.borrow().st()[idx as usize].clone();
                self.rvalue = val;
                arr_remove_ref(arr);
            }
            Value::Dict(dict) => {
                let idx = rval.clone();
                let val = match dict_get(dict, &idx) {
                    Some(v) => v,
                    None => {
                        cleanup(&idx);
                        self.err(IErr::KeyNotFound, node, ln::IDX_KEY)
                    }
                };
                cleanup(&idx);
                self.rvalue = val;
                dict_remove_ref(dict);
            }
            Value::Matrix(mat) => {
                let idx = self.as_int(rval, node);
                if idx < 0 || idx >= mat_num_rows(mat) {
                    self.err(IErr::IndexOutOfBounds, node, ln::IDX_MAT_OOB);
                }
                let row = mat.borrow().rw()[idx as usize].clone();
                self.rvalue = Value::Array(row);
                mat_remove_ref(mat);
            }
            Value::Str(s) => {
                let idx = self.as_int(rval, node);
                let b = sread(s);
                if idx < 0 || idx as usize >= b.len() {
                    self.err(IErr::IndexOutOfBounds, node, ln::IDX_STR_OOB);
                }
                self.rvalue = Value::Char(b[idx as usize] as i8);
                sdelete(s);
            }
            _ => self.err(
                IErr::TypeCast(map_ty(lval.ty()), map_ty(AsType::Array)),
                node,
                ln::IDX_CAST,
            ),
        }
    }

    fn visit_unary(&mut self, node: &Node, op: Tok, expr: &NodeRef) {
        self.visit(expr);
        match op {
            Tok::OpBitNot => match &self.rvalue {
                Value::Char(c) => self.rvalue = Value::Char(!*c),
                Value::Int(i) => self.rvalue = Value::Int(!*i),
                v => self.err(
                    IErr::UndefinedTypeOp(map_token(Tok::OpBitNot).into(), map_type(&val_info(v))),
                    node,
                    ln::UN_BITNOT_UNDEF,
                ),
            },
            Tok::OpLogicNot => {
                let v = self.rvalue.clone();
                let b = self.as_bool(&v, node);
                self.rvalue = Value::Bool(!b);
            }
            Tok::OpSub => match &self.rvalue {
                Value::Char(c) => self.rvalue = Value::Char(c.wrapping_neg()),
                Value::Int(i) => self.rvalue = Value::Int(i.wrapping_neg()),
                Value::Float(f) => self.rvalue = Value::Float(-*f),
                v => self.err(
                    IErr::UndefinedTypeOp(map_token(Tok::OpBitNot).into(), map_type(&val_info(v))),
                    node,
                    ln::UN_SUB_UNDEF,
                ),
            },
            _ => self.err(IErr::Internal, node, ln::UN_INTERNAL),
        }
    }

    fn first_arg(&mut self, args: &Option<NodeRef>) -> NodeRef {
        match args_vec(args) {
            Some(v) if !v.is_empty() => v[0].clone(),
            _ => output::crash(),
        }
    }

    fn visit_builtin(
        &mut self,
        node: &Node,
        args: &Option<NodeRef>,
        builtin: BuiltIn,
        target: &Option<NodeRef>,
    ) {
        let trgt = target.clone();
        match builtin {
            BuiltIn::CastBool => {
                let a = self.first_arg(args);
                self.visit(&a);
                let v = self.rvalue.clone();
                let b = self.as_bool(&v, node);
                self.rvalue = Value::Bool(b);
            }
            BuiltIn::CastChar => {
                let a = self.first_arg(args);
                self.visit(&a);
                let v = self.rvalue.clone();
                let b = self.as_char(&v, node);
                self.rvalue = Value::Char(b);
            }
            BuiltIn::CastInt => {
                let a = self.first_arg(args);
                self.visit(&a);
                let v = self.rvalue.clone();
                let b = self.as_int(&v, node);
                self.rvalue = Value::Int(b);
            }
            BuiltIn::CastFloat => {
                let a = self.first_arg(args);
                self.visit(&a);
                let v = self.rvalue.clone();
                let b = self.as_float(&v, node);
                self.rvalue = Value::Float(b);
            }
            BuiltIn::CastString => {
                let a = self.first_arg(args);
                self.visit(&a);
                let v = self.rvalue.clone();
                let b = self.as_string(&v, node);
                self.rvalue = Value::Str(b);
            }
            BuiltIn::IsArray
            | BuiltIn::IsBool
            | BuiltIn::IsChar
            | BuiltIn::IsDict
            | BuiltIn::IsInt
            | BuiltIn::IsFloat
            | BuiltIn::IsMatrix
            | BuiltIn::IsString => {
                let a = self.first_arg(args);
                self.visit(&a);
                cleanup(&self.rvalue.clone());
                let t = self.rvalue.ty();
                let want = match builtin {
                    BuiltIn::IsArray => AsType::Array,
                    BuiltIn::IsBool => AsType::Bool,
                    BuiltIn::IsChar => AsType::Char,
                    BuiltIn::IsDict => AsType::Dict,
                    BuiltIn::IsInt => AsType::Int,
                    BuiltIn::IsFloat => AsType::Float,
                    BuiltIn::IsMatrix => AsType::Matrix,
                    _ => AsType::String,
                };
                self.rvalue = Value::Bool(t == want);
            }
            BuiltIn::Clear => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Dict(d) => {
                        dict_clear(&d);
                        dict_remove_ref(&d);
                    }
                    Value::Matrix(m) => {
                        mat_resize(&m, 0, 0);
                        mat_remove_ref(&m);
                    }
                    Value::Array(a) => {
                        arr_resize(&a, 0);
                        arr_remove_ref(&a);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp("clear".into(), map_type(&val_info(&v))),
                        node,
                        ln::CLEAR_UNDEF,
                    ),
                }
            }
            BuiltIn::Copy => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Array(a) => {
                        let arr = arr_copy_new(&a);
                        arr_remove_ref(&a);
                        self.rvalue = Value::Array(arr);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp("copy".into(), map_type(&val_info(&v))),
                        node,
                        ln::COPY_UNDEF,
                    ),
                }
            }
            BuiltIn::HasKey => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Dict(dict) => {
                        let a = self.first_arg(args);
                        self.visit(&a);
                        let idx = self.rvalue.clone();
                        let has = dict.borrow().mp().has(&AggKey(idx.clone()));
                        self.rvalue = Value::Bool(has);
                        cleanup(&idx);
                        dict_remove_ref(&dict);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp("keys".into(), map_type(&val_info(&v))),
                        node,
                        ln::HASKEY_UNDEF,
                    ),
                }
            }
            BuiltIn::Keys | BuiltIn::Values => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Dict(dict) => {
                        let arr = LocalArray::new_ref();
                        let entries: Vec<Value> = dict
                            .borrow()
                            .map
                            .iter()
                            .map(|e| {
                                if builtin == BuiltIn::Keys {
                                    e.0 .0.clone()
                                } else {
                                    e.1.clone()
                                }
                            })
                            .collect();
                        arr_set_with_entries(&arr, entries);
                        dict_remove_ref(&dict);
                        self.rvalue = Value::Array(arr);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp(
                            if builtin == BuiltIn::Keys {
                                "keys"
                            } else {
                                "values"
                            }
                            .into(),
                            map_type(&val_info(&v)),
                        ),
                        node,
                        if builtin == BuiltIn::Keys {
                            ln::KEYS_UNDEF
                        } else {
                            ln::VALUES_UNDEF
                        },
                    ),
                }
            }
            BuiltIn::Len => {
                let t = trgt.expect("target");
                self.visit(&t);
                if let Value::Str(s) = &self.rvalue {
                    let sz = sread(s).len() as i32;
                    sdelete(s);
                    self.rvalue = Value::Int(sz);
                } else {
                    let v = self.rvalue.clone();
                    let arr = self.as_array(&v, node);
                    let sz = arr.borrow().st().len() as i32;
                    arr_remove_ref(&arr);
                    self.rvalue = Value::Int(sz);
                }
            }
            BuiltIn::Remove => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Dict(dict) => {
                        if dict.borrow().rc <= 1 {
                            dict_remove_ref(&dict);
                            return;
                        }
                        let a = self.first_arg(args);
                        self.visit(&a);
                        let idx = self.rvalue.clone();
                        let removed = dict.borrow_mut().mp_mut().remove(&AggKey(idx.clone()));
                        if let Some(r) = removed {
                            cleanup(&r);
                        }
                        cleanup(&idx);
                        dict_remove_ref(&dict);
                    }
                    Value::Array(arr) => {
                        if arr.borrow().rc <= 1 {
                            arr_remove_ref(&arr);
                            return;
                        }
                        let a = self.first_arg(args);
                        self.visit(&a);
                        let v = self.rvalue.clone();
                        let mut i = self.as_int(&v, node);
                        let size = arr.borrow().st().len() as i32;
                        if i < 0 || i >= size {
                            self.err(IErr::IndexOutOfBounds, node, ln::REMOVE_OOB);
                        }
                        while i < arr.borrow().st().len() as i32 - 1 {
                            let nv = arr.borrow().st()[i as usize + 1].clone();
                            arr_set(&arr, i as usize, &nv);
                            i += 1;
                        }
                        let n = arr.borrow().st().len() as i32 - 1;
                        arr_resize(&arr, n);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp("remove".into(), map_type(&val_info(&v))),
                        node,
                        ln::REMOVE_UNDEF,
                    ),
                }
            }
            BuiltIn::Resize => {
                let t = trgt.expect("target");
                self.visit(&t);
                match self.rvalue.clone() {
                    Value::Matrix(mat) => {
                        let av = match args_vec(args) {
                            Some(a) => a,
                            None => output::crash(),
                        };
                        if av.len() != 2 {
                            self.err(IErr::InvalidArraySize, node, ln::RESIZE_MAT_SIZE);
                        }
                        if mat.borrow().rc <= 1 {
                            mat_remove_ref(&mat);
                            return;
                        }
                        self.visit(&av[0]);
                        let v = self.rvalue.clone();
                        let sz_x = self.as_int(&v, node);
                        self.visit(&av[1]);
                        let v = self.rvalue.clone();
                        let sz_y = self.as_int(&v, node);
                        mat_resize(&mat, sz_x, sz_y);
                        mat_remove_ref(&mat);
                    }
                    Value::Array(arr) => {
                        if !arr.borrow().resizable {
                            self.err(IErr::CannotResizeMatrixRow, node, ln::RESIZE_ROW);
                        }
                        let av = match args_vec(args) {
                            Some(a) => a,
                            None => output::crash(),
                        };
                        if av.len() != 1 {
                            self.err(IErr::InvalidArraySize, node, ln::RESIZE_ARR_SIZE);
                        }
                        if arr.borrow().rc <= 1 {
                            arr_remove_ref(&arr);
                            return;
                        }
                        self.visit(&av[0]);
                        let v = self.rvalue.clone();
                        let sz = self.as_int(&v, node);
                        arr_resize(&arr, sz);
                        arr_remove_ref(&arr);
                    }
                    v => self.err(
                        IErr::UndefinedTypeOp("resize".into(), map_type(&val_info(&v))),
                        node,
                        ln::RESIZE_UNDEF,
                    ),
                }
            }
            BuiltIn::Unknown => self.err(IErr::Internal, node, ln::BUILTIN_INTERNAL),
        }
    }

    // ---- calls -----------------------------------------------------------------------

    fn cpp_param(
        &mut self,
        ty: &TypeInfo,
        v: &Value,
        node: &Node,
        line: u32,
        owned: &mut Vec<StrRef>,
    ) -> CppParam {
        match ty.ty {
            AsType::Bool => CppParam::Bool(self.as_bool(v, node)),
            AsType::Char => CppParam::Char(self.as_char(v, node)),
            AsType::Float => CppParam::Float(self.as_float(v, node)),
            AsType::Int => CppParam::Int(self.as_int(v, node)),
            AsType::String => {
                let r = self.as_string(v, node);
                let b = sread(&r);
                owned.push(r);
                CppParam::Str(b)
            }
            AsType::ObjectRef => {
                let (o, _) = self.as_native(&ty.info, v, node);
                CppParam::NObj(o)
            }
            _ => self.err(IErr::Internal, node, line),
        }
    }

    fn param_value(&mut self, ty: &TypeInfo, p: CppParam, node: &Node, line: u32) -> Value {
        match (ty.ty, p) {
            (AsType::Bool, CppParam::Bool(b)) => Value::Bool(b),
            (AsType::Char, CppParam::Char(c)) => Value::Char(c),
            (AsType::Float, CppParam::Float(f)) => Value::Float(f),
            (AsType::Int, CppParam::Int(i)) => Value::Int(i),
            (AsType::String, CppParam::Str(s)) => Value::Str(snew(s)),
            (AsType::ObjectRef, CppParam::NObj(o)) => Value::NObj(o, ty.info.clone()),
            (AsType::Void, _) => Value::Void,
            _ => self.err(IErr::Internal, node, line),
        }
    }

    fn visit_function_call(
        &mut self,
        node: &Node,
        args: &Option<NodeRef>,
        fty: TypeInfo,
        fun_id: i32,
        fglobal: bool,
        func: &RefCell<Option<Rc<dyn crate::library::ASFunction>>>,
    ) {
        let libfn = func.borrow().clone();
        if let Some(f) = libfn {
            let arity = f.arity();
            let mut cargs: Vec<CppParam> = vec![CppParam::None; arity];
            let mut owned: Vec<StrRef> = Vec::new();
            if arity > 0 {
                let av = match args_vec(args) {
                    Some(a) => a,
                    None => output::crash(),
                };
                for (i, a) in av.iter().enumerate().take(arity) {
                    self.visit(a);
                    let v = self.rvalue.clone();
                    let at = f.argument_type(i).clone();
                    cargs[i] = self.cpp_param(&at, &v, node, ln::LIBCALL_ARG, &mut owned);
                }
            }

            let ret = match f.call(&mut cargs) {
                Ok(r) => r,
                Err(m) => self.err(IErr::Native(m), node, 0),
            };

            match fty.ty {
                AsType::Void => {}
                _ => {
                    let v = self.param_value(&fty, ret, node, ln::LIBCALL_RET);
                    self.rvalue = v;
                }
            }
            if fty.ty == AsType::Void {
                self.rvalue = Value::Void;
            }

            for i in 0..arity {
                match f.argument_type(i).ty {
                    AsType::Bool
                    | AsType::Char
                    | AsType::Float
                    | AsType::Int
                    | AsType::String
                    | AsType::ObjectRef => {}
                    _ => self.err(IErr::Internal, node, ln::LIBCALL_CLEAN),
                }
            }
            for r in &owned {
                sdelete(r);
            }
            return;
        }

        // User defined function
        let prev_symtbl = self.cur.clone();
        let func_src = self.sym(fglobal);
        let func_symtbl = func_src
            .borrow()
            .function_symtbl(fun_id)
            .expect("function symtbl");
        let o_sp = self.sp;
        let sp = self.stack.len();
        let nvars = func_symtbl.borrow().num_variables();
        self.stack.resize(sp + nvars as usize, Value::Invalid);
        for i in 0..nvars {
            let t = func_symtbl.borrow().variable_type(i);
            let slot = sp + i as usize;
            self.stack[slot] = match t.ty {
                AsType::Array => Value::Array(LocalArray::new_ref()),
                AsType::Bool => Value::Bool(false),
                AsType::Char => Value::Char(0),
                AsType::Dict => Value::Dict(new_dict()),
                AsType::Int => Value::Int(0),
                AsType::Float => Value::Float(0.0),
                AsType::Matrix | AsType::ObjectRef | AsType::String => Value::Null,
                _ => Value::Invalid,
            };
        }

        // Process the arguments to the function
        // QUIRK: both `GetFunctionSignature(fun_id)->Iterator()` and
        // `node.GetArguments()->Iterator()` dereference NULL for a function
        // without parameters or a call without an argument list (SIGSEGV).
        let sig = match func_src.borrow().function_signature(fun_id) {
            Some(s) => s,
            None => output::crash(),
        };
        let argv = match args_vec(args) {
            Some(a) => a,
            None => output::crash(),
        };
        let sigv = match &sig.kind {
            Kind::VariableDefinitionList(l) => l.clone(),
            _ => Vec::new(),
        };
        let mut ai = 0usize;
        for arg_def in &sigv {
            let (var_id, def_assign) = match &arg_def.kind {
                Kind::VariableDefinition { id, assign, .. } => (id.get(), assign.clone()),
                _ => (0, None),
            };
            let arg = argv.get(ai).cloned();
            ai += 1;
            match arg {
                Some(a) => self.visit(&a),
                None => match def_assign {
                    Some(d) => self.visit(&d),
                    None => output::crash(),
                },
            }
            let rv = self.rvalue.clone();
            let vt = func_symtbl.borrow().variable_type(var_id);
            let slot = sp + var_id as usize;
            match vt.ty {
                AsType::Array => {
                    let a = self.as_array(&rv, node);
                    self.stack[slot] = Value::Array(a);
                }
                AsType::Bool => {
                    let v = self.as_bool(&rv, node);
                    self.stack[slot] = Value::Bool(v);
                }
                AsType::Char => {
                    let v = self.as_char(&rv, node);
                    self.stack[slot] = Value::Char(v);
                }
                AsType::Dict => {
                    let d = self.as_dict(&rv, node);
                    self.stack[slot] = Value::Dict(d);
                }
                AsType::Float => {
                    let v = self.as_float(&rv, node);
                    self.stack[slot] = Value::Float(v);
                }
                AsType::Int => {
                    let v = self.as_int(&rv, node);
                    self.stack[slot] = Value::Int(v);
                }
                AsType::ObjectRef => {
                    let (o, i) = self.as_native(&vt.info, &rv, node);
                    self.stack[slot] = Value::NObj(o, i);
                }
                AsType::Matrix => {
                    let m = self.as_matrix(&rv, node);
                    self.stack[slot] = Value::Matrix(m);
                }
                AsType::String => {
                    let s = self.as_string(&rv, node);
                    self.stack[slot] = Value::Str(s);
                }
                AsType::Var => {
                    self.stack[slot] = rv;
                }
                _ => self.err(IErr::Internal, node, ln::USRCALL_ARG),
            }
        }

        // Execute the function
        self.cur = func_symtbl.clone();
        self.sp = sp;
        let body = func_src
            .borrow()
            .function_definition(fun_id)
            .expect("function body");
        self.visit(&body);

        // Handle function return value
        let rv = self.rvalue.clone();
        match fty.ty {
            AsType::Array => {
                let a = self.as_array(&rv, node);
                self.rvalue = Value::Array(a);
            }
            AsType::Bool => {
                let v = self.as_bool(&rv, node);
                self.rvalue = Value::Bool(v);
            }
            AsType::Char => {
                let v = self.as_char(&rv, node);
                self.rvalue = Value::Char(v);
            }
            AsType::Dict => {
                let d = self.as_dict(&rv, node);
                self.rvalue = Value::Dict(d);
            }
            AsType::Float => {
                let v = self.as_float(&rv, node);
                self.rvalue = Value::Float(v);
            }
            AsType::Int => {
                let v = self.as_int(&rv, node);
                self.rvalue = Value::Int(v);
            }
            AsType::ObjectRef => {
                let (o, i) = self.as_native(&fty.info, &rv, node);
                self.rvalue = Value::NObj(o, i);
            }
            AsType::Matrix => {
                let m = self.as_matrix(&rv, node);
                self.rvalue = Value::Matrix(m);
            }
            AsType::String => {
                let s = self.as_string(&rv, node);
                self.rvalue = Value::Str(s);
            }
            AsType::Var => {}
            AsType::Void => {
                self.rvalue = Value::Void;
            }
            _ => self.err(IErr::Internal, node, ln::USRCALL_RET),
        }

        // Clean up variables in the current scope
        for i in 0..nvars {
            let slot = sp + i as usize;
            match self.stack[slot].clone() {
                Value::Array(a) => arr_remove_ref(&a),
                Value::Dict(d) => dict_remove_ref(&d),
                Value::Matrix(m) => mat_remove_ref(&m),
                Value::Str(s) => sdelete(&s),
                _ => {
                    // QUIRK: a NULL matrix of a declared-but-unset matrix
                    // variable is released through a NULL pointer.
                    let t = func_symtbl.borrow().variable_type(i);
                    if t.ty == AsType::Matrix && matches!(self.stack[slot], Value::Null) {
                        output::crash();
                    }
                }
            }
        }

        // Restore previous scope
        self.has_returned = false;
        self.stack.truncate(sp);
        self.sp = o_sp;
        self.cur = prev_symtbl;
    }

    fn visit_literal(&mut self, node: &Node, ty: &TypeInfo, value: &Bytes) {
        match ty.ty {
            AsType::Bool => self.rvalue = Value::Bool(value == b"true"),
            AsType::Char => self.rvalue = Value::Char(value.first().copied().unwrap_or(0) as i8),
            AsType::Int => self.rvalue = Value::Int(as_int(&String::from_utf8_lossy(value))),
            AsType::Float => self.rvalue = Value::Float(as_double(&String::from_utf8_lossy(value))),
            AsType::String => self.rvalue = Value::Str(snew(value.clone())),
            _ => self.err(IErr::Internal, node, ln::LITERAL_INTERNAL),
        }
    }

    fn visit_literal_array(&mut self, node: &Node, values: &Option<NodeRef>, is_matrix: bool) {
        // QUIRK: `vals->Iterator()` / `vals->GetSize()` on NULL for `{}`.
        let vals = match args_vec(values) {
            Some(v) => v,
            None => output::crash(),
        };
        if is_matrix {
            let mat = new_matrix();
            let mut sz_y: i32 = 1;
            let mut first = true;
            for (i, val) in vals.iter().enumerate() {
                self.visit(val);
                let rv = self.rvalue.clone();
                if first {
                    if let Value::Array(a) = &rv {
                        sz_y = a.borrow().st().len() as i32;
                    }
                }

                mat_resize(&mat, i as i32 + 1, sz_y);

                if sz_y > 1 {
                    match &rv {
                        Value::Array(a) => mat_set_row(&mat, i, a),
                        _ => self.err(IErr::InvalidArraySize, node, ln::LITMAT_SIZE),
                    }
                } else {
                    let row = mat.borrow().rw()[i].clone();
                    // `row->Set(0, ...)` on a row that may have zero columns
                    if row.borrow().st().is_empty() {
                        output::crash();
                    }
                    arr_set(&row, 0, &rv);
                }

                cleanup(&rv);
                first = false;
            }
            self.rvalue = Value::Matrix(mat);
        } else {
            let arr = LocalArray::sized(vals.len());
            for (i, val) in vals.iter().enumerate() {
                self.visit(val);
                let rv = self.rvalue.clone();
                arr_set(&arr, i, &rv);
            }
            self.rvalue = Value::Array(arr);
        }
    }

    fn visit_object_call(
        &mut self,
        node: &Node,
        object: &NodeRef,
        name: &Bytes,
        args: &Option<NodeRef>,
    ) {
        self.visit(object);
        let (nobj, info) = match &self.rvalue {
            Value::NObj(o, i) => (o.clone(), i.clone()),
            v => self.err(
                IErr::TypeCast(map_ty(v.ty()), map_ty(AsType::ObjectRef)),
                node,
                ln::OBJCALL_CAST,
            ),
        };
        let mid = match nobj.lookup_method(name) {
            Some(m) => m,
            None => self.err(
                IErr::NobjMethodLookupFailed(String::from_utf8_lossy(name).into_owned(), info),
                node,
                ln::OBJCALL_LOOKUP,
            ),
        };
        let arity = nobj.arity(mid);
        let mut cargs: Vec<CppParam> = vec![CppParam::None; arity];
        let mut owned: Vec<StrRef> = Vec::new();
        if arity > 0 {
            let av = match args_vec(args) {
                Some(a) => a,
                None => output::crash(),
            };
            for (i, a) in av.iter().enumerate().take(arity) {
                self.visit(a);
                let v = self.rvalue.clone();
                let at = nobj.argument_type(mid, i);
                if at.ty == AsType::ObjectRef {
                    self.err(IErr::Internal, node, ln::OBJCALL_ARG);
                }
                cargs[i] = self.cpp_param(&at, &v, node, ln::OBJCALL_ARG, &mut owned);
            }
        }
        let ret = match nobj.call_method(mid, &mut cargs) {
            Ok(r) => r,
            Err(m) => self.err(IErr::Native(m), node, 0),
        };
        let rt = nobj.return_type(mid);
        let v = self.param_value(&rt, ret, node, ln::OBJCALL_RET);
        self.rvalue = v;
        for r in &owned {
            sdelete(r);
        }
    }

    fn visit_varref(&mut self, node: &Node, var_id: i32, global: bool, ty: TypeInfo) {
        let slot = self.slot(global, var_id);
        if self.obj_assign {
            let r = match ty.ty {
                AsType::Array => ObjRef::ArrayVar(slot),
                AsType::Dict => ObjRef::DictVar(slot),
                AsType::ObjectRef => ObjRef::NativeVar(slot),
                AsType::Matrix => ObjRef::MatrixVar(slot),
                AsType::String => ObjRef::StringVar(slot),
                _ => self.err(IErr::Internal, node, ln::VARREF_OBJASSIGN),
            };
            self.rvalue = Value::Ref(Rc::new(r));
        } else {
            let cur = self.stack[slot].clone();
            let mut t = ty.ty;
            if t == AsType::Var {
                t = cur.ty();
            }
            // QUIRK: the annotated type can come from the wrong symbol table
            // (global variable ids looked up in a function table), so the
            // slot may hold another type; the union is then reinterpreted.
            let bits = value_bits(&cur);
            self.rvalue = match t {
                AsType::Array => match cur {
                    Value::Array(a) => Value::Array(arr_get_ref(&a)),
                    _ => output::crash(),
                },
                AsType::Dict => match cur {
                    Value::Dict(d) => Value::Dict(dict_get_ref(&d)),
                    _ => output::crash(),
                },
                AsType::Matrix => match cur {
                    Value::Matrix(m) => Value::Matrix(mat_get_ref(&m)),
                    _ => output::crash(),
                },
                AsType::String => match cur {
                    Value::Str(s) => Value::Str(snew(sread(&s))),
                    _ => output::crash(),
                },
                AsType::ObjectRef => match cur {
                    v @ Value::NObj(..) => v,
                    _ => output::crash(),
                },
                AsType::Bool => Value::Bool(bits & 0xff != 0),
                AsType::Char => Value::Char(bits as u8 as i8),
                AsType::Int => Value::Int(bits as u32 as i32),
                AsType::Float => Value::Float(f64::from_bits(bits)),
                _ => self.err(IErr::Internal, node, ln::VARREF_INTERNAL),
            };
        }
    }

    fn visit_unpack(
        &mut self,
        node: &Node,
        nodes: &[UnpackNode],
        last_wild: bool,
        last_named: bool,
        expr: &NodeRef,
    ) {
        self.visit(expr);
        let arr = match &self.rvalue {
            Value::Array(a) => a.clone(),
            // QUIRK: the union is read as an array pointer whatever the type.
            _ => output::crash(),
        };

        let mut unpack_size = nodes.len() as i32;
        if last_named {
            unpack_size -= 1;
        }
        let asize = arr.borrow().st().len() as i32;
        if unpack_size > asize {
            self.err(IErr::UnpackValueTooSmall, node, ln::UNPACK_SMALL);
        }
        if unpack_size < asize && !last_wild {
            self.err(IErr::UnpackValueTooLarge, node, ln::UNPACK_LARGE);
        }

        for i in 0..unpack_size as usize {
            let un = &nodes[i];
            let var_type = self
                .sym(un.global.get())
                .borrow()
                .variable_type(un.var_id.get());
            let var_idx = (if un.global.get() { 0 } else { self.sp }) + un.var_id.get() as usize;
            let val = arr.borrow().st()[i].clone();
            self.assign_converted(var_idx, &var_type, &val, node, ln::UNPACK_DEFAULT);
        }

        if last_named {
            let un = &nodes[unpack_size as usize];
            let var_idx = (if un.global.get() { 0 } else { self.sp }) + un.var_id.get() as usize;
            let sz = asize - unpack_size;
            let wild = LocalArray::sized(sz as usize);
            for i in 0..sz as usize {
                let val = arr.borrow().st()[i + unpack_size as usize].clone();
                arr_set(&wild, i, &val);
            }
            if let Value::Array(a) = self.stack[var_idx].clone() {
                arr_remove_ref(&a);
            }
            self.stack[var_idx] = Value::Array(wild);
        }

        arr_remove_ref(&arr);
    }

    // ---- object references -------------------------------------------------------------

    fn slot_array(&self, s: usize) -> ArrRef {
        match &self.stack[s] {
            Value::Array(a) => a.clone(),
            _ => output::crash(),
        }
    }
    fn slot_dict(&self, s: usize) -> DictRef {
        match &self.stack[s] {
            Value::Dict(a) => a.clone(),
            _ => output::crash(),
        }
    }
    fn slot_matrix(&self, s: usize) -> MatRef {
        match &self.stack[s] {
            Value::Matrix(a) => a.clone(),
            _ => output::crash(),
        }
    }

    /// `cObjectRef::Get(val)`.
    fn ref_get_val(&mut self, r: &Rc<ObjRef>) -> Option<Value> {
        match &**r {
            ObjRef::ArrayVar(s) => Some(Value::Array(self.slot_array(*s))),
            ObjRef::DictVar(s) => Some(Value::Dict(self.slot_dict(*s))),
            ObjRef::MatrixVar(s) => Some(Value::Matrix(self.slot_matrix(*s))),
            ObjRef::NativeVar(_) => None,
            ObjRef::StringVar(s) => match &self.stack[*s] {
                Value::Str(x) => Some(Value::Str(x.clone())),
                _ => output::crash(),
            },
            ObjRef::Index(obj, idx) => self.ref_get_idx(obj, idx),
        }
    }

    /// `cObjectRef::Get(idx, val)`.
    fn ref_get_idx(&mut self, r: &Rc<ObjRef>, idx: &Value) -> Option<Value> {
        match &**r {
            ObjRef::ArrayVar(s) => {
                let arr = self.slot_array(*s);
                let i = idx_to_int(idx);
                if i < 0 || i as usize >= arr.borrow().st().len() {
                    return None;
                }
                let v = arr.borrow().st()[i as usize].clone();
                Some(v)
            }
            ObjRef::DictVar(s) => {
                let d = self.slot_dict(*s);
                dict_get(&d, idx)
            }
            ObjRef::MatrixVar(s) => {
                let m = self.slot_matrix(*s);
                let i = idx_to_int(idx);
                if i < 0 || i >= mat_num_rows(&m) {
                    return None;
                }
                let row = m.borrow().rw()[i as usize].clone();
                Some(Value::Array(row))
            }
            ObjRef::NativeVar(_) => None,
            ObjRef::StringVar(s) => {
                let i = idx_to_int(idx);
                match &self.stack[*s] {
                    Value::Str(x) => {
                        let b = sread(x);
                        if i < 0 || i as usize >= b.len() {
                            return None;
                        }
                        Some(Value::Char(b[i as usize] as i8))
                    }
                    _ => output::crash(),
                }
            }
            ObjRef::Index(obj, midx) => {
                let o_val = self.ref_get_idx(obj, midx)?;
                match o_val {
                    Value::Array(arr) => {
                        let i = idx_to_int(idx);
                        if i < 0 || i as usize >= arr.borrow().st().len() {
                            return None;
                        }
                        let v = arr.borrow().st()[i as usize].clone();
                        Some(v)
                    }
                    Value::Dict(d) => dict_get(&d, idx),
                    Value::Matrix(m) => {
                        let i = idx_to_int(idx);
                        if i < 0 || i >= mat_num_rows(&m) {
                            return None;
                        }
                        let row = m.borrow().rw()[i as usize].clone();
                        Some(Value::Array(row))
                    }
                    _ => None,
                }
            }
        }
    }

    /// `cObjectRef::Set(val)`.
    fn ref_set_val(&mut self, r: &Rc<ObjRef>, val: &Value) -> bool {
        match &**r {
            ObjRef::Index(obj, idx) => self.ref_set_idx(obj, &idx.clone(), val),
            _ => false,
        }
    }

    /// Release an index value that is not used as a position
    /// (the `switch (idx.type.type)` clean-up arms).
    fn idx_for_set(&self, idx: &Value) -> i32 {
        let i = idx_to_int(idx);
        if matches!(
            idx,
            Value::Array(_) | Value::Dict(_) | Value::Matrix(_) | Value::Str(_)
        ) {
            cleanup(idx);
        }
        i
    }

    /// `cObjectRef::Set(idx, val)`.
    fn ref_set_idx(&mut self, r: &Rc<ObjRef>, idx: &Value, val: &Value) -> bool {
        match &**r {
            ObjRef::ArrayVar(s) => {
                let arr = self.slot_array(*s);
                let i = self.idx_for_set(idx);
                if i < 0 || i as usize >= arr.borrow().st().len() {
                    return false;
                }
                arr_set(&arr, i as usize, val);
                true
            }
            ObjRef::DictVar(s) => {
                let d = self.slot_dict(*s);
                dict_set(&d, idx, val);
                true
            }
            ObjRef::MatrixVar(s) => {
                let m = self.slot_matrix(*s);
                let i = self.idx_for_set(idx);
                self.matrix_row_set(&m, i, val)
            }
            ObjRef::NativeVar(_) => false,
            ObjRef::StringVar(s) => {
                let i = self.idx_for_set(idx);
                let len = match &self.stack[*s] {
                    Value::Str(x) => sread(x).len(),
                    _ => output::crash(),
                };
                if i < 0 || i as usize >= len {
                    return false;
                }
                let c = match val {
                    Value::Bool(b) => *b as u8,
                    Value::Char(c) => *c as u8,
                    Value::Int(n) => *n as i8 as u8,
                    _ => return false,
                };
                if let Value::Str(x) = &self.stack[*s] {
                    x.bytes.borrow_mut()[i as usize] = c;
                }
                true
            }
            ObjRef::Index(obj, midx) => {
                let o_val = match self.ref_get_idx(obj, midx) {
                    Some(v) => v,
                    None => {
                        cleanup(idx);
                        return false;
                    }
                };
                match o_val {
                    Value::Array(arr) => {
                        let i = self.idx_for_set(idx);
                        if i < 0 || i as usize >= arr.borrow().st().len() {
                            return false;
                        }
                        arr_set(&arr, i as usize, val);
                        true
                    }
                    Value::Dict(d) => {
                        dict_set(&d, idx, val);
                        true
                    }
                    Value::Matrix(m) => {
                        let i = self.idx_for_set(idx);
                        self.matrix_row_set(&m, i, val)
                    }
                    _ => {
                        cleanup(idx);
                        false
                    }
                }
            }
        }
    }

    fn matrix_row_set(&mut self, m: &MatRef, i: i32, val: &Value) -> bool {
        if i < 0 || i >= mat_num_rows(m) {
            return false;
        }
        if m.borrow().sz_y == 0 {
            return false;
        }
        let a = match val {
            Value::Array(a) => a.clone(),
            _ => return false,
        };
        if a.borrow().st().len() as i32 != m.borrow().sz_y {
            return false;
        }
        mat_set_row(m, i as usize, &a);
        true
    }

    // ---- matrix multiplication ----------------------------------------------------------

    fn matrix_multiply(&mut self, m1: &MatRef, m2: &MatRef, node: &Node) -> Value {
        // Validate all values in matrix operands and determine operation type
        let mut op_type = AsType::Int;
        let mul = map_token(Tok::OpMul).to_string();
        for (m, line) in [(m1, ln::MATMUL_CHECK1), (m2, ln::MATMUL_CHECK2)] {
            let nr = mat_num_rows(m);
            let nc = m.borrow().sz_y;
            for i in 0..nr as usize {
                let row = m.borrow().rw()[i].clone();
                for j in 0..nc as usize {
                    let e = row_get(&row, j);
                    match e.ty() {
                        AsType::Bool | AsType::Char | AsType::Int | AsType::String => {}
                        AsType::Float => op_type = AsType::Float,
                        _ => self.err(
                            IErr::MatrixOpTypeMismatch(mul.clone(), map_type(&val_info(&e))),
                            node,
                            line,
                        ),
                    }
                }
            }
        }

        let (r1, c1) = (mat_num_rows(m1), m1.borrow().sz_y);
        let (r2, c2) = (mat_num_rows(m2), m2.borrow().sz_y);
        if r1 == 1 && c1 == 1 {
            self.scalar_multiply(m1, m2, op_type == AsType::Int, node)
        } else if r2 == 1 && c2 == 1 {
            self.scalar_multiply(m2, m1, op_type == AsType::Int, node)
        } else {
            if c1 != r2 {
                self.err(IErr::MatrixSizeMismatch(mul), node, ln::MATMUL_SIZE);
            }
            self.full_multiply(m1, m2, op_type == AsType::Int, node)
        }
    }

    fn elem_i(&self, e: &Value, node: &Node, line: u32) -> i32 {
        match e {
            Value::Bool(b) => *b as i32,
            Value::Char(c) => *c as i32,
            Value::Int(i) => *i,
            Value::Str(s) => as_int(&String::from_utf8_lossy(&sread(s))),
            _ => self.err(
                IErr::MatrixOpTypeMismatch(map_token(Tok::OpMul).into(), map_type(&val_info(e))),
                node,
                line,
            ),
        }
    }

    fn elem_f(&self, e: &Value, node: &Node, line: u32) -> f64 {
        match e {
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Char(c) => *c as f64,
            Value::Int(i) => *i as f64,
            Value::Float(f) => *f,
            Value::Str(s) => as_double(&String::from_utf8_lossy(&sread(s))),
            _ => self.err(
                IErr::MatrixOpTypeMismatch(map_token(Tok::OpMul).into(), map_type(&val_info(e))),
                node,
                line,
            ),
        }
    }

    fn scalar_multiply(&mut self, s: &MatRef, m: &MatRef, int_op: bool, node: &Node) -> Value {
        let nr = mat_num_rows(m) as usize;
        let nc = m.borrow().sz_y as usize;
        let srow = s.borrow().rw()[0].clone();
        let sval = row_get(&srow, 0);
        let mat = new_matrix();
        if int_op {
            let scalar = self.elem_i(&sval, node, ln::SCALAR_INT_S);
            let mut op = vec![vec![0i32; nc]; nr];
            for (i, orow) in op.iter_mut().enumerate() {
                let row = m.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_i(&row_get(&row, j), node, ln::SCALAR_INT_M);
                }
            }
            mat_resize(&mat, nr as i32, nc as i32);
            for (i, orow) in op.iter().enumerate() {
                let row = mat.borrow().rw()[i].clone();
                for (j, o) in orow.iter().enumerate() {
                    arr_set(&row, j, &Value::Int(scalar.wrapping_mul(*o)));
                }
            }
        } else {
            let scalar = self.elem_f(&sval, node, ln::SCALAR_FLOAT_S);
            let mut op = vec![vec![0f64; nc]; nr];
            for (i, orow) in op.iter_mut().enumerate() {
                let row = m.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_f(&row_get(&row, j), node, ln::SCALAR_FLOAT_M);
                }
            }
            mat_resize(&mat, nr as i32, nc as i32);
            for (i, orow) in op.iter().enumerate() {
                let row = mat.borrow().rw()[i].clone();
                for (j, o) in orow.iter().enumerate() {
                    arr_set(&row, j, &Value::Float(scalar * *o));
                }
            }
        }
        Value::Matrix(mat)
    }

    fn full_multiply(&mut self, m1: &MatRef, m2: &MatRef, int_op: bool, node: &Node) -> Value {
        let (r1, c1) = (mat_num_rows(m1) as usize, m1.borrow().sz_y as usize);
        let (r2, c2) = (mat_num_rows(m2) as usize, m2.borrow().sz_y as usize);
        let mat = new_matrix();
        if int_op {
            let mut op1 = vec![vec![0i32; c1]; r1];
            let mut op2 = vec![vec![0i32; c2]; r2];
            for (i, orow) in op1.iter_mut().enumerate() {
                let row = m1.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_i(&row_get(&row, j), node, ln::FULL_INT_1);
                }
            }
            for (i, orow) in op2.iter_mut().enumerate() {
                let row = m2.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_i(&row_get(&row, j), node, ln::FULL_INT_2);
                }
            }
            mat_resize(&mat, r1 as i32, c2 as i32);
            for i in 0..r1 {
                let row = mat.borrow().rw()[i].clone();
                for j in 0..c2 {
                    let mut sum = 0i32;
                    for k in 0..c1 {
                        sum = sum.wrapping_add(op1[i][k].wrapping_mul(op2[k][j]));
                    }
                    arr_set(&row, j, &Value::Int(sum));
                }
            }
        } else {
            let mut op1 = vec![vec![0f64; c1]; r1];
            let mut op2 = vec![vec![0f64; c2]; r2];
            for (i, orow) in op1.iter_mut().enumerate() {
                let row = m1.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_f(&row_get(&row, j), node, ln::FULL_FLOAT_1);
                }
            }
            for (i, orow) in op2.iter_mut().enumerate() {
                let row = m2.borrow().rw()[i].clone();
                for (j, o) in orow.iter_mut().enumerate() {
                    *o = self.elem_f(&row_get(&row, j), node, ln::FULL_FLOAT_2);
                }
            }
            mat_resize(&mat, r1 as i32, c2 as i32);
            for i in 0..r1 {
                let row = mat.borrow().rw()[i].clone();
                for j in 0..c2 {
                    let mut sum = 0f64;
                    for k in 0..c1 {
                        // `sum.as_float += op1 * op2` is one expression: fused.
                        sum = op1[i][k].mul_add(op2[k][j], sum);
                    }
                    arr_set(&row, j, &Value::Float(sum));
                }
            }
        }
        Value::Matrix(mat)
    }
}

/// The bytes of the C++ union for a stored value (unwritten bytes read as 0).
fn value_bits(v: &Value) -> u64 {
    match v {
        Value::Bool(b) => *b as u64,
        Value::Char(c) => *c as u8 as u64,
        Value::Int(i) => *i as u32 as u64,
        Value::Float(f) => f.to_bits(),
        Value::Invalid | Value::Null | Value::Void => 0,
        // Pointers: the address is not reproducible.
        _ => 0x6000_0000_1000,
    }
}

fn row_get(row: &ArrRef, j: usize) -> Value {
    row.borrow().st().get(j).cloned().unwrap_or(Value::Invalid)
}

fn is_plain(t: &TypeInfo, a: AsType) -> bool {
    t.ty == a && t.info.is_empty()
}

#[allow(dead_code)]
fn _cname(t: AsType) -> String {
    cname(t)
}
