//! Built-in function library (`cASFunction.h`, `cASLibrary`, `ASCoreLib`,
//! and the registration surface of `ASAvidaLib` / `ASAnalyzeLib`).

use std::rc::Rc;

use crate::ast::Bytes;
use crate::native::CppParam;
use crate::oldmap::OldMap;
use crate::output;
use crate::types::{AsType, TypeInfo};

/// `cASFunction`.
pub trait ASFunction {
    fn name(&self) -> &[u8];
    fn arity(&self) -> usize;
    fn return_type(&self) -> &TypeInfo;
    fn argument_type(&self, arg: usize) -> &TypeInfo;
    fn call(&self, args: &mut [CppParam]) -> Result<CppParam, String>;
}

type CallFn = Box<dyn Fn(&mut [CppParam]) -> Result<CppParam, String>>;

/// `tASFunction<R (Args...)>` as a closure with an explicit signature.
pub struct FnEntry {
    name: Bytes,
    rtype: TypeInfo,
    args: Vec<TypeInfo>,
    f: CallFn,
}

impl FnEntry {
    pub fn new(name: &str, rtype: TypeInfo, args: Vec<TypeInfo>, f: CallFn) -> Self {
        FnEntry {
            name: name.as_bytes().to_vec(),
            rtype,
            args,
            f,
        }
    }
}

impl ASFunction for FnEntry {
    fn name(&self) -> &[u8] {
        &self.name
    }
    fn arity(&self) -> usize {
        self.args.len()
    }
    fn return_type(&self) -> &TypeInfo {
        &self.rtype
    }
    fn argument_type(&self, arg: usize) -> &TypeInfo {
        &self.args[arg]
    }
    fn call(&self, args: &mut [CppParam]) -> Result<CppParam, String> {
        (self.f)(args)
    }
}

/// `cASLibrary`.
pub struct ASLibrary {
    fun_dict: OldMap<Bytes, Rc<dyn ASFunction>>,
}

impl Default for ASLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl ASLibrary {
    pub fn new() -> Self {
        ASLibrary {
            fun_dict: OldMap::new(),
        }
    }

    pub fn lookup_function(&self, name: &[u8]) -> Option<Rc<dyn ASFunction>> {
        self.fun_dict.find(&name.to_vec()).cloned()
    }

    pub fn has_function(&self, name: &[u8]) -> bool {
        self.fun_dict.has(&name.to_vec())
    }

    /// `RegisterFunction`: false when the name is already taken.
    pub fn register_function(&mut self, func: Box<dyn ASFunction>) -> bool {
        let name = func.name().to_vec();
        if self.fun_dict.has(&name) {
            false
        } else {
            self.fun_dict.add(name, Rc::from(func));
            true
        }
    }
}

/// `std::cout << cString` goes through `const char*`: output stops at the
/// first NUL byte.
fn str_arg(args: &[CppParam]) -> Bytes {
    match args.first() {
        Some(CppParam::Str(s)) => {
            let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
            s[..end].to_vec()
        }
        _ => Vec::new(),
    }
}

/// `RegisterASCoreLib`: `print` and `println`.
pub fn register_core_lib(lib: &mut ASLibrary) {
    lib.register_function(Box::new(FnEntry::new(
        "print",
        TypeInfo::new(AsType::Void),
        vec![TypeInfo::new(AsType::String)],
        Box::new(|a| {
            output::write(&str_arg(a));
            Ok(CppParam::None)
        }),
    )));
    lib.register_function(Box::new(FnEntry::new(
        "println",
        TypeInfo::new(AsType::Void),
        vec![TypeInfo::new(AsType::String)],
        Box::new(|a| {
            output::write(&str_arg(a));
            output::endl();
            Ok(CppParam::None)
        }),
    )));
}

/// Signature-only stand-ins for the functions `RegisterASAvidaLib` and
/// `RegisterASAnalyzeLib` add (`Config()`, `Driver(World@)`, `World(Config@)`,
/// `LoadResourceHistory(string)`). They make the semantic checks behave as in
/// the C++ (names reserved, signatures checked) but cannot run: calling one
/// ends the script with an internal error.
///
/// HOOK: the world crate replaces these with real constructors by
/// registering functions of the same names in a library built without this
/// call.
pub fn register_native_stubs(lib: &mut ASLibrary) {
    fn stub(name: &str, ret: &str, args: Vec<TypeInfo>) -> Box<dyn ASFunction> {
        let n = name.to_string();
        Box::new(FnEntry::new(
            name,
            TypeInfo::with_info(AsType::ObjectRef, ret),
            args,
            Box::new(move |_| Err(format!("native object support for '{n}' is not available"))),
        ))
    }
    lib.register_function(stub("Config", "Config", vec![]));
    lib.register_function(stub(
        "Driver",
        "Driver",
        vec![TypeInfo::with_info(AsType::ObjectRef, "World")],
    ));
    lib.register_function(stub(
        "World",
        "World",
        vec![TypeInfo::with_info(AsType::ObjectRef, "Config")],
    ));
    lib.register_function(stub(
        "LoadResourceHistory",
        "ResourceHistory",
        vec![TypeInfo::new(AsType::String)],
    ));
}
