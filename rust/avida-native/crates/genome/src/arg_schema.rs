//! Port of `cArgSchema` and `cArgContainer`
//! (avida-core/source/tools/cArgSchema.{h,cc}, cArgContainer.{h,cc}):
//! named `name=value` arguments, separated by a configurable entry
//! separator, loaded into typed slots.
//!
//! This is the complete generic port (required and optional entries, int
//! and double ranges, case sensitivity, separators, every message) and is
//! meant to become the shared implementation for the tasks and actions code.

use crate::cstr::{apto_trim, as_double, as_int, pop, to_lower, trim};
use crate::feedback::Feedback;

/// `cArgSchema::tType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    /// `SCHEMA_INT`
    Int,
    /// `SCHEMA_DOUBLE`
    Double,
    /// `SCHEMA_STRING`
    String,
}

/// `cArgSchema::sArgSchemaEntry`.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    optional: bool,
    def_int: i32,
    def_double: f64,
    def_string: String,
    has_range_limits: bool,
    r_l_int: i32,
    r_u_int: i32,
    r_l_double: f64,
    r_u_double: f64,
}

impl Entry {
    fn new(name: &str, optional: bool) -> Self {
        Entry {
            name: name.to_string(),
            optional,
            def_int: 0,
            def_double: 0.0,
            def_string: String::new(),
            has_range_limits: false,
            r_l_int: 0,
            r_u_int: 0,
            r_l_double: 0.0,
            r_u_double: 0.0,
        }
    }
}

/// `cArgSchema`.
///
/// Entries are registered under a name (adjusted: Apto-trimmed and, unless
/// case sensitive, lower-cased) with a type and a slot index. As in C++, a
/// later entry with the same type and index replaces the earlier one in its
/// slot (defaults, ranges and the name used in messages come from the slot),
/// while name lookup keeps every registered name.
#[derive(Debug, Clone)]
pub struct ArgSchema {
    /// `m_entries`: adjusted name -> (type, index), in registration order.
    entries: Vec<(String, ArgType, usize)>,
    ints: Vec<Option<Entry>>,
    doubles: Vec<Option<Entry>>,
    strings: Vec<Option<Entry>>,
    sep_entry: char,
    sep_value: char,
    case_sensitive: bool,
}

impl Default for ArgSchema {
    /// `cArgSchema(',', '=', false)`.
    fn default() -> Self {
        ArgSchema::new(',', '=', false)
    }
}

fn put(slots: &mut Vec<Option<Entry>>, idx: usize, entry: Entry) {
    if slots.len() <= idx {
        slots.resize(idx + 1, None);
    }
    slots[idx] = Some(entry);
}

impl ArgSchema {
    /// `cArgSchema(char entry = ',', char value = '=', bool case_sensitive = false)`.
    pub fn new(entry_sep: char, value_sep: char, case_sensitive: bool) -> Self {
        ArgSchema {
            entries: Vec::new(),
            ints: Vec::new(),
            doubles: Vec::new(),
            strings: Vec::new(),
            sep_entry: entry_sep,
            sep_value: value_sep,
            case_sensitive,
        }
    }

    /// `cArgSchema(entry)` with the default value separator and case
    /// insensitivity (e.g. `cArgSchema schema(':')`).
    pub fn with_entry_separator(entry_sep: char) -> Self {
        ArgSchema::new(entry_sep, '=', false)
    }

    /// `AdjustArgName`: `Apto::String::Trim`, then `ToLower` unless case
    /// sensitive.
    pub fn adjust_arg_name(&self, name: &str) -> String {
        let n = apto_trim(name);
        if self.case_sensitive {
            n
        } else {
            to_lower(&n)
        }
    }

    /// `GetEntrySeparator()`.
    pub fn entry_separator(&self) -> char {
        self.sep_entry
    }

    /// `GetValueSeparator()`.
    pub fn value_separator(&self) -> char {
        self.sep_value
    }

    /// `IsCaseSensitive()`.
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    /// Shared prologue of every `AddEntry`: adjust the name and refuse
    /// duplicates (and negative slots, which are undefined in C++).
    fn register(&mut self, name: &str, idx: i32, ty: ArgType) -> Option<(String, usize)> {
        let name = self.adjust_arg_name(name);
        if idx < 0 || self.entries.iter().any(|(n, _, _)| *n == name) {
            return None;
        }
        self.entries.push((name.clone(), ty, idx as usize));
        Some((name, idx as usize))
    }

