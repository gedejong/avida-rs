//! Private port of `cArgSchema` and `cArgContainer`
//! (avida-core/source/tools/cArgSchema.{h,cc}, cArgContainer.{h,cc}).
//!
//! The genome crate is building the shared argument machinery; when that lands
//! this module should be replaced by it (same method set, same messages).

use crate::cstr;
use std::collections::HashMap;

/// `cArgSchema::tType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    /// `SCHEMA_INT`
    Int,
    /// `SCHEMA_DOUBLE`
    Double,
    /// `SCHEMA_STRING`
    Str,
}

/// Severity of a [`Feedback`] message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    /// `Feedback::Error`
    Error,
    /// `Feedback::Warning`
    Warning,
    /// `Feedback::Notify`
    Notify,
}

/// Collecting stand-in for `Avida::Feedback` / `cUserFeedback`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feedback {
    /// Messages in the order they were emitted.
    pub messages: Vec<(FeedbackKind, String)>,
}

impl Feedback {
    /// New empty feedback sink.
    pub fn new() -> Self {
        Self::default()
    }
    /// `Feedback::Error(...)`.
    pub fn error(&mut self, msg: impl Into<String>) {
        self.messages.push((FeedbackKind::Error, msg.into()));
    }
    /// `Feedback::Warning(...)`.
    pub fn warning(&mut self, msg: impl Into<String>) {
        self.messages.push((FeedbackKind::Warning, msg.into()));
    }
    /// `Feedback::Notify(...)`.
    pub fn notify(&mut self, msg: impl Into<String>) {
        self.messages.push((FeedbackKind::Notify, msg.into()));
    }
    /// `Feedback::GetNumErrors()`.
    pub fn num_errors(&self) -> usize {
        self.messages
            .iter()
            .filter(|(k, _)| *k == FeedbackKind::Error)
            .count()
    }
    /// The error messages only.
    pub fn errors(&self) -> Vec<&str> {
        self.messages
            .iter()
            .filter(|(k, _)| *k == FeedbackKind::Error)
            .map(|(_, m)| m.as_str())
            .collect()
    }
}

