//! Port of `Avida::Genome` (avida-core/include/public/avida/core/Genome.h,
//! avida-core/source/core/Genome.cc) and the minimal `Avida::PropertyMap`
//! (avida-core/include/public/avida/core/Properties.h) it is built from.

use crate::cstr::as_int;
use crate::sequence::InstructionSequence;

/// `s_prop_id_instset` (Genome.cc / cHardwareManager.cc).
pub const PROP_ID_INSTSET: &str = "instset";
/// Description registered by `cHardwareManager::Initialize()`.
pub const PROP_DESC_INSTSET: &str = "Instruction Set";
/// Name that `cHardwareManager::GetInstSet` maps to the first inst set.
pub const DEFAULT_INST_SET_NAME: &str = "(default)";

/// `Avida::StringProperty`: the only property kind genomes carry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Property {
    /// `ID()`
    pub id: String,
    /// `Description()`
    pub description: String,
    /// `StringValue()`
    pub value: String,
}

/// `Avida::HashPropertyMap`, reduced to what Avida stores: string-valued
/// properties keyed by id, kept in definition order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PropertyMap {
    props: Vec<Property>,
}

impl PropertyMap {
    /// Empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// `GetSize()`.
    pub fn len(&self) -> usize {
        self.props.len()
    }

    /// True when no property is defined.
    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }

    /// `Has(id)`.
    pub fn has(&self, id: &str) -> bool {
        self.props.iter().any(|p| p.id == id)
    }

    /// `Get(id)`; `None` where C++ returns the empty default property.
    pub fn get(&self, id: &str) -> Option<&Property> {
        self.props.iter().find(|p| p.id == id)
    }

    /// `Get(id).StringValue()` ("" for an undefined id, like the C++
    /// `s_default_prop`).
    pub fn string_value(&self, id: &str) -> &str {
        self.get(id).map(|p| p.value.as_str()).unwrap_or("")
    }

    /// `Define(p)`: add, or replace a property with the same id.
    pub fn define(&mut self, p: Property) {
        if let Some(e) = self.props.iter_mut().find(|e| e.id == p.id) {
            *e = p;
        } else {
            self.props.push(p);
        }
    }

    /// `SetValue(id, value)`: false when the id is not defined.
    pub fn set_value(&mut self, id: &str, value: &str) -> bool {
        match self.props.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                e.value = value.to_string();
                true
            }
            None => false,
        }
    }

    /// `Remove(id)`.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.props.len();
        self.props.retain(|p| p.id != id);
        self.props.len() != before
    }

    /// Properties in definition order.
    pub fn iter(&self) -> std::slice::Iter<'_, Property> {
        self.props.iter()
    }
}

/// `cHardwareManager::SetupPropertyMap(props, inst_set)`: define the
/// `instset` property.
pub fn setup_property_map(props: &mut PropertyMap, inst_set: &str) {
    props.define(Property {
        id: PROP_ID_INSTSET.to_string(),
        description: PROP_DESC_INSTSET.to_string(),
        value: inst_set.to_string(),
    });
}

/// One column written by `Genome::LegacySave` into a data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegacyValue {
    /// `df.Write(int, ...)`
    Int(i32),
    /// `df.Write(Apto::String, ...)`
    Str(String),
}

/// `Avida::Genome`: hardware type, properties (the instruction set name)
/// and the genetic representation.
///
/// The C++ holds the representation through a polymorphic smart pointer;
/// the only representation Avida uses is `InstructionSequence`, which is
/// owned directly here (`Genome::Genome(const Genome&)` and `operator=`
/// clone it anyway). Epigenetic objects are not ported (unused in Avida).
#[derive(Debug, Clone)]
pub struct Genome {
    hw_type: i32,
    /// The `InstSetPropertyMap`'s single `instset` property value.
    inst_set: String,
    representation: InstructionSequence,
}

impl Default for Genome {
    /// `Genome()`: hardware type -1, empty inst set name.
    ///
    /// QUIRK: the C++ representation pointer is NULL; here it is an empty
    /// sequence.
    fn default() -> Self {
        Genome {
            hw_type: -1,
            inst_set: String::new(),
            representation: InstructionSequence::new(),
        }
    }
}

impl Genome {
    /// `Genome(HardwareTypeID hw, const PropertyMap& props, GeneticRepresentationPtr rep)`:
    /// only the `instset` property is copied.
    pub fn new(hw_type: i32, props: &PropertyMap, rep: InstructionSequence) -> Self {
        Genome {
            hw_type,
            inst_set: props.string_value(PROP_ID_INSTSET).to_string(),
            representation: rep,
        }
    }

    /// Shorthand for `new` with a property map holding just `inst_set`.
    pub fn with_inst_set(hw_type: i32, inst_set: &str, rep: InstructionSequence) -> Self {
        Genome {
            hw_type,
            inst_set: inst_set.to_string(),
            representation: rep,
        }
    }

    /// `explicit Genome(const Apto::String& genome_str)`: parse the
    /// `"<hw_type>,<inst_set>,<sequence>"` form written by `AsString()`.
    /// The hardware type is read with `strtol(.., 0)` (non-numbers give 0).
    pub fn from_string(genome_str: &str) -> Self {
        let mut s = genome_str.to_string();
        let hw = crate::cstr::pop(&mut s, ',');
        let inst_set = crate::cstr::pop(&mut s, ',');
        Genome {
            hw_type: as_int(&hw),
            inst_set,
            representation: InstructionSequence::from_string(&s),
        }
    }