    /// `AddEntry(name, idx, tType)`: required argument of the given type.
    pub fn add_required(&mut self, name: &str, idx: i32, ty: ArgType) -> bool {
        let Some((name, idx)) = self.register(name, idx, ty) else {
            return false;
        };
        let entry = Entry::new(&name, false);
        match ty {
            ArgType::Int => put(&mut self.ints, idx, entry),
            ArgType::Double => put(&mut self.doubles, idx, entry),
            ArgType::String => put(&mut self.strings, idx, entry),
        }
        true
    }

    /// `AddEntry(name, idx, int def)`: optional int.
    pub fn add_int(&mut self, name: &str, idx: i32, def: i32) -> bool {
        let Some((name, idx)) = self.register(name, idx, ArgType::Int) else {
            return false;
        };
        let mut entry = Entry::new(&name, true);
        entry.def_int = def;
        put(&mut self.ints, idx, entry);
        true
    }

    /// `AddEntry(name, idx, int lower, int upper)`: required int with range.
    pub fn add_int_range(&mut self, name: &str, idx: i32, lower: i32, upper: i32) -> bool {
        if self.add_required(name, idx, ArgType::Int) {
            let e = self.ints[idx as usize].as_mut().expect("just added");
            e.has_range_limits = true;
            e.r_l_int = lower;
            e.r_u_int = upper;
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, int lower, int upper, int def)`: optional int
    /// with range.
    pub fn add_int_range_default(
        &mut self,
        name: &str,
        idx: i32,
        lower: i32,
        upper: i32,
        def: i32,
    ) -> bool {
        if self.add_int(name, idx, def) {
            let e = self.ints[idx as usize].as_mut().expect("just added");
            e.has_range_limits = true;
            e.r_l_int = lower;
            e.r_u_int = upper;
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, double def)`: optional double.
    pub fn add_double(&mut self, name: &str, idx: i32, def: f64) -> bool {
        let Some((name, idx)) = self.register(name, idx, ArgType::Double) else {
            return false;
        };
        let mut entry = Entry::new(&name, true);
        entry.def_double = def;
        put(&mut self.doubles, idx, entry);
        true
    }

    /// QUIRK of both double-range overloads: they set the limits on
    /// `m_ints[in_idx]` (through the int/double unions) instead of on the
    /// double entry. The double therefore stays unlimited, and an int entry
    /// in the same slot gets range-limited to the low 32 bits of the
    /// doubles' bit patterns. (With no int entry in that slot the C++
    /// dereferences a null/out-of-range pointer; here nothing happens.)
    fn quirk_double_range_on_int_slot(&mut self, idx: i32, lower: f64, upper: f64) {
        if let Some(Some(e)) = self.ints.get_mut(idx as usize) {
            e.has_range_limits = true;
            e.r_l_int = lower.to_bits() as u32 as i32;
            e.r_u_int = upper.to_bits() as u32 as i32;
        }
    }

    /// `AddEntry(name, idx, double lower, double upper)`: required double
    /// "with range" (see the QUIRK on the range handling).
    pub fn add_double_range(&mut self, name: &str, idx: i32, lower: f64, upper: f64) -> bool {
        if self.add_required(name, idx, ArgType::Double) {
            self.quirk_double_range_on_int_slot(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, double lower, double upper, double def)`:
    /// optional double "with range" (see the QUIRK on the range handling).
    pub fn add_double_range_default(
        &mut self,
        name: &str,
        idx: i32,
        lower: f64,
        upper: f64,
        def: f64,
    ) -> bool {
        if self.add_double(name, idx, def) {
            self.quirk_double_range_on_int_slot(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, const Apto::String& def)`: optional string.
    pub fn add_string(&mut self, name: &str, idx: i32, def: &str) -> bool {
        let Some((name, idx)) = self.register(name, idx, ArgType::String) else {
            return false;
        };
        let mut entry = Entry::new(&name, true);
        entry.def_string = def.to_string();
        put(&mut self.strings, idx, entry);
        true
    }

    /// `FindEntry(name, type, idx)`: lookup by (already adjusted) name.
    pub fn find_entry(&self, name: &str) -> Option<(ArgType, i32)> {
        self.entries
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, t, i)| (*t, *i as i32))
    }

    /// `GetNumIntArgs()` (slot count, including unfilled slots).
    pub fn num_int_args(&self) -> i32 {
        self.ints.len() as i32
    }

    /// `GetNumDoubleArgs()`.
    pub fn num_double_args(&self) -> i32 {
        self.doubles.len() as i32
    }

    /// `GetNumStringArgs()`.
    pub fn num_string_args(&self) -> i32 {
        self.strings.len() as i32
    }

    fn slot(slots: &[Option<Entry>], i: i32) -> Option<&Entry> {
        if i < 0 {
            return None;
        }
        slots.get(i as usize).and_then(|e| e.as_ref())
    }

    /// `IsOptionalInt(i)`.
    pub fn is_optional_int(&self, i: i32) -> bool {
        Self::slot(&self.ints, i).is_some_and(|e| e.optional)
    }

    /// `IsOptionalDouble(i)`.
    pub fn is_optional_double(&self, i: i32) -> bool {
        Self::slot(&self.doubles, i).is_some_and(|e| e.optional)
    }

    /// `IsOptionalString(i)`.
    pub fn is_optional_string(&self, i: i32) -> bool {
        Self::slot(&self.strings, i).is_some_and(|e| e.optional)
    }

    /// `SetDefaultInt(i, v)`: the default when slot `i` is optional.
    pub fn default_int(&self, i: i32) -> Option<i32> {
        Self::slot(&self.ints, i)
            .filter(|e| e.optional)
            .map(|e| e.def_int)
    }

    /// `SetDefaultDouble(i, v)`.
    pub fn default_double(&self, i: i32) -> Option<f64> {
        Self::slot(&self.doubles, i)
            .filter(|e| e.optional)
            .map(|e| e.def_double)
    }

    /// `SetDefaultString(i, v)`.
    pub fn default_string(&self, i: i32) -> Option<&str> {
        Self::slot(&self.strings, i)
            .filter(|e| e.optional)
            .map(|e| e.def_string.as_str())
    }

    /// `GetIntName(i, name)`.
    pub fn int_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.ints, i).map(|e| e.name.as_str())
    }

    /// `GetDoubleName(i, name)`.
    pub fn double_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.doubles, i).map(|e| e.name.as_str())
    }

    /// `GetStringName(i, name)`.
    pub fn string_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.strings, i).map(|e| e.name.as_str())
    }

    /// `ValidateInt(i, v)`: slot exists and `v` is within its limits.
    pub fn validate_int(&self, i: i32, v: i32) -> bool {
        Self::slot(&self.ints, i)
            .is_some_and(|e| !e.has_range_limits || (v <= e.r_u_int && v >= e.r_l_int))
    }

    /// `ValidateDouble(i, v)`.
    pub fn validate_double(&self, i: i32, v: f64) -> bool {
        Self::slot(&self.doubles, i)
            .is_some_and(|e| !e.has_range_limits || (v <= e.r_u_double && v >= e.r_l_double))
    }
}