/// `cArgSchema::sArgSchemaEntry`.
#[derive(Debug, Clone)]
struct SchemaEntry {
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

impl SchemaEntry {
    fn new(name: String, optional: bool) -> Self {
        SchemaEntry {
            name,
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

/// `cArgSchema`: declares the named, typed, indexed arguments of a task.
#[derive(Debug, Clone)]
pub struct ArgSchema {
    /// All entries ever created (the C++ map keeps entries that a later entry
    /// with the same index displaced from the typed arrays).
    store: Vec<SchemaEntry>,
    /// name -> (type, index, store position)  (`m_entries` + `FindEntry`).
    entries: HashMap<String, (ArgType, i32, usize)>,
    ints: Vec<Option<usize>>,
    doubles: Vec<Option<usize>>,
    strings: Vec<Option<usize>>,
    sep_entry: char,
    sep_value: char,
    case_sensitive: bool,
}

impl Default for ArgSchema {
    fn default() -> Self {
        Self::new()
    }
}

fn place(vec: &mut Vec<Option<usize>>, idx: i32, store_pos: usize) {
    // C++ `Resize(in_idx + 1, NULL)` then assignment. A negative index is
    // undefined behaviour there; it is ignored here.
    if idx < 0 {
        return;
    }
    let idx = idx as usize;
    if vec.len() <= idx {
        vec.resize(idx + 1, None);
    }
    vec[idx] = Some(store_pos);
}

impl ArgSchema {
    /// `cArgSchema()` with the default separators `,` and `=`, case-insensitive.
    pub fn new() -> Self {
        Self::with_separators(',', '=', false)
    }

    /// `cArgSchema(char entry, char value, bool case_sensitive)`.
    pub fn with_separators(entry: char, value: char, case_sensitive: bool) -> Self {
        ArgSchema {
            store: Vec::new(),
            entries: HashMap::new(),
            ints: Vec::new(),
            doubles: Vec::new(),
            strings: Vec::new(),
            sep_entry: entry,
            sep_value: value,
            case_sensitive,
        }
    }

    /// `AdjustArgName`: trim, then lower-case unless case sensitive.
    pub fn adjust_arg_name(&self, name: &str) -> String {
        let t = cstr::trim(name);
        if self.case_sensitive {
            t
        } else {
            t.to_ascii_lowercase()
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

    fn insert(&mut self, name: String, ty: ArgType, idx: i32, entry: SchemaEntry) {
        let pos = self.store.len();
        self.store.push(entry);
        match ty {
            ArgType::Int => place(&mut self.ints, idx, pos),
            ArgType::Double => place(&mut self.doubles, idx, pos),
            ArgType::Str => place(&mut self.strings, idx, pos),
        }
        self.entries.insert(name, (ty, idx, pos));
    }

    /// `AddEntry(name, idx, tType)`: required argument of the given type.
    pub fn add_required(&mut self, name: &str, idx: i32, ty: ArgType) -> bool {
        let name = self.adjust_arg_name(name);
        if self.entries.contains_key(&name) {
            return false;
        }
        let entry = SchemaEntry::new(name.clone(), false);
        self.insert(name, ty, idx, entry);
        true
    }

    /// `AddEntry(name, idx, int def)`: optional int.
    pub fn add_int(&mut self, name: &str, idx: i32, def: i32) -> bool {
        let name = self.adjust_arg_name(name);
        if self.entries.contains_key(&name) {
            return false;
        }
        let mut entry = SchemaEntry::new(name.clone(), true);
        entry.def_int = def;
        self.insert(name, ArgType::Int, idx, entry);
        true
    }

    /// `AddEntry(name, idx, int lower, int upper)`: required int with range.
    pub fn add_int_range(&mut self, name: &str, idx: i32, lower: i32, upper: i32) -> bool {
        if self.add_required(name, idx, ArgType::Int) {
            self.set_int_range(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, int lower, int upper, int def)`: optional int with range.
    pub fn add_int_range_default(
        &mut self,
        name: &str,
        idx: i32,
        lower: i32,
        upper: i32,
        def: i32,
    ) -> bool {
        if self.add_int(name, idx, def) {
            self.set_int_range(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, double def)`: optional double.
    pub fn add_double(&mut self, name: &str, idx: i32, def: f64) -> bool {
        let name = self.adjust_arg_name(name);
        if self.entries.contains_key(&name) {
            return false;
        }
        let mut entry = SchemaEntry::new(name.clone(), true);
        entry.def_double = def;
        self.insert(name, ArgType::Double, idx, entry);
        true
    }

    /// `AddEntry(name, idx, double lower, double upper)`: required double with range.
    ///
    /// QUIRK: the C++ stores the limits through `m_ints[in_idx]` (a copy-paste
    /// bug, out of bounds when no int entry exists). Here the limits are
    /// applied to the double entry that was just added, the evident intent.
    pub fn add_double_range(&mut self, name: &str, idx: i32, lower: f64, upper: f64) -> bool {
        if self.add_required(name, idx, ArgType::Double) {
            self.set_double_range(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, double lower, double upper, double def)`: optional
    /// double with range. Same QUIRK as [`ArgSchema::add_double_range`].
    pub fn add_double_range_default(
        &mut self,
        name: &str,
        idx: i32,
        lower: f64,
        upper: f64,
        def: f64,
    ) -> bool {
        if self.add_double(name, idx, def) {
            self.set_double_range(idx, lower, upper);
            return true;
        }
        false
    }

    /// `AddEntry(name, idx, const Apto::String& def)`: optional string.
    pub fn add_string(&mut self, name: &str, idx: i32, def: &str) -> bool {
        let name = self.adjust_arg_name(name);
        if self.entries.contains_key(&name) {
            return false;
        }
        let mut entry = SchemaEntry::new(name.clone(), true);
        entry.def_string = def.to_string();
        self.insert(name, ArgType::Str, idx, entry);
        true
    }

    fn set_int_range(&mut self, idx: i32, lower: i32, upper: i32) {
        if let Some(Some(pos)) = self.ints.get(idx.max(0) as usize) {
            let e = &mut self.store[*pos];
            e.has_range_limits = true;
            e.r_l_int = lower;
            e.r_u_int = upper;
        }
    }

    fn set_double_range(&mut self, idx: i32, lower: f64, upper: f64) {
        if let Some(Some(pos)) = self.doubles.get(idx.max(0) as usize) {
            let e = &mut self.store[*pos];
            e.has_range_limits = true;
            e.r_l_double = lower;
            e.r_u_double = upper;
        }
    }

    /// `FindEntry`: the name must already be adjusted (the C++ does not adjust
    /// inside `FindEntry`; `cArgContainer::Load` does it first).
    pub fn find_entry(&self, name: &str) -> Option<(ArgType, i32)> {
        self.entries.get(name).map(|(t, i, _)| (*t, *i))
    }

    /// `GetNumIntArgs()`.
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

    fn slot(vec: &[Option<usize>], i: i32) -> Option<usize> {
        if i < 0 {
            return None;
        }
        vec.get(i as usize).copied().flatten()
    }

    /// `IsOptionalInt(i)`.
    pub fn is_optional_int(&self, i: i32) -> bool {
        Self::slot(&self.ints, i).is_some_and(|p| self.store[p].optional)
    }
    /// `IsOptionalDouble(i)`.
    pub fn is_optional_double(&self, i: i32) -> bool {
        Self::slot(&self.doubles, i).is_some_and(|p| self.store[p].optional)
    }
    /// `IsOptionalString(i)`.
    pub fn is_optional_string(&self, i: i32) -> bool {
        Self::slot(&self.strings, i).is_some_and(|p| self.store[p].optional)
    }

    /// `SetDefaultInt(i, v)`: writes the default only for optional entries.
    pub fn set_default_int(&self, i: i32, v: &mut i32) {
        if let Some(p) = Self::slot(&self.ints, i) {
            if self.store[p].optional {
                *v = self.store[p].def_int;
            }
        }
    }
    /// `SetDefaultDouble(i, v)`.
    pub fn set_default_double(&self, i: i32, v: &mut f64) {
        if let Some(p) = Self::slot(&self.doubles, i) {
            if self.store[p].optional {
                *v = self.store[p].def_double;
            }
        }
    }
    /// `SetDefaultString(i, v)`.
    pub fn set_default_string(&self, i: i32, v: &mut String) {
        if let Some(p) = Self::slot(&self.strings, i) {
            if self.store[p].optional {
                *v = self.store[p].def_string.clone();
            }
        }
    }

    /// `GetIntName(i, name)`.
    pub fn int_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.ints, i).map(|p| self.store[p].name.as_str())
    }
    /// `GetDoubleName(i, name)`.
    pub fn double_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.doubles, i).map(|p| self.store[p].name.as_str())
    }
    /// `GetStringName(i, name)`.
    pub fn string_name(&self, i: i32) -> Option<&str> {
        Self::slot(&self.strings, i).map(|p| self.store[p].name.as_str())
    }

    /// `ValidateInt(i, v)`: false for a missing entry or an out-of-range value.
    pub fn validate_int(&self, i: i32, v: i32) -> bool {
        match Self::slot(&self.ints, i) {
            Some(p) => {
                let e = &self.store[p];
                !e.has_range_limits || (v <= e.r_u_int && v >= e.r_l_int)
            }
            None => false,
        }
    }
    /// `ValidateDouble(i, v)`.
    pub fn validate_double(&self, i: i32, v: f64) -> bool {
        match Self::slot(&self.doubles, i) {
            Some(p) => {
                let e = &self.store[p];
                !e.has_range_limits || (v <= e.r_u_double && v >= e.r_l_double)
            }
            None => false,
        }
    }
}

/// `cArgContainer`: argument values loaded against an [`ArgSchema`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArgContainer {
    ints: Vec<i32>,
    doubles: Vec<f64>,
    strings: Vec<String>,
}

impl ArgContainer {
    /// `cArgContainer::Load(args, schema, feedback)`; `None` where the C++
    /// returns `NULL` (the failure reasons are reported through `feedback`).
    pub fn load(args: &str, schema: &ArgSchema, feedback: &mut Feedback) -> Option<ArgContainer> {
        let n_i = schema.num_int_args().max(0) as usize;
        let n_d = schema.num_double_args().max(0) as usize;
        let n_s = schema.num_string_args().max(0) as usize;
        let mut set_ints = vec![false; n_i];
        let mut set_doubles = vec![false; n_d];
        let mut set_strings = vec![false; n_s];
        let mut ret = ArgContainer {
            ints: vec![0; n_i],
            doubles: vec![0.0; n_d],
            strings: vec![String::new(); n_s],
        };

        let mut args = args.to_string();
        let mut success = true;

        let mut arg_ent = cstr::pop(&mut args, schema.entry_separator());
        while !arg_ent.is_empty() {
            let raw_name = cstr::pop(&mut arg_ent, schema.value_separator());
            let arg_name = schema.adjust_arg_name(&raw_name);

            if let Some((ty, index)) = schema.find_entry(&arg_name) {
                let ui = index.max(0) as usize;
                match ty {
                    ArgType::Int => {
                        set_ints[ui] = true;
                        ret.ints[ui] = cstr::as_int(&arg_ent);
                        if !schema.validate_int(index, ret.ints[ui]) {
                            match schema.int_name(index) {
                                Some(n) => feedback
                                    .error(format!("value of '{}' exceeds its defined range", n)),
                                None => feedback
                                    .error(format!("invalid int schema entry at index {}", index)),
                            }
                        }
                    }
                    ArgType::Double => {
                        set_doubles[ui] = true;
                        ret.doubles[ui] = cstr::as_double(&arg_ent);
                        if !schema.validate_double(index, ret.doubles[ui]) {
                            match schema.double_name(index) {
                                Some(n) => feedback
                                    .error(format!("value of '{}' exceeds its defined range", n)),
                                None => feedback.error(format!(
                                    "invalid double schema entry at index {}",
                                    index
                                )),
                            }
                        }
                    }
                    ArgType::Str => {
                        set_strings[ui] = true;
                        ret.strings[ui] = cstr::trim(&arg_ent);
                    }
                }
            } else {
                success = false;
                feedback.error(format!("unrecognized argument: '{}'", arg_name));
            }
            arg_ent = cstr::pop(&mut args, schema.entry_separator());
        }

        for (i, &was_set) in set_ints.iter().enumerate() {
            if was_set {
                continue;
            }
            let i = i as i32;
            if schema.is_optional_int(i) {
                schema.set_default_int(i, &mut ret.ints[i as usize]);
            } else {
                success = false;
                match schema.int_name(i) {
                    Some(n) => feedback.error(format!("required argument '{}' was not found", n)),
                    None => feedback.error(format!("invalid int schema entry at index {}", i)),
                }
            }
        }
        for (i, &was_set) in set_doubles.iter().enumerate() {
            if was_set {
                continue;
            }
            let i = i as i32;
            if schema.is_optional_double(i) {
                schema.set_default_double(i, &mut ret.doubles[i as usize]);
            } else {
                success = false;
                match schema.double_name(i) {
                    Some(n) => feedback.error(format!("required argument '{}' was not found", n)),
                    None => feedback.error(format!("invalid double schema entry at index {}", i)),
                }
            }
        }
        for (i, &was_set) in set_strings.iter().enumerate() {
            if was_set {
                continue;
            }
            let i = i as i32;
            if schema.is_optional_string(i) {
                let mut def = String::new();
                schema.set_default_string(i, &mut def);
                ret.strings[i as usize] = def;
            } else {
                success = false;
                match schema.string_name(i) {
                    Some(n) => feedback.error(format!("required argument '{}' was not found", n)),
                    None => feedback.error(format!("invalid string schema entry at index {}", i)),
                }
            }
        }

        if success {
            Some(ret)
        } else {
            None
        }
    }

    /// `GetInt(i)`. Out-of-range reads (undefined in C++) return 0.
    pub fn get_int(&self, i: i32) -> i32 {
        self.ints.get(i.max(0) as usize).copied().unwrap_or(0)
    }
    /// `GetDouble(i)`. Out-of-range reads return 0.0.
    pub fn get_double(&self, i: i32) -> f64 {
        self.doubles.get(i.max(0) as usize).copied().unwrap_or(0.0)
    }
    /// `GetString(i)`. Out-of-range reads return "".
    pub fn get_string(&self, i: i32) -> &str {
        self.strings
            .get(i.max(0) as usize)
            .map_or("", |s| s.as_str())
    }

    /// `SetInt(i, v)`: grows the array if needed.
    pub fn set_int(&mut self, i: i32, v: i32) {
        let i = i.max(0) as usize;
        if self.ints.len() <= i {
            self.ints.resize(i + 1, 0);
        }
        self.ints[i] = v;
    }
    /// `SetDouble(i, v)`.
    pub fn set_double(&mut self, i: i32, v: f64) {
        let i = i.max(0) as usize;
        if self.doubles.len() <= i {
            self.doubles.resize(i + 1, 0.0);
        }
        self.doubles[i] = v;
    }
    /// `SetString(i, v)`.
    pub fn set_string(&mut self, i: i32, v: &str) {
        let i = i.max(0) as usize;
        if self.strings.len() <= i {
            self.strings.resize(i + 1, String::new());
        }
        self.strings[i] = v.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn match_number_schema() -> ArgSchema {
        // Load_MatchNumber
        let mut s = ArgSchema::new();
        s.add_required("target", 0, ArgType::Int);
        s.add_int("threshold", 1, -1);
        s.add_required("halflife", 0, ArgType::Double);
        s
    }

    #[test]
    fn loads_values_and_defaults() {
        let s = match_number_schema();
        let mut fb = Feedback::new();
        let a = ArgContainer::load("target=5:halflife=2.5", &s, &mut fb);
        // ':' is not the entry separator: whole text is one entry named "target"
        // with value "5:halflife=2.5" -> halflife missing.
        assert!(a.is_none());
        assert_eq!(
            fb.errors(),
            vec!["required argument 'halflife' was not found"]
        );

        let mut fb = Feedback::new();
        let a = ArgContainer::load("target=5,halflife=2.5", &s, &mut fb).unwrap();
        assert!(fb.messages.is_empty());
        assert_eq!(a.get_int(0), 5);
        assert_eq!(a.get_int(1), -1);
        assert_eq!(a.get_double(0), 2.5);
    }

    #[test]
    fn names_are_trimmed_and_case_folded() {
        let s = match_number_schema();
        let mut fb = Feedback::new();
        let a = ArgContainer::load(" Target = 7 , HALFLIFE=1 ,threshold=3", &s, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 7);
        assert_eq!(a.get_int(1), 3);
        assert_eq!(a.get_double(0), 1.0);
    }

    #[test]
    fn unknown_argument_fails() {
        let s = match_number_schema();
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("target=1,halflife=1,bogus=2", &s, &mut fb).is_none());
        assert_eq!(fb.errors(), vec!["unrecognized argument: 'bogus'"]);
    }

    #[test]
    fn range_violation_reports_but_does_not_fail() {
        let mut s = ArgSchema::new();
        s.add_int_range_default("n", 0, 0, 10, 5);
        let mut fb = Feedback::new();
        let a = ArgContainer::load("n=11", &s, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 11);
        assert_eq!(fb.errors(), vec!["value of 'n' exceeds its defined range"]);
        let mut fb = Feedback::new();
        let a = ArgContainer::load("", &s, &mut fb).unwrap();
        assert_eq!(a.get_int(0), 5);
    }

    #[test]
    fn string_args_and_empty_entry_stops_parsing() {
        let mut s = ArgSchema::new();
        s.add_required("string", 0, ArgType::Str);
        s.add_int("partial", 0, 0);
        let mut fb = Feedback::new();
        let a = ArgContainer::load("string=0101, partial=1", &s, &mut fb).unwrap();
        assert_eq!(a.get_string(0), "0101");
        assert_eq!(a.get_int(0), 1);
        // QUIRK: an empty entry ends the loop (`while (arg_ent.GetSize() > 0)`)
        let mut fb = Feedback::new();
        let a = ArgContainer::load("string=11,,partial=1", &s, &mut fb).unwrap();
        assert_eq!(a.get_string(0), "11");
        assert_eq!(a.get_int(0), 0);
    }

    #[test]
    fn duplicate_names_rejected_and_index_gaps_invalid() {
        let mut s = ArgSchema::new();
        assert!(s.add_int("a", 1, 0));
        assert!(!s.add_int("A", 2, 0));
        // index 0 never defined: required-but-missing slot
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("a=3", &s, &mut fb).is_none());
        assert_eq!(fb.errors(), vec!["invalid int schema entry at index 0"]);
    }
}
