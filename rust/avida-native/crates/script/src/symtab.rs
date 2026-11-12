//! `cSymbolTable`: variables and functions of one scope level (a function
//! body or the global scope) with nested block scopes.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ast::{Bytes, NodeRef};
use crate::oldmap::{near_match, OldMap};
use crate::types::TypeInfo;

pub type SymRef = Rc<RefCell<SymbolTable>>;

struct SymEntry {
    name: Bytes,
    ty: TypeInfo,
    scope: i32,
    shadow: i32,
    deactivate: i32,
}

struct FunEntry {
    name: Bytes,
    ty: TypeInfo,
    signature: Option<NodeRef>,
    symtbl: Option<SymRef>,
    code: Option<NodeRef>,
    scope: i32,
    shadow: i32,
    deactivate: i32,
}

pub struct SymbolTable {
    sym_tbl: Vec<SymEntry>,
    sym_dict: OldMap<Bytes, i32>,
    fun_tbl: Vec<FunEntry>,
    fun_dict: OldMap<Bytes, i32>,
    scope: i32,
    deactivate_cycle: i32,
    ret: bool,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            sym_tbl: Vec::new(),
            sym_dict: OldMap::new(),
            fun_tbl: Vec::new(),
            fun_dict: OldMap::new(),
            scope: 0,
            deactivate_cycle: 0,
            ret: false,
        }
    }

    pub fn new_ref() -> SymRef {
        Rc::new(RefCell::new(Self::new()))
    }

    /// `AddVariable`: `Ok(id)` on success, `Err(())` for a redefinition in the
    /// same scope.
    pub fn add_variable(&mut self, name: &[u8], ty: &TypeInfo) -> Result<i32, ()> {
        let key = name.to_vec();
        let found = self.sym_dict.find(&key).copied();
        if let Some(id) = found {
            let e = &self.sym_tbl[id as usize];
            if e.scope == self.scope && e.deactivate == 0 {
                return Err(());
            }
        }
        let var_id = self.sym_tbl.len() as i32;
        self.sym_tbl.push(SymEntry {
            name: key.clone(),
            ty: ty.clone(),
            scope: self.scope,
            shadow: -1,
            deactivate: 0,
        });
        if let Some(shadow) = found {
            self.sym_dict.set_value(key, var_id);
            self.sym_tbl[var_id as usize].shadow = shadow;
        } else {
            self.sym_dict.add(key, var_id);
        }
        Ok(var_id)
    }

    /// `AddFunction`. QUIRK: the C++ reads `shadow` from the incoming
    /// `fun_id` (always -1 at both call sites) before the lookup, so a
    /// shadowed function is removed outright when its scope is popped.
    pub fn add_function(&mut self, name: &[u8], ty: &TypeInfo) -> Result<i32, i32> {
        let shadow = -1;
        let key = name.to_vec();
        let found = self.fun_dict.find(&key).copied();
        if let Some(id) = found {
            let e = &self.fun_tbl[id as usize];
            if e.scope == self.scope && e.deactivate == 0 {
                return Err(id);
            }
        }
        let fun_id = self.fun_tbl.len() as i32;
        self.fun_tbl.push(FunEntry {
            name: key.clone(),
            ty: ty.clone(),
            signature: None,
            symtbl: None,
            code: None,
            scope: self.scope,
            shadow: -1,
            deactivate: 0,
        });
        if found.is_some() {
            self.fun_dict.set_value(key, fun_id);
            self.fun_tbl[fun_id as usize].shadow = shadow;
        } else {
            self.fun_dict.add(key, fun_id);
        }
        Ok(fun_id)
    }

    pub fn lookup_variable(&self, name: &[u8]) -> Option<i32> {
        self.sym_dict.find(&name.to_vec()).copied()
    }

    pub fn lookup_function(&self, name: &[u8]) -> Option<i32> {
        self.fun_dict.find(&name.to_vec()).copied()
    }

    pub fn num_variables(&self) -> i32 {
        self.sym_tbl.len() as i32
    }

    pub fn num_functions(&self) -> i32 {
        self.fun_tbl.len() as i32
    }

    pub fn variable_near_match(&self, name: &[u8]) -> Bytes {
        near_match(&self.sym_dict, name)
    }

    pub fn function_near_match(&self, name: &[u8]) -> Bytes {
        near_match(&self.fun_dict, name)
    }

    pub fn push_scope(&mut self) {
        self.scope += 1;
    }

    pub fn pop_scope(&mut self) {
        self.deactivate_cycle += 1;
        let cycle = self.deactivate_cycle;
        let scope = self.scope;

        for i in 0..self.sym_tbl.len() {
            let (name, shadow, hit) = {
                let se = &self.sym_tbl[i];
                (
                    se.name.clone(),
                    se.shadow,
                    se.scope == scope && se.deactivate == 0,
                )
            };
            if hit {
                if shadow == -1 {
                    self.sym_dict.remove(&name);
                } else {
                    self.sym_dict.set_value(name, shadow);
                }
                self.sym_tbl[i].deactivate = cycle;
            }
        }
        for i in 0..self.fun_tbl.len() {
            let (name, shadow, hit) = {
                let fe = &self.fun_tbl[i];
                (
                    fe.name.clone(),
                    fe.shadow,
                    fe.scope == scope && fe.deactivate == 0,
                )
            };
            if hit {
                if shadow == -1 {
                    self.fun_dict.remove(&name);
                } else {
                    self.fun_dict.set_value(name, shadow);
                }
                self.fun_tbl[i].deactivate = cycle;
            }
        }

        self.ret = false;
        self.scope -= 1;
    }

    pub fn set_scope_return(&mut self) {
        self.ret = true;
    }

    pub fn scope_has_return(&self) -> bool {
        self.ret
    }

    pub fn variable_type(&self, id: i32) -> TypeInfo {
        self.sym_tbl[id as usize].ty.clone()
    }

    pub fn function_name(&self, id: i32) -> Bytes {
        self.fun_tbl[id as usize].name.clone()
    }

    pub fn function_rtype(&self, id: i32) -> TypeInfo {
        self.fun_tbl[id as usize].ty.clone()
    }

    pub fn function_symtbl(&self, id: i32) -> Option<SymRef> {
        self.fun_tbl[id as usize].symtbl.clone()
    }

    pub fn function_signature(&self, id: i32) -> Option<NodeRef> {
        self.fun_tbl[id as usize].signature.clone()
    }

    pub fn function_definition(&self, id: i32) -> Option<NodeRef> {
        self.fun_tbl[id as usize].code.clone()
    }

    pub fn set_function_symtbl(&mut self, id: i32, s: SymRef) {
        self.fun_tbl[id as usize].symtbl = Some(s);
    }

    pub fn set_function_signature(&mut self, id: i32, s: Option<NodeRef>) {
        self.fun_tbl[id as usize].signature = s;
    }

    pub fn set_function_definition(&mut self, id: i32, c: Option<NodeRef>) {
        self.fun_tbl[id as usize].code = c;
    }

    /// `ActiveFunctionIterator`: `(has_code, name)` of the active functions of
    /// the current scope level, newest first.
    pub fn active_functions(&self) -> Vec<(bool, Bytes)> {
        let mut v = Vec::new();
        for i in (0..self.fun_tbl.len()).rev() {
            let f = &self.fun_tbl[i];
            if f.scope == self.scope && f.deactivate == 0 {
                v.push((f.code.is_some(), f.name.clone()));
            }
        }
        v
    }
}
