//! `cFlexVar` (avida-core/source/tools/cFlexVar.h): a value that is an
//! int, a char, a double or a string, with the C++ cross-type comparisons.

use avida_apto::fmt_double;
use avida_config::string_util as su;

/// `cFlexVar`.
///
/// There is no bool variant: the C++ has no `cFlexVar(bool)` constructor,
/// so a `bool` getter promotes to `int` and is stored as [`FlexVar::Int`].
#[derive(Debug, Clone, PartialEq)]
pub enum FlexVar {
    /// `cFlexVar_Int`.
    Int(i32),
    /// `cFlexVar_Char` (`char` is signed on the reference platforms).
    Char(i8),
    /// `cFlexVar_Double`.
    Double(f64),
    /// `cFlexVar_String`.
    Str(String),
}

impl Default for FlexVar {
    /// `cFlexVar(int in_value = 0)`.
    fn default() -> Self {
        FlexVar::Int(0)
    }
}

/// The six comparison operators the C++ forwards between types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

fn num_cmp<T: PartialOrd>(a: T, b: T, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => a < b,
        CmpOp::Gt => a > b,
        CmpOp::Le => a <= b,
        CmpOp::Ge => a >= b,
    }
}

/// `cString` operators: `==` byte equality, `!=` its negation, the
/// orderings through `cString::Compare` (signed chars).
fn str_cmp(a: &str, b: &str, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::Lt => su::compare(a, b) < 0,
        CmpOp::Gt => su::compare(a, b) > 0,
        CmpOp::Le => su::compare(a, b) <= 0,
        CmpOp::Ge => su::compare(a, b) >= 0,
    }
}

/// `cString(&c, 1)`.
fn char_string(c: i8) -> String {
    String::from_utf8_lossy(&[c as u8]).into_owned()
}

impl FlexVar {
    /// `AsInt()`.
    ///
    /// For a double the C++ `(int)` cast is undefined outside the `int`
    /// range (and for NaN); Rust saturates (NaN gives 0).
    pub fn as_int(&self) -> i32 {
        match self {
            FlexVar::Int(v) => *v,
            FlexVar::Char(c) => *c as i32,
            FlexVar::Double(d) => *d as i32,
            FlexVar::Str(s) => su::as_int(s),
        }
    }

    /// `AsChar()`.
    pub fn as_char(&self) -> i8 {
        match self {
            FlexVar::Int(v) => *v as i8,
            FlexVar::Char(c) => *c,
            FlexVar::Double(d) => (*d as i32) as i8,
            // `m_value[0]`: the terminating NUL for an empty string.
            FlexVar::Str(s) => s.as_bytes().first().copied().unwrap_or(0) as i8,
        }
    }

    /// `AsDouble()`.
    pub fn as_double(&self) -> f64 {
        match self {
            FlexVar::Int(v) => *v as f64,
            FlexVar::Char(c) => *c as f64,
            FlexVar::Double(d) => *d,
            FlexVar::Str(s) => su::as_double(s),
        }
    }

    /// `AsBool()`.
    pub fn as_bool(&self) -> bool {
        match self {
            FlexVar::Int(v) => *v != 0,
            FlexVar::Char(c) => *c != 0,
            FlexVar::Double(d) => *d != 0.0,
            FlexVar::Str(s) => {
                let up = s.to_ascii_uppercase();
                if up == "TRUE" || up == "T" {
                    return true;
                }
                if up == "FALSE" || up == "F" {
                    return false;
                }
                su::as_int(&up) != 0
            }
        }
    }

    /// `AsString()`: ints as `%d`, chars as `%c`, doubles as `%f`.
    pub fn as_string(&self) -> String {
        match self {
            FlexVar::Int(v) => v.to_string(),
            FlexVar::Char(c) => char_string(*c),
            FlexVar::Double(d) => su::format_f(*d),
            FlexVar::Str(s) => s.clone(),
        }
    }

