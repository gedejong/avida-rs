//! Values of group properties, as read through `Avida::PropertyMap::Get`
//! (avida-core/include/public/avida/core/Properties.h,
//! avida-core/source/core/Properties.cc).
//!
//! Systematics groups expose their statistics as a property map that print
//! actions read by name and convert with `StringValue()`, `IntValue()`,
//! `DoubleValue()` or `Apto::StrAs`. [`PropertyValue`] carries the value
//! together with the conversion rules of the C++ property class it came
//! from.

use avida_apto::fmt::fmt_g;
use avida_config::string_util::{as_int, strtod};

/// One property value with its C++ conversion behaviour.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// `PropertyMap::s_default_prop`, returned for an undefined id: a
    /// `StringProperty` with the value `""` and type `"null"`.
    Null,
    /// `ReferenceProperty<bool>` / `FunctorProperty<bool>`.
    Bool(bool),
    /// `ReferenceProperty<int>` / `FunctorProperty<int>`.
    Int(i32),
    /// `ReferenceProperty<double>` / `FunctorProperty<double>`.
    Double(f64),
    /// `ReferenceProperty<Apto::String>` / `FunctorProperty<Apto::String>`.
    Str(String),
    /// `StringProperty` built from an `int` (type `"int"`, stored as text),
    /// e.g. `src_transmission_type`.
    IntText(String),
}

impl PropertyValue {
    /// `Type()`: the `PropertyTypeID`.
    pub fn type_id(&self) -> &'static str {
        match self {
            PropertyValue::Null => "null",
            PropertyValue::Bool(_) => "bool",
            PropertyValue::Int(_) | PropertyValue::IntText(_) => "int",
            PropertyValue::Double(_) => "float",
            PropertyValue::Str(_) => "string",
        }
    }

    /// `StringValue()`: `Apto::AsStr` (`%d`, `%g`, `"true"`/`"false"`).
    pub fn string_value(&self) -> String {
        match self {
            PropertyValue::Null => String::new(),
            PropertyValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            PropertyValue::Int(i) => i.to_string(),
            PropertyValue::Double(d) => fmt_g(*d, 6),
            PropertyValue::Str(s) | PropertyValue::IntText(s) => s.clone(),
        }
    }

    /// `IntValue()`: numeric values convert as C casts (truncation; the
    /// arm64 conversion saturates and maps NaN to 0, as `as i32` does), text
    /// through `Apto::StrAs` (`strtol(s, NULL, 0)`).
    pub fn int_value(&self) -> i32 {
        match self {
            PropertyValue::Null => 0,
            PropertyValue::Bool(b) => i32::from(*b),
            PropertyValue::Int(i) => *i,
            PropertyValue::Double(d) => *d as i32,
            PropertyValue::Str(s) | PropertyValue::IntText(s) => as_int(s),
        }
    }

    /// `DoubleValue()`: text through `strtod`.
    pub fn double_value(&self) -> f64 {
        match self {
            PropertyValue::Null => 0.0,
            PropertyValue::Bool(b) => f64::from(u8::from(*b)),
            PropertyValue::Int(i) => f64::from(*i),
            PropertyValue::Double(d) => *d,
            PropertyValue::Str(s) | PropertyValue::IntText(s) => strtod(s),
        }
    }

    /// `(bool)Apto::StrAs(StringValue())`, the way print actions read flags
    /// such as `threshold`: true for `"1"`, `"T"`, `"t"` and any casing of
    /// `"true"`.
    pub fn bool_value(&self) -> bool {
        str_as_bool(&self.string_value())
    }
}

/// `Apto::StrAs::operator bool()`.
pub fn str_as_bool(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() == 1 {
        return matches!(b[0], b'1' | b'T' | b't');
    }
    b.len() == 4 && s.eq_ignore_ascii_case("true")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(PropertyValue::Null.string_value(), "");
        assert_eq!(PropertyValue::Null.int_value(), 0);
        assert_eq!(PropertyValue::Null.type_id(), "null");
        assert_eq!(PropertyValue::Bool(true).string_value(), "true");
        assert!(PropertyValue::Bool(true).bool_value());
        assert!(!PropertyValue::Bool(false).bool_value());
        assert_eq!(PropertyValue::Double(0.25).string_value(), "0.25");
        assert_eq!(PropertyValue::Double(1.0 / 3.0).string_value(), "0.333333");
        assert_eq!(PropertyValue::Double(2.9).int_value(), 2);
        assert_eq!(PropertyValue::Double(f64::NAN).int_value(), 0);
        assert_eq!(PropertyValue::Str("0x10".into()).int_value(), 16);
        assert_eq!(PropertyValue::Str("3,4".into()).int_value(), 3);
        assert_eq!(PropertyValue::IntText("1".into()).type_id(), "int");
        assert_eq!(PropertyValue::Str("2.5e1".into()).double_value(), 25.0);
        assert!(str_as_bool("TrUe") && str_as_bool("1") && !str_as_bool("yes"));
    }
}