/// `cArgContainer`: the typed values loaded for one argument string.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArgContainer {
    ints: Vec<i32>,
    doubles: Vec<f64>,
    strings: Vec<String>,
}

impl ArgContainer {
    /// `cArgContainer::Load(args, schema, feedback)`.
    ///
    /// Returns `None` (after reporting to `feedback`) on an unrecognised
    /// argument or a missing required argument. QUIRK: an out-of-range value
    /// is reported as an error but does not make the load fail. QUIRK:
    /// parsing stops at the first empty entry (`a=1,,b=2` ignores `b`).
    /// Int values use `strtol(.., 0)`, doubles `strtod`, strings are trimmed.
    pub fn load(args: &str, schema: &ArgSchema, feedback: &mut Feedback) -> Option<ArgContainer> {
        let n_int = schema.num_int_args() as usize;
        let n_dbl = schema.num_double_args() as usize;
        let n_str = schema.num_string_args() as usize;
        let mut set_ints = vec![false; n_int];
        let mut set_doubles = vec![false; n_dbl];
        let mut set_strings = vec![false; n_str];
        let mut ret = ArgContainer {
            ints: vec![0; n_int],
            doubles: vec![0.0; n_dbl],
            strings: vec![String::new(); n_str],
        };

        let mut args = args.to_string();
        let mut success = true;

        let mut arg_ent = pop(&mut args, schema.entry_separator());
        while !arg_ent.is_empty() {
            let arg_name = schema.adjust_arg_name(&pop(&mut arg_ent, schema.value_separator()));

            match schema.find_entry(&arg_name) {
                Some((ArgType::Int, index)) => {
                    let i = index as usize;
                    set_ints[i] = true;
                    ret.ints[i] = as_int(&arg_ent);
                    if !schema.validate_int(index, ret.ints[i]) {
                        match schema.int_name(index) {
                            Some(name) => feedback
                                .error(format!("value of '{}' exceeds its defined range", name)),
                            None => feedback
                                .error(format!("invalid int schema entry at index {}", index)),
                        }
                    }
                }
                Some((ArgType::Double, index)) => {
                    let i = index as usize;
                    set_doubles[i] = true;
                    ret.doubles[i] = as_double(&arg_ent);
                    if !schema.validate_double(index, ret.doubles[i]) {
                        match schema.double_name(index) {
                            Some(name) => feedback
                                .error(format!("value of '{}' exceeds its defined range", name)),
                            None => feedback
                                .error(format!("invalid double schema entry at index {}", index)),
                        }
                    }
                }
                Some((ArgType::String, index)) => {
                    let i = index as usize;
                    set_strings[i] = true;
                    ret.strings[i] = trim(&arg_ent);
                }
                None => {
                    success = false;
                    feedback.error(format!("unrecognized argument: '{}'", arg_name));
                }
            }
            arg_ent = pop(&mut args, schema.entry_separator());
        }

        for (i, set) in set_ints.iter().enumerate() {
            if *set {
                continue;
            }
            let ii = i as i32;
            if let Some(def) = schema.default_int(ii) {
                ret.ints[i] = def;
            } else {
                success = false;
                match schema.int_name(ii) {
                    Some(name) => {
                        feedback.error(format!("required argument '{}' was not found", name))
                    }
                    None => feedback.error(format!("invalid int schema entry at index {}", ii)),
                }
            }
        }
        for (i, set) in set_doubles.iter().enumerate() {
            if *set {
                continue;
            }
            let ii = i as i32;
            if let Some(def) = schema.default_double(ii) {
                ret.doubles[i] = def;
            } else {
                success = false;
                match schema.double_name(ii) {
                    Some(name) => {
                        feedback.error(format!("required argument '{}' was not found", name))
                    }
                    None => feedback.error(format!("invalid double schema entry at index {}", ii)),
                }
            }
        }
        for (i, set) in set_strings.iter().enumerate() {
            if *set {
                continue;
            }
            let ii = i as i32;
            if let Some(def) = schema.default_string(ii) {
                ret.strings[i] = def.to_string();
            } else {
                success = false;
                match schema.string_name(ii) {
                    Some(name) => {
                        feedback.error(format!("required argument '{}' was not found", name))
                    }
                    None => feedback.error(format!("invalid string schema entry at index {}", ii)),
                }
            }
        }

        if success {
            Some(ret)
        } else {
            None
        }
    }

