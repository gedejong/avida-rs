//! Port of `cCodeLabel` (avida-core/source/cpu/cCodeLabel.{h,cc}).

/// A label: the sequence of nop modifiers following an instruction.
///
/// Nops are stored as C `char` (signed on the reference platform).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeLabel {
    nops: Vec<i8>,
}

impl CodeLabel {
    /// C++: `cCodeLabel::MAX_LENGTH`
    pub const MAX_LENGTH: usize = 10;

    pub fn new() -> Self {
        CodeLabel {
            nops: Vec::with_capacity(Self::MAX_LENGTH),
        }
    }

    /// C++: `cCodeLabel::ReadString` (input is expected to be trimmed text of
    /// letters `A`, `B`, ...). Not limited by `MAX_LENGTH`, as in C++.
    pub fn read_string(&mut self, label_str: &str) {
        let trimmed =
            label_str.trim_matches(|c: char| c == ' ' || c == '\t' || c == '\n' || c == '\r');
        self.nops = trimmed
            .bytes()
            .map(|b| (i32::from(b) - i32::from(b'A')) as i8)
            .collect();
    }

    /// C++: `cCodeLabel::AddNop`
    pub fn add_nop(&mut self, nop_num: i32) {
        if self.nops.len() < Self::MAX_LENGTH {
            self.nops.push(nop_num as i8);
        }
    }

    pub fn get_size(&self) -> i32 {
        self.nops.len() as i32
    }

    /// C++: `cCodeLabel::operator[]`
    pub fn get(&self, index: i32) -> i32 {
        i32::from(self.nops[index as usize])
    }

    pub fn clear(&mut self) {
        self.nops.clear();
    }

    /// C++: `cCodeLabel::Rotate`
    pub fn rotate(&mut self, rot: i32, base: i32) {
        for nop in self.nops.iter_mut() {
            // `m_nops[i] += rot` on a char, then compared after promotion.
            *nop = (i32::from(*nop) + rot) as i8;
            if i32::from(*nop) >= base {
                *nop = (i32::from(*nop) - base) as i8;
            }
        }
    }

    /// C++: `cCodeLabel::FindSublabel`. An empty sub-label is found at 0.
    pub fn find_sublabel(&self, sub: &CodeLabel) -> i32 {
        let size = self.nops.len() as i32;
        let sub_size = sub.nops.len() as i32;
        let mut offset = 0;
        while offset <= size - sub_size {
            let start = offset as usize;
            if self.nops[start..start + sub.nops.len()] == sub.nops[..] {
                return offset;
            }
            offset += 1;
        }
        -1
    }

    /// C++: `cCodeLabel::Contains`
    pub fn contains(&self, sub: &CodeLabel) -> bool {
        self.find_sublabel(sub) >= 0
    }

    /// C++: `cCodeLabel::AsString`
    pub fn as_string(&self) -> String {
        self.nops
            .iter()
            .map(|&n| char::from((i32::from(n) + i32::from(b'A')) as u8))
            .collect()
    }

    /// C++: `cCodeLabel::AsInt`
    pub fn as_int(&self, base: i32) -> i32 {
        let mut value: i32 = 0;
        for &nop in &self.nops {
            value = value.wrapping_mul(base).wrapping_add(i32::from(nop));
        }
        value
    }

    /// C++: `cCodeLabel::AsIntGreyCode`
    pub fn as_int_grey_code(&self, base: i32) -> i32 {
        let mut value: i32 = 0;
        let mut odd_count = 0;
        for &nop in &self.nops {
            let nop = i32::from(nop);
            value = value.wrapping_mul(base);
            if odd_count % 2 == 0 {
                value = value.wrapping_add(nop);
            } else {
                value = value.wrapping_add((base - 1) - nop);
            }
            if nop % 2 == 1 {
                odd_count += 1;
            }
        }
        value
    }

    /// C++: `cCodeLabel::AsIntDirect`
    pub fn as_int_direct(&self, base: i32) -> i32 {
        self.as_int(base)
    }

    /// C++: `cCodeLabel::AsIntUnique`
    pub fn as_int_unique(&self, base: i32) -> i32 {
        let mut value: i32 = 0;
        for &nop in &self.nops {
            value = value.wrapping_mul(base).wrapping_add(i32::from(nop) + 1);
        }
        value
    }

