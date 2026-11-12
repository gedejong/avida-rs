//! Private minimal port of `cArgSchema` / `cArgContainer`
//! (avida-core/source/tools/cArgSchema.{h,cc}, cArgContainer.{h,cc}), just
//! enough for `cEnvironment::LoadStateGrid` (required int arguments with
//! range limits and required string arguments). The full port belongs to
//! the tasks crate.

use avida_config::feedback::Feedback;
use avida_config::string_util::{as_int, pop, trim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgType {
    Int,
    Str,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    ty: ArgType,
    index: usize,
    range: Option<(i32, i32)>,
}

/// `cArgSchema` (required entries only).
#[derive(Debug, Clone)]
pub(crate) struct ArgSchema {
    entries: Vec<Entry>,
    ints: Vec<Option<usize>>,
    strings: Vec<Option<usize>>,
    sep_entry: char,
    sep_value: char,
}

/// `Apto::String::Trim` + `ToLower` (`cArgSchema::AdjustArgName`, case
/// insensitive schema).
fn adjust_arg_name(name: &str) -> String {
    let b = name.as_bytes();
    let ws = |c: u8| c == b' ' || c == b'\t' || c == b'\r' || c == b'\n';
    let mut s = 0;
    while s < b.len() && ws(b[s]) {
        s += 1;
    }
    let mut e = b.len();
    while e > s && ws(b[e - 1]) {
        e -= 1;
    }
    name[s..e].to_ascii_lowercase()
}

impl ArgSchema {
    /// `cArgSchema(entry, value)`.
    pub(crate) fn new(sep_entry: char, sep_value: char) -> Self {
        ArgSchema {
            entries: Vec::new(),
            ints: Vec::new(),
            strings: Vec::new(),
            sep_entry,
            sep_value,
        }
    }

    fn add(&mut self, name: &str, index: usize, ty: ArgType, range: Option<(i32, i32)>) -> bool {
        let name = adjust_arg_name(name);
        if self.entries.iter().any(|e| e.name == name) {
            return false;
        }
        let slot = self.entries.len();
        self.entries.push(Entry {
            name,
            ty,
            index,
            range,
        });
        let v = match ty {
            ArgType::Int => &mut self.ints,
            ArgType::Str => &mut self.strings,
        };
        if v.len() <= index {
            v.resize(index + 1, None);
        }
        v[index] = Some(slot);
        true
    }

    /// `AddEntry(name, idx, lower, upper)`: required int with range limits.
    pub(crate) fn add_int_range(&mut self, name: &str, index: usize, lower: i32, upper: i32) {
        self.add(name, index, ArgType::Int, Some((lower, upper)));
    }

    /// `AddEntry(name, idx, SCHEMA_STRING)`: required string.
    pub(crate) fn add_string(&mut self, name: &str, index: usize) {
        self.add(name, index, ArgType::Str, None);
    }

    fn entry_name(&self, slot: Option<usize>) -> Option<&str> {
        slot.map(|s| self.entries[s].name.as_str())
    }
}

/// `cArgContainer`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ArgContainer {
    ints: Vec<i32>,
    strings: Vec<String>,
}

impl ArgContainer {
    /// `cArgContainer::Load(args, schema, feedback)`: `None` (C++ `NULL`)
    /// after an unrecognised argument or a missing required one.
    ///
    /// QUIRKS kept from the C++: parsing stops at the first empty entry, and
    /// an out-of-range int only logs an error without failing the load.
    pub(crate) fn load(args: &str, schema: &ArgSchema, feedback: &mut Feedback) -> Option<Self> {
        let mut args = args.to_string();
        let mut set_ints = vec![false; schema.ints.len()];
        let mut set_strings = vec![false; schema.strings.len()];
        let mut ret = ArgContainer {
            ints: vec![0; schema.ints.len()],
            strings: vec![String::new(); schema.strings.len()],
        };
        let mut success = true;

        let mut arg_ent = pop(&mut args, schema.sep_entry);
        while !arg_ent.is_empty() {
            let arg_name = adjust_arg_name(&pop(&mut arg_ent, schema.sep_value));
            match schema.entries.iter().find(|e| e.name == arg_name) {
                Some(e) => match e.ty {
                    ArgType::Int => {
                        let i = e.index;
                        set_ints[i] = true;
                        ret.ints[i] = as_int(&arg_ent);
                        let v = ret.ints[i];
                        let slot = schema.ints[i];
                        let ok = match slot {
                            Some(s) => match schema.entries[s].range {
                                Some((lo, hi)) => v <= hi && v >= lo,
                                None => true,
                            },
                            None => false,
                        };
                        if !ok {
                            match schema.entry_name(slot) {
                                Some(n) => feedback
                                    .error(format!("value of '{}' exceeds its defined range", n)),
                                None => feedback
                                    .error(format!("invalid int schema entry at index {}", i)),
                            }
                        }
                    }
                    ArgType::Str => {
                        let i = e.index;
                        set_strings[i] = true;
                        trim(&mut arg_ent);
                        ret.strings[i] = arg_ent.clone();
                    }
                },
                None => {
                    success = false;
                    feedback.error(format!("unrecognized argument: '{}'", arg_name));
                }
            }
            arg_ent = pop(&mut args, schema.sep_entry);
        }

        for i in 0..set_ints.len() {
            if set_ints[i] {
                continue;
            }
            success = false;
            match schema.entry_name(schema.ints[i]) {
                Some(n) => feedback.error(format!("required argument '{}' was not found", n)),
                None => feedback.error(format!("invalid int schema entry at index {}", i)),
            }
        }
        for i in 0..set_strings.len() {
            if set_strings[i] {
                continue;
            }
            success = false;
            match schema.entry_name(schema.strings[i]) {
                Some(n) => feedback.error(format!("required argument '{}' was not found", n)),
                None => feedback.error(format!("invalid string schema entry at index {}", i)),
            }
        }

        if success {
            Some(ret)
        } else {
            None
        }
    }

    /// `GetInt(i)`.
    pub(crate) fn int(&self, i: usize) -> i32 {
        self.ints[i]
    }

    /// `GetString(i)`.
    pub(crate) fn string(&self, i: usize) -> &str {
        &self.strings[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> ArgSchema {
        let mut s = ArgSchema::new(':', '=');
        s.add_int_range("width", 0, 0, i32::MAX);
        s.add_int_range("facing", 1, 0, 7);
        s.add_string("states", 0);
        s
    }

    #[test]
    fn load_ok_and_range() {
        let mut fb = Feedback::new();
        let a = ArgContainer::load("WIDTH = 5:facing=9:states= a,b ", &schema(), &mut fb).unwrap();
        assert_eq!(a.int(0), 5);
        assert_eq!(a.int(1), 9);
        assert_eq!(a.string(0), "a,b");
        assert_eq!(fb.message(0), "value of 'facing' exceeds its defined range");
    }

    #[test]
    fn load_failures() {
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("width=1:bogus=2", &schema(), &mut fb).is_none());
        assert_eq!(fb.message(0), "unrecognized argument: 'bogus'");
        assert_eq!(fb.message(1), "required argument 'facing' was not found");
        assert_eq!(fb.message(2), "required argument 'states' was not found");
        // parsing stops at the first empty entry
        let mut fb = Feedback::new();
        assert!(ArgContainer::load("width=1::facing=2:states=x", &schema(), &mut fb).is_none());
        assert_eq!(fb.num_errors(), 2);
    }
}
