//! Port of `Avida::Instruction`
//! (avida-core/include/public/avida/core/InstructionSequence.h,
//! avida-core/source/core/InstructionSequence.cc).

use std::fmt;

/// `Avida::Instruction`: a single opcode stored as an `unsigned char`.
///
/// Opcode 255 is the error / "no instruction" value
/// (`cInstSet::GetInstError()`), printed as `_`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Instruction(u8);

impl Instruction {
    /// `cInstSet::GetInstError()`: `Instruction(255)`.
    pub const ERROR: Instruction = Instruction(255);

    /// `Instruction(int in_op)`. The C++ stores the value in an `unsigned
    /// char`, so out-of-range values wrap modulo 256 (e.g. -1 becomes 255).
    #[inline]
    pub const fn new(op: i32) -> Self {
        Instruction(op as u8)
    }

    /// `Instruction(const Apto::String& symbol)`.
    ///
    /// QUIRK: in C++ the operand stays uninitialised when `SetSymbol` fails;
    /// here it is 0.
    pub fn from_symbol(symbol: &str) -> Self {
        let mut inst = Instruction(0);
        inst.set_symbol(symbol);
        inst
    }

    /// `GetOp()`.
    #[inline]
    pub const fn op(self) -> i32 {
        self.0 as i32
    }

    /// `SetOp(int)` (wraps modulo 256 like the `unsigned char` store).
    #[inline]
    pub fn set_op(&mut self, op: i32) {
        self.0 = op as u8;
    }

    /// `Instruction::GetSymbol()`: `a`-`z`, `A`-`Z`, `0`-`9` for 0..61, then
    /// the same alphabet prefixed by `+`, `-`, `~`, `?` for each further
    /// block of 62; 255 is `_`.
    pub fn symbol(self) -> String {
        let mut out = String::with_capacity(2);
        self.push_symbol(&mut out);
        out
    }

    /// Appends [`Instruction::symbol`] to `out`.
    pub fn push_symbol(self, out: &mut String) {
        // aA0 +a+A+0 -a-A-0 ~a~A~0 ?a
        let op = self.0 as i32;
        if op == 255 {
            out.push('_');
            return;
        }
        let offset = op % 62;
        match op / 62 {
            1 => out.push('+'),
            2 => out.push('-'),
            3 => out.push('~'),
            4 => out.push('?'),
            _ => {}
        }
        let c = if offset < 26 {
            b'a' + offset as u8
        } else if offset < 52 {
            b'A' + (offset - 26) as u8
        } else {
            b'0' + (offset - 52) as u8
        };
        out.push(c as char);
    }

    /// `Instruction::SetSymbol(const Apto::String&)`. Returns false (leaving
    /// the opcode unchanged) for an unrecognised symbol.
    pub fn set_symbol(&mut self, symbol: &str) -> bool {
        self.set_symbol_bytes(symbol.as_bytes())
    }

    /// Byte-level `SetSymbol`. `InstructionSequence(const Apto::String&)`
    /// can hand it a two-byte symbol whose second byte is the string's NUL
    /// terminator; callers model that by passing a 0 byte.
    pub(crate) fn set_symbol_bytes(&mut self, symbol: &[u8]) -> bool {
        if symbol.is_empty() {
            return false;
        }
        let mut sym_char = symbol[0];
        let mut operand: i32 = 0;
        match sym_char {
            b'+' | b'-' | b'~' | b'?' => {
                if symbol.len() != 2 {
                    return false;
                }
                operand = match sym_char {
                    b'+' => 62,
                    b'-' => 124,
                    b'~' => 186,
                    _ => 248,
                };
                sym_char = symbol[1];
            }
            b'_' => {
                self.0 = 255;
                return true;
            }
            _ => {}
        }

        if sym_char.is_ascii_lowercase() {
            operand += (sym_char - b'a') as i32;
        } else if sym_char.is_ascii_uppercase() {
            operand += (sym_char - b'A') as i32 + 26;
        } else if sym_char.is_ascii_digit() {
            operand += (sym_char - b'0') as i32 + 52;
        } else {
            return false;
        }

        // QUIRK: `?i` .. `?9` give operands above 255, which wrap in the
        // `unsigned char` store (e.g. `?i` == 256 -> 0).
        self.0 = operand as u8;
        true
    }

    /// `AsString()` (same as `GetSymbol()`).
    pub fn as_string(self) -> String {
        self.symbol()
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Instruction({}:{})", self.0, self.symbol())
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.symbol())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_table() {
        assert_eq!(Instruction::new(0).symbol(), "a");
        assert_eq!(Instruction::new(25).symbol(), "z");
        assert_eq!(Instruction::new(26).symbol(), "A");
        assert_eq!(Instruction::new(51).symbol(), "Z");
        assert_eq!(Instruction::new(52).symbol(), "0");
        assert_eq!(Instruction::new(61).symbol(), "9");
        assert_eq!(Instruction::new(62).symbol(), "+a");
        assert_eq!(Instruction::new(124).symbol(), "-a");
        assert_eq!(Instruction::new(186).symbol(), "~a");
        assert_eq!(Instruction::new(248).symbol(), "?a");
        assert_eq!(Instruction::new(254).symbol(), "?g");
        assert_eq!(Instruction::new(255).symbol(), "_");
        assert_eq!(Instruction::new(-1), Instruction::ERROR);
    }

    #[test]
    fn symbol_round_trip() {
        for op in 0..=255 {
            let inst = Instruction::new(op);
            let mut back = Instruction::new(7);
            assert!(back.set_symbol(&inst.symbol()));
            assert_eq!(back, inst, "op {}", op);
        }
    }

    #[test]
    fn bad_symbols() {
        let mut i = Instruction::new(3);
        assert!(!i.set_symbol("+"));
        assert!(!i.set_symbol("*"));
        assert!(!i.set_symbol("+*"));
        assert_eq!(i.op(), 3);
        // The length is only checked for prefixed symbols: "Bx" reads 'B'.
        assert!(i.set_symbol("Bx"));
        assert_eq!(i.op(), 27);
        let mut j = Instruction::new(9);
        assert!(!j.set_symbol_bytes(b"+\0"));
        assert_eq!(j.op(), 9);
        // QUIRK wrap.
        assert!(j.set_symbol("?i"));
        assert_eq!(j.op(), 0);
        assert!(j.set_symbol("?z"));
        assert_eq!(j.op(), 17);
    }
}