    /// `GetInt(i)`.
    pub fn get_int(&self, i: i32) -> i32 {
        self.ints[i as usize]
    }

    /// `GetDouble(i)`.
    pub fn get_double(&self, i: i32) -> f64 {
        self.doubles[i as usize]
    }

    /// `GetString(i)`.
    pub fn get_string(&self, i: i32) -> &str {
        &self.strings[i as usize]
    }

    /// `SetInt(i, v)` (grows the slot list).
    pub fn set_int(&mut self, i: i32, v: i32) {
        let i = i as usize;
        if self.ints.len() <= i {
            self.ints.resize(i + 1, 0);
        }
        self.ints[i] = v;
    }

    /// `SetDouble(i, v)`.
    pub fn set_double(&mut self, i: i32, v: f64) {
        let i = i as usize;
        if self.doubles.len() <= i {
            self.doubles.resize(i + 1, 0.0);
        }
        self.doubles[i] = v;
    }

    /// `SetString(i, v)`.
    pub fn set_string(&mut self, i: i32, v: &str) {
        let i = i as usize;
        if self.strings.len() <= i {
            self.strings.resize(i + 1, String::new());
        }
        self.strings[i] = v.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::ErrorMessages;

    fn inst_schema() -> ArgSchema {
        // cInstSet::LoadWithStringList
        let mut schema = ArgSchema::with_entry_separator(':');
        assert!(schema.add_int("cost", 0, 0));
        assert!(schema.add_int("initial_cost", 1, 0));
        assert!(schema.add_double("prob_fail", 0, 0.0));
        assert!(schema.add_double("redundancy", 1, 1.0));
        assert!(schema.add_string("inst_code", 0, ""));
        schema
    }

    #[test]
    fn defaults_and_values() {
        let schema = inst_schema();
        let mut fb = Feedback::new();
        let a = ArgContainer::load("", &schema, &mut fb).unwrap();
        assert_eq!(
            (a.get_int(0), a.get_int(1), a.get_double(1), a.get_string(0)),
            (0, 0, 1.0, "")
        );
        let a = ArgContainer::load(
            "redundancy=2.5: COST =0x10:inst_code= 0101 :prob_fail=1e-2",
            &schema,
            &mut fb,
        )
        .unwrap();
        assert_eq!(a.get_double(1), 2.5);
        assert_eq!(a.get_int(0), 16);
        assert_eq!(a.get_string(0), "0101");
        assert_eq!(a.get_double(0), 0.01);
        assert_eq!(fb.num_messages(), 0);
        // value without '=' reads as 0.
        let a = ArgContainer::load("cost", &schema, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 0);
    }

    #[test]
    fn errors() {
        let schema = inst_schema();
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("foo=1:Bar=2", &schema, &mut fb).is_none());
        assert_eq!(
            fb.error_messages(),
            vec![
                "unrecognized argument: 'foo'",
                "unrecognized argument: 'bar'"
            ]
        );

        // empty entry ends parsing
        let mut fb = Feedback::new();
        let a = ArgContainer::load("cost=3::foo=1", &schema, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 3);
        assert_eq!(fb.num_messages(), 0);

        let mut hw = ArgSchema::with_entry_separator(':');
        hw.add_required("hw_type", 0, ArgType::Int);
        hw.add_int("stack_size", 1, 10);
        hw.add_int("uops_per_cycle", 2, 20);
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("stack_size=3", &hw, &mut fb).is_none());
        assert_eq!(
            fb.error_messages(),
            vec!["required argument 'hw_type' was not found"]
        );
        let a = ArgContainer::load("hw_type=2", &hw, &mut fb).unwrap();
        assert_eq!((a.get_int(0), a.get_int(1), a.get_int(2)), (2, 10, 20));

