//! Native object support (`cASNativeObject.h`, `cASNativeObjectMethod.h`,
//! `cASCPPParameter.h`).
//!
//! HOOK: the world crate implements [`NativeObject`] for the Avida objects
//! scripts can hold (`Config`, `Driver`, `Genotype`, `GenotypeBatch`,
//! `ResourceHistory`, `World`) and registers constructor functions through
//! [`crate::library::ASLibrary::register_function`].

use std::rc::Rc;

use crate::ast::Bytes;
use crate::types::TypeInfo;

/// `cASCPPParameter`: one argument or return value crossing into native code.
#[derive(Clone)]
pub enum CppParam {
    None,
    Bool(bool),
    Char(i8),
    Int(i32),
    Float(f64),
    Str(Bytes),
    NObj(Rc<dyn NativeObject>),
}

/// `cASNativeObject` + its method table (`tASNativeObject<T>`).
pub trait NativeObject {
    /// `GetType()`: the class name used for type checks (`Config`, ...).
    fn type_name(&self) -> &str;
    /// `LookupMethod`.
    fn lookup_method(&self, name: &[u8]) -> Option<usize>;
    fn arity(&self, mid: usize) -> usize;
    fn argument_type(&self, mid: usize, arg: usize) -> TypeInfo;
    fn return_type(&self, mid: usize) -> TypeInfo;
    /// `CallMethod`; an `Err` aborts the script with an internal error.
    fn call_method(&self, mid: usize, args: &mut [CppParam]) -> Result<CppParam, String>;
}