    /// Rebuild a genome from the `hw_type`, `inst_set` and `sequence`
    /// columns of a `.spop` line (`Systematics::Genotype(world, props)`,
    /// avida-core/source/systematics/Genotype.cc): an empty inst set name
    /// becomes `"(default)"`.
    pub fn from_spop_fields(hw_type: &str, inst_set: &str, sequence: &str) -> Self {
        let inst_set = if inst_set.is_empty() {
            DEFAULT_INST_SET_NAME
        } else {
            inst_set
        };
        Genome {
            hw_type: as_int(hw_type),
            inst_set: inst_set.to_string(),
            representation: InstructionSequence::from_string(sequence),
        }
    }

    /// `HardwareType()`.
    pub fn hardware_type(&self) -> i32 {
        self.hw_type
    }

    /// `Properties().Get("instset").StringValue()`.
    pub fn inst_set(&self) -> &str {
        &self.inst_set
    }

    /// `Properties()`: a copy of the genome's property map (one `instset`
    /// entry).
    pub fn properties(&self) -> PropertyMap {
        let mut p = PropertyMap::new();
        setup_property_map(&mut p, &self.inst_set);
        p
    }

    /// `Properties().SetValue(id, value)`: only `instset` is settable.
    pub fn set_property(&mut self, id: &str, value: &str) -> bool {
        if id == PROP_ID_INSTSET {
            self.inst_set = value.to_string();
            return true;
        }
        false
    }

    /// `Representation()`.
    pub fn representation(&self) -> &InstructionSequence {
        &self.representation
    }

    /// `Representation()` (mutable).
    pub fn representation_mut(&mut self) -> &mut InstructionSequence {
        &mut self.representation
    }

    /// Replace the representation.
    pub fn set_representation(&mut self, rep: InstructionSequence) {
        self.representation = rep;
    }

    /// `AsString()`: `"%d,%s,%s"` of hardware type, inst set, sequence.
    pub fn as_string(&self) -> String {
        format!(
            "{},{},{}",
            self.hw_type,
            self.inst_set,
            self.representation.as_string()
        )
    }

    /// `LegacySave(df)`: the three `(value, description, name)` columns
    /// written to data files such as `.spop`.
    pub fn legacy_save_columns(&self) -> [(LegacyValue, &'static str, &'static str); 3] {
        [
            (
                LegacyValue::Int(self.hw_type),
                "Hardware Type ID",
                "hw_type",
            ),
            (
                LegacyValue::Str(self.inst_set.clone()),
                "Inst Set Name",
                "inst_set",
            ),
            (
                LegacyValue::Str(self.representation.as_string()),
                "Genome Sequence",
                "sequence",
            ),
        ]
    }
}

impl PartialEq for Genome {
    /// `operator==`: hardware type, `instset` property and sequence.
    fn eq(&self, other: &Self) -> bool {
        self.hw_type == other.hw_type
            && self.inst_set == other.inst_set
            && self.representation == other.representation
    }
}

impl Eq for Genome {}

#[cfg(test)]
mod tests {
    use super::*;

    const SPOP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../avida-core/tests/heads_default_100u/expected/data/detail-100.spop"
    );

    #[test]
    fn string_round_trip() {
        let g = Genome::from_string("0,heads_default,rucavab");
        assert_eq!(g.hardware_type(), 0);
        assert_eq!(g.inst_set(), "heads_default");
        assert_eq!(g.representation().size(), 7);
        assert_eq!(g.as_string(), "0,heads_default,rucavab");
        assert_eq!(Genome::from_string(&g.as_string()), g);
        let d = Genome::default();
        assert_eq!(d.as_string(), "-1,,");
        let h = Genome::from_string("0x2,x");
        assert_eq!(
            (h.hardware_type(), h.inst_set(), h.representation().size()),
            (2, "x", 0)
        );
    }

    #[test]
    fn properties_and_equality() {
        let mut props = PropertyMap::new();
        setup_property_map(&mut props, "heads_default");
        assert_eq!(
            props.get(PROP_ID_INSTSET).unwrap().description,
            "Instruction Set"
        );
        let g = Genome::new(0, &props, InstructionSequence::from_string("abc"));
        assert_eq!(g.properties(), props);
        let mut h = g.clone();
        assert_eq!(g, h);
        assert!(h.set_property("instset", "other"));
        assert!(!h.set_property("foo", "x"));
        assert_ne!(g, h);
        let g2 = Genome::new(
            3,
            &PropertyMap::new(),
            InstructionSequence::from_string("abc"),
        );
        assert_eq!(g2.inst_set(), "");
    }

    #[test]
    fn spop_lines_round_trip() {
        let text = std::fs::read_to_string(SPOP).expect("spop file");
        let mut format: Vec<&str> = Vec::new();
        let mut n = 0;
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("#format ") {
                format = rest.split_whitespace().collect();
                continue;
            }
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split_whitespace().collect();
            let col = |name: &str| cols[format.iter().position(|f| *f == name).unwrap()];
            let g = Genome::from_spop_fields(col("hw_type"), col("inst_set"), col("sequence"));
            assert_eq!(g.hardware_type(), 0);
            assert_eq!(g.inst_set(), "heads_default");
            assert_eq!(
                g.representation().size(),
                col("length").parse::<i32>().unwrap()
            );
            let cols_out = g.legacy_save_columns();
            assert_eq!(cols_out[0].0, LegacyValue::Int(0));
            assert_eq!(cols_out[1].0, LegacyValue::Str("heads_default".into()));
            assert_eq!(cols_out[2].0, LegacyValue::Str(col("sequence").to_string()));
            assert_eq!(cols_out[2].2, "sequence");
            n += 1;
        }
        assert!(n > 10);
        assert_eq!(
            Genome::from_spop_fields("0", "", "ab").inst_set(),
            "(default)"
        );
    }
}
