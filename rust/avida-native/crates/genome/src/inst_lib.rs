//! Data part of `cInstLib` / `tInstLib<T>` / `cInstLibEntry` /
//! `tInstLibEntry<T>` (avida-core/source/cpu/cInstLib.h, tInstLib.h,
//! cInstLibEntry.h, tInstLibEntry.h).
//!
//! The per-hardware instruction *functions* live in the hardware crate,
//! which builds an [`InstLib`] from its table and keeps a parallel function
//! array indexed by the same library index (`cInstSet::GetLibFunctionIndex`).

use crate::cstr::apto_near_match;
use crate::instruction::Instruction;

/// `nInstFlag` (cInstLibEntry.h).
pub mod inst_flag {
    /// `DEFAULT`
    pub const DEFAULT: u32 = 0x1;
    /// `NOP`
    pub const NOP: u32 = 0x2;
    /// `LABEL`
    pub const LABEL: u32 = 0x4;
    /// `STALL`
    pub const STALL: u32 = 0x8;
    /// `SLEEP`
    pub const SLEEP: u32 = 0x10;
    /// `PROMOTER`
    pub const PROMOTER: u32 = 0x20;
    /// `TERMINATOR`
    pub const TERMINATOR: u32 = 0x40;
    /// `IMMEDIATE_VALUE`
    pub const IMMEDIATE_VALUE: u32 = 0x80;
}

/// `InstructionClass` (cInstLibEntry.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum InstructionClass {
    /// `INST_CLASS_NOP`
    Nop = 0,
    /// `INST_CLASS_FLOW_CONTROL`
    FlowControl,
    /// `INST_CLASS_CONDITIONAL`
    Conditional,
    /// `INST_CLASS_ARITHMETIC_LOGIC`
    ArithmeticLogic,
    /// `INST_CLASS_DATA`
    Data,
    /// `INST_CLASS_ENVIRONMENT`
    Environment,
    /// `INST_CLASS_LIFECYCLE`
    Lifecycle,
    /// `INST_CLASS_OTHER`
    Other,
}

/// `NUM_INST_CLASSES`
pub const NUM_INST_CLASSES: i32 = 8;

/// `BehavClass` (cInstLibEntry.h).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum BehavClass {
    /// `BEHAV_CLASS_INPUT`
    Input = 0,
    /// `BEHAV_CLASS_ACTION`
    Action,
    /// `BEHAV_CLASS_COPY`
    Copy,
    /// `BEHAV_CLASS_NONE`
    None,
    /// `BEHAV_CLASS_BREAK`
    Break,
    /// `BEHAV_CLASS_END_GENE`
    EndGene,
}

/// `cInstLibEntry` (without the function pointer of `tInstLibEntry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstLibEntry {
    name: String,
    flags: u32,
    description: String,
    class: InstructionClass,
    behav_class: BehavClass,
}

impl InstLibEntry {
    /// `cInstLibEntry(name, class, flags, desc, bclass)`.
    pub fn new(
        name: &str,
        class: InstructionClass,
        flags: u32,
        description: &str,
        behav_class: BehavClass,
    ) -> Self {
        InstLibEntry {
            name: name.to_string(),
            flags,
            description: description.to_string(),
            class,
            behav_class,
        }
    }

    /// `tInstLibEntry(name, fun)` with the default arguments:
    /// `INST_CLASS_OTHER`, no flags, empty description, `BEHAV_CLASS_NONE`.
    pub fn named(name: &str) -> Self {
        InstLibEntry::new(name, InstructionClass::Other, 0, "", BehavClass::None)
    }

    /// `GetName()`
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `GetDescription()`
    pub fn description(&self) -> &str {
        &self.description
    }
    /// `GetClass()`
    pub fn class(&self) -> InstructionClass {
        self.class
    }
    /// `GetBehavClass()`
    pub fn behav_class(&self) -> BehavClass {
        self.behav_class
    }
    /// `GetFlags()`
    pub fn flags(&self) -> u32 {
        self.flags
    }
    /// `IsDefault()`
    pub fn is_default(&self) -> bool {
        self.flags & inst_flag::DEFAULT != 0
    }
    /// `IsNop()`
    pub fn is_nop(&self) -> bool {
        self.flags & inst_flag::NOP != 0
    }
    /// `IsLabel()`
    pub fn is_label(&self) -> bool {
        self.flags & inst_flag::LABEL != 0
    }
    /// `IsPromoter()`
    pub fn is_promoter(&self) -> bool {
        self.flags & inst_flag::PROMOTER != 0
    }
    /// `IsTerminator()`
    pub fn is_terminator(&self) -> bool {
        self.flags & inst_flag::TERMINATOR != 0
    }
    /// `ShouldStall()`
    pub fn should_stall(&self) -> bool {
        self.flags & inst_flag::STALL != 0
    }
    /// `ShouldSleep()`
    pub fn should_sleep(&self) -> bool {
        self.flags & inst_flag::SLEEP != 0
    }
    /// `IsImmediateValue()`
    pub fn is_immediate_value(&self) -> bool {
        self.flags & inst_flag::IMMEDIATE_VALUE != 0
    }
}

