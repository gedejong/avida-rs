//! `field_arith`: multi-bit arithmetic computed bit-parallel across fields of
//! the 32-bit word, with graded credit. A port addition, not in the C++
//! (PORTING.md, "Tasks the C++ does not have").
//!
//! The two newest inputs are the operands: `a = input[1]` (the older) and
//! `b = input[0]` (the newer). The word is cut into `n = 32 / stride`
//! whole fields from bit 0. The operand of a field is the low `bits` (w) bits
//! of the field; the other bits are masked away, so that the sum of two
//! operands never leaves its field. The expected result of a field is
//! `fn(a_field, b_field)` in `r` bits: `w + 1` for `add`, `halfadd`, `carry`
//! and `partial`, `w` for the logic functions. `carry` is `(a & b) << 1` and
//! `partial` is `(a ^ b) ^ ((a & b) << 1)`, the two steps of a ripple adder
//! before `add` (for two-bit operands `add` is `partial | ((a ^ b) & carry) << 1`). `stride` is the field width and defaults to `r`
//! (no padding), which makes `add` with `bits=2` three-bit fields; a larger
//! stride leaves the bits above the result unused (`stride=4`, `bits=2`:
//! `((a & 0x33333333) + (b & 0x33333333)) & 0x77777777`, eight adders).
//!
//! `width` (default 32) limits the fields to the low `width` bits of the
//! word: `n = width / stride` whole fields, the bits above them are not
//! compared. `width=24` leaves the top byte of the inputs, which the logic
//! tasks rely on, out of the arithmetic.
//!
//! `mode=packed` compares the newest output with the expected word on the
//! `r` result bits of every whole field. `mode=planes` takes the result as
//! `r` successive outputs, oldest first (least significant bit plane first):
//! output `i` holds bit `i` of every field's result at the field's bit 0
//! (bit `k * stride` of field `k`), and only those positions are compared.
//!
//! The quality is `max(0, 2f - 1)` for the fraction `f` of compared bits that
//! are right (chance earns nothing), and 0 below `thresh`.
//!
//! `expect=1` marks the entry as "expected" (PORTING.md, "Expected quality
//! per genotype"): when `EXPECTED_QUALITY_DRAWS` is above 0, the world pays
//! the genotype's average quality over several test-CPU runs in place of the
//! quality of the organism's own draw. The task itself computes the same
//! quality either way; the flag is only read through
//! [`FieldSpec::is_expected`].

use crate::arg_schema::{ArgContainer, ArgSchema, ArgType, Feedback};
use crate::io_buffer::IoBuffer;

/// The function a `field_arith` task checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFn {
    Xor,
    And,
    Or,
    /// One-bit add: the sum bit and the carry bit. `add` with `bits=1`.
    HalfAdd,
    Add,
    /// The first carry word of a ripple adder, `(a & b) << 1`, in `w + 1` bits.
    Carry,
    /// The sum after one carry round, `(a ^ b) ^ ((a & b) << 1)`, in `w + 1`
    /// bits: what `add` is before the second round's carry is merged in.
    Partial,
}

/// Parsed arguments of one `field_arith` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldSpec {
    func: FieldFn,
    /// Operand width w, 1 to 4.
    bits: u32,
    /// Result bits per field (r): w + 1 for the additions, else w.
    result_bits: u32,
    /// Field width, from `result_bits` to 32.
    stride: u32,
    planes: bool,
    thresh: f64,
    /// `expect=1`: the pay uses the genotype's expected quality.
    expect: bool,
    /// Operand bits of every whole field.
    operand_mask: u32,
    /// Result bits of every whole field.
    result_mask: u32,
    /// Bit 0 of every whole field.
    plane_mask: u32,
    /// Number of whole fields.
    fields: u32,
}

/// Schema of `field_arith`: `fn` and `mode` are strings, `bits` and `stride`
/// ints, `thresh` a double.
fn schema() -> ArgSchema {
    let mut s = ArgSchema::new();
    s.add_required("fn", 0, ArgType::Str);
    s.add_string("mode", 1, "packed");
    s.add_int("bits", 0, 1);
    s.add_int("stride", 1, 0);
    s.add_int("width", 2, 32);
    s.add_double("thresh", 0, 1.0);
    s.add_int("expect", 3, 0);
    s
}

impl FieldSpec {
    /// Parse the arguments of a `field_arith` line (`fn=add:bits=2:mode=planes`,
    /// or the same with commas); `None` after reporting through `feedback`.
    pub fn load(info: &str, feedback: &mut Feedback) -> Option<(FieldSpec, ArgContainer)> {
        let normalised = info.replace(':', ",");
        let before = feedback.num_errors();
        let args = ArgContainer::load(&normalised, &schema(), feedback)?;
        // `Load` reports an out-of-range value without failing.
        if feedback.num_errors() != before {
            return None;
        }
        let spec = Self::from_args(&args, feedback)?;
        Some((spec, args))
    }