    /// `Print(ostream&)` / `operator<<`: ints in decimal, chars as the
    /// character, doubles with the default stream format (`%g`, 6 digits),
    /// strings as they are.
    pub fn print(&self) -> String {
        match self {
            FlexVar::Int(v) => v.to_string(),
            FlexVar::Char(c) => char_string(*c),
            FlexVar::Double(d) => fmt_double(*d),
            FlexVar::Str(s) => s.clone(),
        }
    }

    /// `operator OP (int)`.
    pub fn cmp_int(&self, rhs: i32, op: CmpOp) -> bool {
        match self {
            FlexVar::Int(a) => num_cmp(*a, rhs, op),
            FlexVar::Char(a) => num_cmp(*a as i32, rhs, op),
            FlexVar::Double(a) => num_cmp(*a, rhs as f64, op),
            FlexVar::Str(a) => num_cmp(su::as_double(a), rhs as f64, op),
        }
    }

    /// `operator OP (char)`.
    pub fn cmp_char(&self, rhs: i8, op: CmpOp) -> bool {
        match self {
            FlexVar::Int(a) => num_cmp(*a, rhs as i32, op),
            FlexVar::Char(a) => num_cmp(*a, rhs, op),
            FlexVar::Double(a) => num_cmp(*a, rhs as f64, op),
            FlexVar::Str(a) => str_cmp(a, &char_string(rhs), op),
        }
    }

    /// `operator OP (double)`.
    pub fn cmp_double(&self, rhs: f64, op: CmpOp) -> bool {
        match self {
            FlexVar::Int(a) => num_cmp(*a as f64, rhs, op),
            FlexVar::Char(a) => num_cmp(*a as f64, rhs, op),
            FlexVar::Double(a) => num_cmp(*a, rhs, op),
            FlexVar::Str(a) => num_cmp(su::as_double(a), rhs, op),
        }
    }

    /// `operator OP (const cString&)`.
    pub fn cmp_str(&self, rhs: &str, op: CmpOp) -> bool {
        match self {
            FlexVar::Int(a) => num_cmp(*a as f64, su::as_double(rhs), op),
            FlexVar::Char(a) => str_cmp(&char_string(*a), rhs, op),
            FlexVar::Double(a) => num_cmp(*a, su::as_double(rhs), op),
            FlexVar::Str(a) => str_cmp(a, rhs, op),
        }
    }

    /// `operator OP (const cFlexVar&)`: dispatch on the right-hand type.
    pub fn cmp(&self, rhs: &FlexVar, op: CmpOp) -> bool {
        match rhs {
            FlexVar::Int(b) => self.cmp_int(*b, op),
            FlexVar::Char(b) => self.cmp_char(*b, op),
            FlexVar::Double(b) => self.cmp_double(*b, op),
            FlexVar::Str(b) => self.cmp_str(b, op),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        assert_eq!(FlexVar::Double(2.0).as_string(), "2.000000");
        assert_eq!(FlexVar::Double(2.0).print(), "2");
        assert_eq!(FlexVar::Double(0.1234567).print(), "0.123457");
        assert_eq!(FlexVar::Char(b'3' as i8).as_int(), 51);
        assert_eq!(FlexVar::Str("four".into()).as_int(), 0);
        assert!(FlexVar::Str("t".into()).as_bool());
        assert!(!FlexVar::Str("F".into()).as_bool());
        assert!(FlexVar::Str("7".into()).as_bool());
    }

    #[test]
    fn cross_type_comparisons() {
        let s9 = FlexVar::Str("9".into());
        let c9 = FlexVar::Char(b'9' as i8);
        let i9 = FlexVar::Int(9);
        // String vs int compares numerically, string vs char textually.
        assert!(s9.cmp(&i9, CmpOp::Eq));
        assert!(s9.cmp(&c9, CmpOp::Eq));
        // Char vs int compares the character code.
        assert!(!c9.cmp(&i9, CmpOp::Eq));
        assert!(c9.cmp(&i9, CmpOp::Gt));
        // Strings order with cString::Compare.
        let a = FlexVar::Str("four".into());
        let b = FlexVar::Str("Testing".into());
        assert!(b.cmp(&a, CmpOp::Le));
    }
}