    /// C++: `cCodeLabel::AsIntAdditivePolynomial` (the `#if 1` branch: `a`,
    /// `b`, `c` are separate statements, so nothing is fused).
    pub fn as_int_additive_polynomial(&self, _base: i32) -> i32 {
        let size = self.nops.len();
        let mut value = 0.0_f64;
        for (i, &nop) in self.nops.iter().enumerate() {
            let n = i32::from(nop) + 1;
            let a = f64::from(n).powf(0.4 * (size as f64 - 1.0));
            let b = 0.3 * i as f64 * (size as f64 - 1.0);
            let c = 0.45 * i as f64;
            value += a + b + c;
        }
        (value + 0.5) as i32
    }

    /// C++: `cCodeLabel::AsIntFib`
    pub fn as_int_fib(&self, base: i32) -> i32 {
        let mut value: i32 = 0;
        if base < 3 {
            return 0;
        }
        let base = base as usize;
        let mut fib = vec![0_i32; base];
        fib[1] = 1;
        fib[2] = 2;
        for i in 3..base {
            fib[i] = fib[i - 2].wrapping_add(fib[i - 1]);
        }
        for &nop in &self.nops {
            value = value.wrapping_add(fib[nop as usize]);
            fib[2] = fib[base - 2].wrapping_add(fib[base - 1]);
            fib[1] = fib[base - 1];
            for j in 3..base {
                fib[j] = fib[j - 2].wrapping_add(fib[j - 1]);
            }
        }
        value
    }

    /// C++: `cCodeLabel::AsIntPolynomialCoefficent`
    pub fn as_int_polynomial_coefficent(&self, base: i32) -> i32 {
        let size = self.nops.len();
        let extra = size % 2;
        let mut value: i32 = 0;
        let mut c = 1;
        let mut i = 0;
        while i < size - extra {
            let b = i32::from(self.nops[i]);
            let a = i32::from(self.nops[i + 1]);
            value = value.wrapping_add(f64::from(a * base + b).powi(c) as i32);
            i += 2;
            c += 1;
        }
        if extra != 0 {
            value = value.wrapping_add(f64::from(self.nops[size - 1]).powi(c) as i32);
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(nops: &[i32]) -> CodeLabel {
        let mut l = CodeLabel::new();
        for &n in nops {
            l.add_nop(n);
        }
        l
    }

    #[test]
    fn rotate_and_as_int() {
        let mut l = label(&[0, 1, 2]);
        l.rotate(1, 3);
        assert_eq!(l, label(&[1, 2, 0]));
        assert_eq!(l.as_int(3), 15);
        assert_eq!(l.as_string(), "BCA");
    }

    #[test]
    fn as_int_examples_from_cpp_comments() {
        // nops A, B, C with base 3
        assert_eq!(label(&[]).as_int(3), 0);
        assert_eq!(label(&[0, 2]).as_int(3), 2);
        assert_eq!(label(&[1, 1]).as_int(3), 4);
        assert_eq!(label(&[2, 0]).as_int(3), 6);
        assert_eq!(label(&[0]).as_int_unique(3), 1);
        assert_eq!(label(&[0, 0]).as_int_unique(3), 4);
        assert_eq!(label(&[0, 2]).as_int_unique(3), 6);
        assert_eq!(label(&[1, 1]).as_int_unique(3), 8);
        // The C++ comment claims CA = 12; the code yields (2+1)*3 + (0+1) = 10.
        assert_eq!(label(&[2, 0]).as_int_unique(3), 10);
    }

    #[test]
    fn sublabel_search() {
        let l = label(&[0, 1, 2, 1, 2]);
        assert_eq!(l.find_sublabel(&label(&[1, 2])), 1);
        assert_eq!(l.find_sublabel(&label(&[2, 2])), -1);
        // QUIRK: the empty label is found at offset 0.
        assert_eq!(l.find_sublabel(&label(&[])), 0);
        assert_eq!(label(&[0]).find_sublabel(&l), -1);
    }

    #[test]
    fn length_is_capped_at_ten() {
        let mut l = CodeLabel::new();
        for _ in 0..30 {
            l.add_nop(1);
        }
        assert_eq!(l.get_size(), 10);
    }

    #[test]
    fn read_string_maps_letters() {
        let mut l = CodeLabel::new();
        l.read_string(" ACB ");
        assert_eq!(l, label(&[0, 2, 1]));
    }
}