    fn from_args(args: &ArgContainer, feedback: &mut Feedback) -> Option<FieldSpec> {
        let func = match args.get_string(0).to_ascii_lowercase().as_str() {
            "xor" => FieldFn::Xor,
            "and" => FieldFn::And,
            "or" => FieldFn::Or,
            "halfadd" => FieldFn::HalfAdd,
            "add" => FieldFn::Add,
            "carry" => FieldFn::Carry,
            "partial" => FieldFn::Partial,
            other => {
                feedback.error(format!(
                    "field_arith: unknown fn '{}' (xor, and, or, halfadd, add, carry, partial)",
                    other
                ));
                return None;
            }
        };
        let planes = match args.get_string(1).to_ascii_lowercase().as_str() {
            "packed" => false,
            "planes" => true,
            other => {
                feedback.error(format!(
                    "field_arith: unknown mode '{}' (packed, planes)",
                    other
                ));
                return None;
            }
        };
        let bits = args.get_int(0);
        if !(1..=4).contains(&bits) {
            feedback.error(format!("field_arith: bits={} is not in 1..4", bits));
            return None;
        }
        let bits = bits as u32;
        if func == FieldFn::HalfAdd && bits != 1 {
            feedback.error("field_arith: halfadd needs bits=1 (use fn=add for wider fields)");
            return None;
        }
        let result_bits = match func {
            FieldFn::HalfAdd | FieldFn::Add | FieldFn::Carry | FieldFn::Partial => bits + 1,
            _ => bits,
        };
        let stride = match args.get_int(1) {
            0 => result_bits,
            s if (result_bits as i32..=32).contains(&s) => s as u32,
            s => {
                feedback.error(format!(
                    "field_arith: stride={} is not in {}..32 (the result bits of a field)",
                    s, result_bits
                ));
                return None;
            }
        };
        let thresh = args.get_double(0);
        if !(0.0..=1.0).contains(&thresh) {
            feedback.error(format!("field_arith: thresh={} is not in 0..1", thresh));
            return None;
        }
        let width = args.get_int(2);
        if !(stride as i32..=32).contains(&width) {
            feedback.error(format!(
                "field_arith: width={} is not in {}..32 (the stride)",
                width, stride
            ));
            return None;
        }
        let expect = args.get_int(3);
        if !(0..=1).contains(&expect) {
            feedback.error(format!("field_arith: expect={} is not 0 or 1", expect));
            return None;
        }
        let fields = width as u32 / stride;
        let repeat = |unit: u32| (0..fields).fold(0u32, |m, k| m | (unit << (k * stride)));
        Some(FieldSpec {
            func,
            bits,
            result_bits,
            stride,
            planes,
            thresh,
            expect: expect == 1,
            operand_mask: repeat((1u32 << bits) - 1),
            result_mask: repeat(((1u64 << result_bits) - 1) as u32),
            plane_mask: repeat(1),
            fields,
        })
    }

    /// Whether the entry asked for the genotype's expected quality
    /// (`expect=1`).
    pub fn is_expected(&self) -> bool {
        self.expect
    }

    /// Operand width w.
    pub fn bits(&self) -> u32 {
        self.bits
    }

    /// Field width.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// Outputs the task reads: the result bits per field in `planes` mode, 1
    /// in `packed` mode. The environment makes the output buffer this large.
    pub fn outputs_needed(&self) -> i32 {
        if self.planes {
            self.result_bits as i32
        } else {
            1
        }
    }

    /// Bits compared.
    pub fn compared_bits(&self) -> u32 {
        self.fields * self.result_bits
    }

    /// The expected result word: field `k`'s result at bits
    /// `k * stride ..`, nothing outside the result bits of whole fields.
    pub fn expected(&self, a: u32, b: u32) -> u32 {
        let (a, b) = (a & self.operand_mask, b & self.operand_mask);
        match self.func {
            FieldFn::Xor => a ^ b,
            FieldFn::And => a & b,
            FieldFn::Or => a | b,
            // Each field sum is below 2^(w+1) <= 2^stride: no carry leaves a field.
            FieldFn::HalfAdd | FieldFn::Add => (a + b) & self.result_mask,
            FieldFn::Carry => ((a & b) << 1) & self.result_mask,
            FieldFn::Partial => ((a ^ b) ^ ((a & b) << 1)) & self.result_mask,
        }
    }

    /// The number of compared bits the outputs get right, `None` when the
    /// task cannot judge (fewer than two inputs, or in `planes` mode fewer
    /// outputs than planes).
    pub fn correct_bits(&self, input: &IoBuffer, output: &IoBuffer) -> Option<u32> {
        if input.num_stored() < 2 {
            return None;
        }
        let expected = self.expected(input.get(1) as u32, input.get(0) as u32);
        if !self.planes {
            if output.num_stored() < 1 {
                return None;
            }
            let diff = (output.get(0) as u32) ^ expected;
            return Some((!diff & self.result_mask).count_ones());
        }
        if output.num_stored() < self.result_bits as i32 {
            return None;
        }
        let mut right = 0;
        for plane in 0..self.result_bits {
            // Plane 0 is the oldest of the last `result_bits` outputs.
            let out = output.get((self.result_bits - 1 - plane) as i32) as u32;
            let diff = out ^ (expected >> plane);
            right += (!diff & self.plane_mask).count_ones();
        }
        Some(right)
    }

    /// The quality: `max(0, 2f - 1)` for the fraction `f` right, 0 below
    /// `thresh`.
    pub fn quality(&self, input: &IoBuffer, output: &IoBuffer) -> f64 {
        let Some(right) = self.correct_bits(input, output) else {
            return 0.0;
        };
        let total = self.compared_bits();
        let q = f64::from((2 * right).saturating_sub(total)) / f64::from(total);
        if q < self.thresh {
            0.0
        } else {
            q
        }
    }
}

/// Evaluator of `TaskKind::FieldArith`.
pub(crate) fn eval_field_arith(spec: &FieldSpec, input: &IoBuffer, output: &IoBuffer) -> f64 {
    spec.quality(input, output)
}