        // holes in the slot list
        let mut holes = ArgSchema::default();
        holes.add_int("x", 1, 5);
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("x=1", &holes, &mut fb).is_none());
        assert_eq!(
            fb.error_messages(),
            vec!["invalid int schema entry at index 0"]
        );
    }

    #[test]
    fn ranges() {
        let mut s = ArgSchema::default();
        assert!(s.add_int_range_default("save_historic", 0, 0, 1, 1));
        assert!(s.add_int_range("initfacing", 1, 0, 7));
        assert!(!s.add_int("save_historic", 2, 0)); // duplicate name
        let mut fb = Feedback::new();
        let a = ArgContainer::load("initfacing=9, save_historic=0", &s, &mut fb).unwrap();
        // QUIRK: out of range is reported but the value is kept.
        assert_eq!((a.get_int(0), a.get_int(1)), (0, 9));
        assert_eq!(
            fb.error_messages(),
            vec!["value of 'initfacing' exceeds its defined range"]
        );
        assert!(s.validate_int(1, 7) && !s.validate_int(1, -1));

        // QUIRK: double ranges land on the int slot with the same index.
        let mut q = ArgSchema::default();
        q.add_int("n", 0, 3);
        q.add_double_range_default("p", 0, 0.0, 1.0, 0.5);
        assert!(q.validate_double(0, 7.0));
        assert!(q.validate_int(0, 0));
        assert!(!q.validate_int(0, 3)); // limits [0, 0] from the bit patterns
        let mut r = ArgSchema::default();
        r.add_double_range("p", 0, 0.0, 1.0);
        assert!(!r.is_optional_double(0));
    }

    #[test]
    fn shared_slots_and_case() {
        // SaveLoadActions: "filename" (optional) and "role" (required) share
        // string slot 0; the slot holds "role".
        let mut s = ArgSchema::default();
        s.add_string("filename", 0, "");
        s.add_required("role", 0, ArgType::String);
        let mut fb = Feedback::new();
        let a = ArgContainer::load("filename=x", &s, &mut fb).unwrap();
        assert_eq!(a.get_string(0), "x");
        assert!(ArgContainer::load("", &s, &mut fb).is_none());
        assert_eq!(
            fb.error_messages(),
            vec!["required argument 'role' was not found"]
        );

        let mut cs = ArgSchema::new(';', ':', true);
        cs.add_int("Key", 0, 1);
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("key:2", &cs, &mut fb).is_none());
        let a = ArgContainer::load(" Key :2;", &cs, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 2);
        let mut c = ArgContainer::default();
        c.set_int(2, 4);
        c.set_double(1, 0.5);
        c.set_string(0, "s");
        assert_eq!(
            (c.get_int(2), c.get_int(0), c.get_double(1), c.get_string(0)),
            (4, 0, 0.5, "s")
        );
    }
}