/// `cInstLib` / `tInstLib<T>` data: the ordered entries, the nop table
/// (names and register modifiers of the first entries), the default and
/// NULL instruction indices, and the name -> index map.
#[derive(Debug, Clone)]
pub struct InstLib {
    entries: Vec<InstLibEntry>,
    nop_names: Vec<String>,
    nop_mods: Vec<i32>,
    inst_default: i32,
    inst_null: i32,
    /// `m_namemap` keys in first-insertion order with their (last) index.
    name_map: Vec<(String, i32)>,
}

impl InstLib {
    /// `tInstLib(size, entries, nopmod_names, nopmods, functions, def, null_inst)`.
    ///
    /// `nops` are the `(name, nop_mod)` pairs of the hardware's NOP table;
    /// as the C++ requires, they correspond to the first `entries`.
    /// A duplicated entry name maps to its last index (`Apto::Map::Set`).
    pub fn new(
        entries: Vec<InstLibEntry>,
        nops: Vec<(String, i32)>,
        inst_default: i32,
        inst_null: i32,
    ) -> Self {
        let mut name_map: Vec<(String, i32)> = Vec::with_capacity(entries.len());
        for (i, e) in entries.iter().enumerate() {
            if let Some(slot) = name_map.iter_mut().find(|(n, _)| n == e.name()) {
                slot.1 = i as i32;
            } else {
                name_map.push((e.name().to_string(), i as i32));
            }
        }
        let (nop_names, nop_mods) = nops.into_iter().unzip();
        InstLib {
            entries,
            nop_names,
            nop_mods,
            inst_default,
            inst_null,
            name_map,
        }
    }

    /// `GetSize()`.
    pub fn size(&self) -> i32 {
        self.entries.len() as i32
    }

    /// `Get(i)` / `operator[](i)`.
    pub fn get(&self, i: i32) -> &InstLibEntry {
        &self.entries[i as usize]
    }

    /// All entries in library order.
    pub fn entries(&self) -> &[InstLibEntry] {
        &self.entries
    }

    /// `GetIndex(name)`: library index or -1.
    pub fn get_index(&self, name: &str) -> i32 {
        self.name_map
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, i)| *i)
            .unwrap_or(-1)
    }

    /// `GetNearMatch(name)`: `Apto::NearMatch(name, m_namemap.Keys())`,
    /// iterating the keys in Apto hash-map order (and with Apto's
    /// `EditDistance`), so ties resolve as in C++. Returns "" when no name
    /// is closer than `name.len()` edits.
    pub fn near_match(&self, name: &str) -> String {
        let keys: Vec<&str> = self.name_map.iter().map(|(n, _)| n.as_str()).collect();
        apto_near_match(name, &keys)
    }

    /// `GetName(entry)`.
    pub fn name(&self, entry: i32) -> &str {
        self.get(entry).name()
    }

    /// `GetNopName(id)`.
    pub fn nop_name(&self, id: u32) -> &str {
        &self.nop_names[id as usize]
    }

    /// `GetNopMod(id)`: register modifier of library nop `id`.
    pub fn nop_mod(&self, id: u32) -> i32 {
        self.nop_mods[id as usize]
    }

    /// `GetNopMod(const Instruction&)` (`GetNopMod(inst.GetOp())`).
    pub fn nop_mod_of(&self, inst: Instruction) -> i32 {
        self.nop_mod(inst.op() as u32)
    }

    /// Number of entries in the NOP table.
    pub fn num_nops(&self) -> i32 {
        self.nop_mods.len() as i32
    }

    /// `GetInstDefault()`.
    pub fn inst_default(&self) -> i32 {
        self.inst_default
    }

    /// `GetInstNull()`.
    pub fn inst_null(&self) -> i32 {
        self.inst_null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_lookup() {
        let lib = InstLib::new(
            vec![
                InstLibEntry::new(
                    "nop-A",
                    InstructionClass::Nop,
                    inst_flag::DEFAULT | inst_flag::NOP,
                    "",
                    BehavClass::None,
                ),
                InstLibEntry::new(
                    "nop-B",
                    InstructionClass::Nop,
                    inst_flag::NOP,
                    "",
                    BehavClass::None,
                ),
                InstLibEntry::named("inc"),
                InstLibEntry::new(
                    "label",
                    InstructionClass::FlowControl,
                    inst_flag::LABEL,
                    "",
                    BehavClass::None,
                ),
                InstLibEntry::named("inc"),
                InstLibEntry::named("NULL"),
            ],
            vec![("nop-A".into(), 0), ("nop-B".into(), 1)],
            0,
            5,
        );
        assert_eq!(lib.size(), 6);
        assert_eq!(lib.get_index("inc"), 4);
        assert_eq!(lib.get_index("dec"), -1);
        assert_eq!(lib.name(3), "label");
        assert!(lib.get(3).is_label() && !lib.get(3).is_nop());
        assert!(lib.get(0).is_default() && lib.get(0).is_nop());
        assert_eq!(
            (lib.nop_mod(1), lib.nop_name(0), lib.num_nops()),
            (1, "nop-A", 2)
        );
        assert_eq!(lib.nop_mod_of(Instruction::new(1)), 1);
        assert_eq!((lib.inst_default(), lib.inst_null()), (0, 5));
        assert_eq!(lib.get(2).class(), InstructionClass::Other);
        assert_eq!(lib.get(2).behav_class(), BehavClass::None);
        // NearMatch results are checked against the reference binary in
        // real_config_tests; here only that it returns a key or "".
        let m = lib.near_match("nop-C");
        assert!(m.is_empty() || lib.get_index(&m) >= 0);
    }
}
