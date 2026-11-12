//! Arithmetic and math task evaluators.
//!
//! `Task_Add`, `Task_Add3`, `Task_Sub`, `Task_Math1in_*`, `Task_Math2in_*`,
//! `Task_Math3in_*`, `Task_Fib*`, `Task_Mult`, `Task_Div`, `Task_Log*`,
//! `Task_Sqrt`, `Task_Sine`, `Task_Cosine`, `Task_MatchNumber`
//! (avida-core/source/main/cTaskLib.cc).
//!
//! C `int` arithmetic is `i32` with wrapping; `abs(INT_MIN)` stays `INT_MIN`.
//! Where the C++ would divide by zero (SIGFPE) the candidate simply does not
//! match (marked `QUIRK:`).

use crate::arg_schema::ArgContainer;
use crate::io_buffer::IoBuffer;
use crate::registry_table::{Math1Op, Math2Op, Math3Op, UnaryOp};

/// `static const double dCastPrecision = 100000.0;`
const D_CAST_PRECISION: f64 = 100000.0;

/// `(int) sqrt((double) abs(x))`
fn isqrt_abs(x: i32) -> i32 {
    (f64::from(x.wrapping_abs())).sqrt() as i32
}

/// `Task_Add`: output equals the sum of two inputs `input[i] + input[j]`, `j < i`.
pub(crate) fn eval_add(input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    let n = input.num_stored();
    for i in 0..n {
        for j in 0..i {
            if test_output == input.get(i).wrapping_add(input.get(j)) {
                return 1.0;
            }
        }
    }
    0.0
}

/// Fewer right low bits than this earn nothing in `add_lowbits`. An arbitrary
/// output reaches 8 bits for one pair of inputs once in 256 times, the XOR of
/// the inputs (a sum without carries) about once in 8.
const ADD_LOWBITS_MIN: u32 = 8;

/// The most low bits in which the output agrees with `input[i] + input[j]`
/// before the first wrong bit, over the pairs `j < i`; `None` with fewer than
/// two inputs.
fn add_low_bits(input: &IoBuffer, output: &IoBuffer) -> Option<u32> {
    let test_output = output.get(0);
    let n = input.num_stored();
    let mut best = None;
    for i in 0..n {
        for j in 0..i {
            let sum = input.get(i).wrapping_add(input.get(j));
            let bits = (test_output ^ sum).trailing_zeros();
            best = Some(best.map_or(bits, |b: u32| b.max(bits)));
        }
    }
    best
}

/// Port addition, not in the C++: `add` with partial credit. The quality is
/// the share of the 32 bits, counted from the lowest, in which the output
/// agrees with the sum of two inputs before the first wrong bit. A correct
/// sum scores 1; fewer than [`ADD_LOWBITS_MIN`] right bits score 0, so that
/// chance agreement rarely pays. Each round of carry propagation makes more
/// low bits right, so a partial adder earns more than none.
pub(crate) fn eval_add_lowbits(input: &IoBuffer, output: &IoBuffer) -> f64 {
    match add_low_bits(input, output) {
        Some(bits) if bits >= ADD_LOWBITS_MIN => f64::from(bits) / 32.0,
        _ => 0.0,
    }
}

/// Port addition, not in the C++: the counterpart of `add_lowbits`. Scores 1
/// for an output that `add_lowbits` gives nothing, once two inputs have been
/// read. A reaction with a small negative value on it makes every wrong
/// output cost merit, so that putting out many values until one agrees by
/// chance does not pay.
pub(crate) fn eval_add_miss(input: &IoBuffer, output: &IoBuffer) -> f64 {
    match add_low_bits(input, output) {
        Some(bits) if bits < ADD_LOWBITS_MIN => 1.0,
        _ => 0.0,
    }
}

/// `Task_Add3`: output equals the sum of three consecutive inputs.
pub(crate) fn eval_add3(input: &IoBuffer, output: &IoBuffer) -> f64 {
    let out = output.get(0);
    let n = input.num_stored();
    let mut i = 0;
    while i < n - 2 {
        if out
            == input
                .get(i)
                .wrapping_add(input.get(i + 1))
                .wrapping_add(input.get(i + 2))
        {
            return 1.0;
        }
        i += 1;
    }
    0.0
}

/// `Task_Sub`: output equals `input[i] - input[j]`, `i != j`.
pub(crate) fn eval_sub(input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    let n = input.num_stored();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            if test_output == input.get(i).wrapping_sub(input.get(j)) {
                return 1.0;
            }
        }
    }
    0.0
}

/// `Task_Math1in_*`: does `output` equal the function of any single input?
pub(crate) fn eval_math1(op: Math1Op, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    let n = input.num_stored();
    for i in 0..n {
        let x = input.get(i);
        let hit = match op {
            Math1Op::AA => test_output == x.wrapping_mul(2),
            Math1Op::AB => test_output == x.wrapping_mul(2) / 3,
            Math1Op::AC => test_output == x.wrapping_mul(5) / 4,
            Math1Op::AD => test_output == x.wrapping_mul(x),
            Math1Op::AE => test_output == x.wrapping_mul(x).wrapping_mul(x),
            Math1Op::AF => test_output == isqrt_abs(x),
            Math1Op::AG => x > 0 && test_output == (f64::from(x)).ln() as i32,
            Math1Op::AH => {
                test_output
                    == x.wrapping_mul(x)
                        .wrapping_add(x.wrapping_mul(x).wrapping_mul(x))
            }
            Math1Op::AI => test_output == x.wrapping_mul(x).wrapping_add(isqrt_abs(x)),
            Math1Op::AJ => test_output == x.wrapping_abs(),
            Math1Op::AK => test_output == x.wrapping_sub(5),
            Math1Op::AL => test_output == 0i32.wrapping_sub(x),
            Math1Op::AM => test_output == x.wrapping_mul(5),
            Math1Op::AN => test_output == x / 4,
            Math1Op::AO => test_output == x.wrapping_sub(6),
            Math1Op::AP => test_output == x.wrapping_sub(7),
            Math1Op::AS => test_output == x.wrapping_mul(3),
        };
        if hit {
            return 1.0;
        }
    }
    0.0
}

/// One candidate pair for the two-input functions; `false` also covers the
/// C++ `continue` guards.
fn math2_hit(op: Math2Op, x: i32, y: i32, test_output: i32) -> bool {
    // `0 - INT_MAX > x && y == -1`: the INT_MIN / -1 overflow guard.
    let min_over_neg1 = 0i32.wrapping_sub(i32::MAX) > x;
    match op {
        Math2Op::AA => test_output == isqrt_abs(x.wrapping_add(y)),
        Math2Op::AB => {
            let s = x.wrapping_add(y);
            test_output == s.wrapping_mul(s)
        }
        Math2Op::AC => y != 0 && test_output == x.wrapping_rem(y),
        Math2Op::AD => test_output == (x.wrapping_mul(3) / 2).wrapping_add(y.wrapping_mul(5) / 4),
        Math2Op::AE => {
            test_output
                == x.wrapping_sub(5)
                    .wrapping_abs()
                    .wrapping_add(y.wrapping_sub(6).wrapping_abs())
        }
        Math2Op::AF => {
            if y == 0 || (min_over_neg1 && y == -1) {
                return false;
            }
            test_output == x.wrapping_mul(y).wrapping_sub(x.wrapping_div(y))
        }
        Math2Op::AG => {
            let d = x.wrapping_sub(y);
            test_output == d.wrapping_mul(d)
        }
        Math2Op::AH => test_output == x.wrapping_mul(x).wrapping_add(y.wrapping_mul(y)),
        Math2Op::AI => {
            test_output
                == x.wrapping_mul(x)
                    .wrapping_add(y.wrapping_mul(y).wrapping_mul(y))
        }
        Math2Op::AJ => {
            let denom = x.wrapping_sub(7);
            if denom == 0 {
                return false;
            }
            test_output == isqrt_abs(x).wrapping_add(y).wrapping_div(denom)
        }
        Math2Op::AK => {
            if y == 0 || (min_over_neg1 && y == -1) {
                return false;
            }
            let quot = x.wrapping_div(y);
            if quot == 0 {
                return false;
            }
            test_output == f64::from(quot.wrapping_abs()).ln() as i32
        }
        Math2Op::AL => {
            if y == 0 {
                return false;
            }
            test_output == (f64::from(x.wrapping_abs()).ln() as i32).wrapping_div(y)
        }
        Math2Op::AM => {
            let log_y = f64::from(y.wrapping_abs()).ln();
            if log_y == 0.0 || (min_over_neg1 && log_y == -1.0) {
                return false;
            }
            let divisor = log_y as i32;
            // QUIRK: (int) log(|y|) is 0 for |y| == 2; the C++ then divides by
            // zero (SIGFPE). Treated as no match here.
            if divisor == 0 {
                return false;
            }
            test_output == x.wrapping_div(divisor)
        }
        Math2Op::AN => test_output == x.wrapping_add(y),
        Math2Op::AO => test_output == x.wrapping_sub(y),
        Math2Op::AP => {
            if y == 0 || (min_over_neg1 && y == -1) {
                return false;
            }
            test_output == x.wrapping_div(y)
        }
        Math2Op::AQ => test_output == x.wrapping_mul(y),
        Math2Op::AR => test_output == isqrt_abs(x).wrapping_add(isqrt_abs(y)),
        Math2Op::AS => test_output == x.wrapping_add(y.wrapping_mul(2)),
        Math2Op::AT => test_output == x.wrapping_add(y.wrapping_mul(3)),
        Math2Op::AU => test_output == x.wrapping_mul(2).wrapping_add(y.wrapping_mul(3)),
        Math2Op::AV => test_output == x.wrapping_mul(y).wrapping_mul(y),
        // QUIRK: math_2AX duplicates math_2AT (X+3Y) in the C++.
        Math2Op::AX => test_output == x.wrapping_add(y.wrapping_mul(3)),
        Math2Op::AY => test_output == x.wrapping_mul(2).wrapping_add(y),
        Math2Op::AZ => test_output == x.wrapping_mul(4).wrapping_add(y.wrapping_mul(6)),
        Math2Op::AAA => test_output == x.wrapping_mul(3).wrapping_sub(y.wrapping_mul(2)),
    }
}

/// `Task_Math2in_*`: does `output` equal the function of any ordered pair of
/// distinct input slots?
pub(crate) fn eval_math2(op: Math2Op, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    let n = input.num_stored();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            if math2_hit(op, input.get(i), input.get(j), test_output) {
                return 1.0;
            }
        }
    }
    0.0
}

fn math3_hit(op: Math3Op, x: i32, y: i32, z: i32, test_output: i32) -> bool {
    match op {
        Math3Op::AA => {
            test_output
                == x.wrapping_mul(x)
                    .wrapping_add(y.wrapping_mul(y))
                    .wrapping_add(z.wrapping_mul(z))
        }
        Math3Op::AB => {
            test_output
                == isqrt_abs(x)
                    .wrapping_add(isqrt_abs(y))
                    .wrapping_add(isqrt_abs(z))
        }
        Math3Op::AC => {
            test_output
                == x.wrapping_add(y.wrapping_mul(2))
                    .wrapping_add(z.wrapping_mul(3))
        }
        Math3Op::AD => {
            test_output
                == x.wrapping_mul(y)
                    .wrapping_mul(y)
                    .wrapping_add(z.wrapping_mul(z).wrapping_mul(z))
        }
        Math3Op::AE => y != 0 && test_output == x.wrapping_rem(y).wrapping_mul(z),
        Math3Op::AF => {
            let xy = x.wrapping_add(y);
            test_output
                == xy
                    .wrapping_mul(xy)
                    .wrapping_add(isqrt_abs(y.wrapping_add(z)))
        }
        Math3Op::AG => {
            let mod_base = y.wrapping_mul(z);
            mod_base != 0 && test_output == x.wrapping_mul(y).wrapping_rem(mod_base)
        }
        Math3Op::AH => test_output == x.wrapping_add(y).wrapping_add(z),
        Math3Op::AI => test_output == 0i32.wrapping_sub(x).wrapping_sub(y).wrapping_sub(z),
        Math3Op::AJ => {
            let xy = x.wrapping_sub(y);
            let yz = y.wrapping_sub(z);
            let zx = z.wrapping_sub(x);
            test_output
                == xy
                    .wrapping_mul(xy)
                    .wrapping_add(yz.wrapping_mul(yz))
                    .wrapping_add(zx.wrapping_mul(zx))
        }
        Math3Op::AK => {
            let xy = x.wrapping_add(y);
            let yz = y.wrapping_add(z);
            let zx = z.wrapping_add(x);
            test_output
                == xy
                    .wrapping_mul(xy)
                    .wrapping_add(yz.wrapping_mul(yz))
                    .wrapping_add(zx.wrapping_mul(zx))
        }
        Math3Op::AL => {
            let xy = x.wrapping_sub(y);
            let xz = x.wrapping_sub(z);
            test_output == xy.wrapping_mul(xy).wrapping_add(xz.wrapping_mul(xz))
        }
        // QUIRK: the description says (X+Y)^2+(Y+Z)^2 but the code computes
        // (X+Y)^2+(X+Z)^2.
        Math3Op::AM => {
            let xy = x.wrapping_add(y);
            let xz = x.wrapping_add(z);
            test_output == xy.wrapping_mul(xy).wrapping_add(xz.wrapping_mul(xz))
        }
    }
}

/// `Task_Math3in_*`: does `output` equal the function of any ordered triple of
/// distinct input slots?
pub(crate) fn eval_math3(op: Math3Op, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    let n = input.num_stored();
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                if i == j || j == k || i == k {
                    continue;
                }
                if math3_hit(op, input.get(i), input.get(j), input.get(k), test_output) {
                    return 1.0;
                }
            }
        }
    }
    0.0
}

/// `Task_Fib1` .. `Task_Fib10` (1-based `fib_index`; there is no 3).
///
/// QUIRK: the C++ wraps the comparison in `for (i < input_size)`, so the task
/// can only fire when at least one input is stored.
pub(crate) fn eval_fib(fib_index: u8, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let target = match fib_index {
        1 => 0,
        2 => 1,
        4 => 2,
        5 => 3,
        6 => 5,
        7 => 8,
        8 => 13,
        9 => 21,
        10 => 34,
        _ => return 0.0,
    };
    let test_output = output.get(0);
    if input.num_stored() > 0 && test_output == target {
        1.0
    } else {
        0.0
    }
}

/// The shared tail of the diff-based tasks (`Match Number`, `Mult`, `Div`, `Log*`,
/// ...): negative threshold means infinite; inside the threshold quality decays
/// by `2^(diff / -|halflife|)`.
pub(crate) fn threshold_halflife_quality(diff: i64, threshold: i32, halflife_arg: f64) -> f64 {
    let mut quality = 0.0;
    if threshold < 0 || diff <= i64::from(threshold) {
        let halflife = -1.0 * halflife_arg.abs();
        quality = 2.0f64.powf(diff as f64 / halflife);
    }
    quality
}

/// `((long long)INT_MAX + 1) * 2`: initial minimum for the diff scans.
const DIFF_SCAN_INIT: i64 = (i32::MAX as i64 + 1) * 2;

/// `Task_Mult`.
pub(crate) fn eval_mult(args: &ArgContainer, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = i64::from(output.get(0));
    let n = input.num_stored();
    let mut diff = DIFF_SCAN_INIT;
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            // `(long long)(input_buffer[i] * input_buffer[j])`: the product is
            // computed in int first.
            let prod = i64::from(input.get(i).wrapping_mul(input.get(j)));
            let cur = (prod - test_output).abs();
            if cur < diff {
                diff = cur;
            }
        }
    }
    threshold_halflife_quality(diff, args.get_int(0), args.get_double(0))
}

/// `Task_Div`.
pub(crate) fn eval_div(args: &ArgContainer, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = i64::from(output.get(0));
    let n = input.num_stored();
    let mut diff = DIFF_SCAN_INIT;
    for i in 0..n {
        for j in 0..n {
            if i == j || input.get(j) == 0 {
                continue;
            }
            // `(long long)(input_buffer[i] / input_buffer[j])` (int division;
            // INT_MIN / -1 overflows in C++, wraps here).
            let quot = i64::from(input.get(i).wrapping_div(input.get(j)));
            let cur = (quot - test_output).abs();
            if cur < diff {
                diff = cur;
            }
        }
    }
    threshold_halflife_quality(diff, args.get_int(0), args.get_double(0))
}

/// `Task_Log`, `Task_Log2`, `Task_Log10`, `Task_Sqrt`, `Task_Sine`, `Task_Cosine`.
pub(crate) fn eval_unary(
    op: UnaryOp,
    args: &ArgContainer,
    input: &IoBuffer,
    output: &IoBuffer,
) -> f64 {
    let test_output = i64::from(output.get(0));
    let n = input.num_stored();
    let mut diff = DIFF_SCAN_INIT;
    for i in 0..n {
        let v = input.get(i);
        let x = f64::from(v);
        let transformed: f64 = match op {
            UnaryOp::Log => f64::from(if v != 0 { v } else { 1 }).abs().ln(),
            UnaryOp::Log2 => f64::from(if v != 0 { v } else { 1 }).abs().log2(),
            UnaryOp::Log10 => f64::from(if v != 0 { v } else { 1 }).abs().log10(),
            UnaryOp::Sqrt => x.abs().sqrt(),
            UnaryOp::Sine => (x / D_CAST_PRECISION).sin() * D_CAST_PRECISION,
            UnaryOp::Cosine => (x / D_CAST_PRECISION).cos() * D_CAST_PRECISION,
        };
        // `(long long)(...)`: truncation toward zero.
        let cur = ((transformed as i64) - test_output).abs();
        if cur < diff {
            diff = cur;
        }
    }
    threshold_halflife_quality(diff, args.get_int(0), args.get_double(0))
}

/// `Task_MatchNumber`.
pub(crate) fn eval_match_number(args: &ArgContainer, output: &IoBuffer) -> f64 {
    let diff = (i64::from(args.get_int(0)) - i64::from(output.get(0))).abs();
    threshold_halflife_quality(diff, args.get_int(1), args.get_double(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Buffer holding `vals` so that `get(0)` is the LAST value listed.
    fn buf(vals: &[i32]) -> IoBuffer {
        let mut b = IoBuffer::new(vals.len().max(3));
        for &v in vals {
            b.add(v);
        }
        b
    }

    fn out(v: i32) -> IoBuffer {
        buf(&[v])
    }

    #[test]
    fn add_lowbits_counts_the_right_low_bits() {
        let i = buf(&[3, 5, 7]);
        assert_eq!(eval_add_lowbits(&i, &out(12)), 1.0); // 7+5
        assert_eq!(eval_add_lowbits(&i, &out(12 ^ (1 << 20))), 20.0 / 32.0);
        // 9 is odd, the sums 12, 10 and 8 are even: the lowest bit is wrong.
        assert_eq!(eval_add_lowbits(&i, &out(9)), 0.0);
        // 2 agrees with 10 (7+3) in three low bits only: below the minimum.
        assert_eq!(eval_add_lowbits(&i, &out(2)), 0.0);
        assert_eq!(eval_add_lowbits(&i, &out(12 ^ (1 << 7))), 0.0);
        assert_eq!(eval_add_lowbits(&i, &out(12 ^ (1 << 8))), 8.0 / 32.0);
        assert_eq!(eval_add_miss(&i, &out(2)), 1.0);
        assert_eq!(eval_add_miss(&i, &out(12)), 0.0);
        assert_eq!(eval_add_miss(&i, &out(12 ^ (1 << 8))), 0.0);
        // With one input there is no sum to miss.
        assert_eq!(eval_add_miss(&buf(&[4]), &out(9)), 0.0);
        // One input is no pair.
        assert_eq!(eval_add_lowbits(&buf(&[4]), &out(8)), 0.0);
    }

    #[test]
    fn add_add3_sub() {
        // inputs: get(0)=7, get(1)=5, get(2)=3
        let i = buf(&[3, 5, 7]);
        assert_eq!(eval_add(&i, &out(12)), 1.0); // 7+5
        assert_eq!(eval_add(&i, &out(10)), 1.0); // 7+3
        assert_eq!(eval_add(&i, &out(9)), 0.0);
        assert_eq!(eval_add3(&i, &out(15)), 1.0);
        assert_eq!(eval_add3(&i, &out(14)), 0.0);
        assert_eq!(eval_sub(&i, &out(2)), 1.0); // 7-5
        assert_eq!(eval_sub(&i, &out(-4)), 1.0); // 3-7
        assert_eq!(eval_sub(&i, &out(0)), 0.0); // i != j
                                                // add3 needs three inputs
        assert_eq!(eval_add3(&buf(&[1, 2]), &out(3)), 0.0);
    }

    #[test]
    fn math1_samples() {
        let i = buf(&[3, 5, 16]); // 16, 5, 3
        assert_eq!(eval_math1(Math1Op::AA, &i, &out(32)), 1.0);
        assert_eq!(eval_math1(Math1Op::AA, &i, &out(33)), 0.0);
        assert_eq!(eval_math1(Math1Op::AB, &i, &out(10)), 1.0); // 2*16/3 = 10
        assert_eq!(eval_math1(Math1Op::AD, &i, &out(25)), 1.0);
        assert_eq!(eval_math1(Math1Op::AF, &i, &out(4)), 1.0);
        assert_eq!(eval_math1(Math1Op::AG, &i, &out(2)), 1.0); // ln 16 = 2.77 -> 2
        assert_eq!(eval_math1(Math1Op::AK, &i, &out(11)), 1.0);
        assert_eq!(eval_math1(Math1Op::AL, &i, &out(-3)), 1.0);
        assert_eq!(eval_math1(Math1Op::AN, &i, &out(4)), 1.0);
        assert_eq!(eval_math1(Math1Op::AS, &i, &out(15)), 1.0);
        // log of non-positive input never matches
        assert_eq!(eval_math1(Math1Op::AG, &buf(&[-5, 0]), &out(0)), 0.0);
        // wrapping: 2*INT_MAX wraps to -2
        assert_eq!(eval_math1(Math1Op::AA, &buf(&[i32::MAX]), &out(-2)), 1.0);
    }

    #[test]
    fn math2_samples() {
        let i = buf(&[3, 5, 16]); // get: 16, 5, 3
        assert_eq!(eval_math2(Math2Op::AN, &i, &out(21)), 1.0); // 16+5
        assert_eq!(eval_math2(Math2Op::AB, &i, &out(64)), 1.0); // (3+5)^2
        assert_eq!(eval_math2(Math2Op::AC, &i, &out(1)), 1.0); // 16%5
        assert_eq!(eval_math2(Math2Op::AP, &i, &out(3)), 1.0); // 16/5
        assert_eq!(eval_math2(Math2Op::AQ, &i, &out(80)), 1.0);
        assert_eq!(eval_math2(Math2Op::AAA, &i, &out(38)), 1.0); // 3*16-2*5
        assert_eq!(eval_math2(Math2Op::AX, &i, &out(31)), 1.0); // 16+3*5
                                                                // division by zero guard
        assert_eq!(eval_math2(Math2Op::AP, &buf(&[0, 7]), &out(7)), 0.0);
        // AM with |y| == 2 would be SIGFPE in C++; here no match
        assert_eq!(eval_math2(Math2Op::AM, &buf(&[2, 9]), &out(0)), 0.0);
        // AJ: x == 7 is skipped
        assert_eq!(eval_math2(Math2Op::AJ, &buf(&[1, 7]), &out(0)), 0.0);
    }

    #[test]
    fn math3_samples() {
        let i = buf(&[2, 3, 4]); // 4, 3, 2
        assert_eq!(eval_math3(Math3Op::AH, &i, &out(9)), 1.0);
        assert_eq!(eval_math3(Math3Op::AA, &i, &out(29)), 1.0);
        assert_eq!(eval_math3(Math3Op::AI, &i, &out(-9)), 1.0);
        assert_eq!(eval_math3(Math3Op::AC, &i, &out(4 + 2 * 3 + 3 * 2)), 1.0);
        // AM computes (X+Y)^2+(X+Z)^2: (4+3)^2+(4+2)^2 = 85
        assert_eq!(eval_math3(Math3Op::AM, &i, &out(85)), 1.0);
        // fewer than three distinct slots never matches
        assert_eq!(eval_math3(Math3Op::AH, &buf(&[1, 2]), &out(3)), 0.0);
    }

    #[test]
    fn fib_needs_an_input() {
        let i = buf(&[1]);
        assert_eq!(eval_fib(1, &i, &out(0)), 1.0);
        assert_eq!(eval_fib(2, &i, &out(1)), 1.0);
        assert_eq!(eval_fib(4, &i, &out(2)), 1.0);
        assert_eq!(eval_fib(10, &i, &out(34)), 1.0);
        assert_eq!(eval_fib(10, &i, &out(33)), 0.0);
        assert_eq!(eval_fib(3, &i, &out(1)), 0.0);
        // QUIRK: no inputs, no reward
        assert_eq!(eval_fib(1, &IoBuffer::new(3), &out(0)), 0.0);
    }

    fn args(threshold: i32, halflife: f64) -> ArgContainer {
        let mut a = ArgContainer::default();
        a.set_int(0, threshold);
        a.set_double(0, halflife);
        a
    }

    #[test]
    fn mult_div_quality_decay() {
        // inputs 6 and 7: product 42
        let i = buf(&[7, 6]);
        let a = args(-1, 2.0);
        assert_eq!(eval_mult(&a, &i, &out(42)), 1.0);
        // diff 2 with halflife 2 -> 2^(-1) = 0.5
        assert_eq!(eval_mult(&a, &i, &out(44)), 0.5);
        // threshold 1 cuts diff 2 off
        assert_eq!(eval_mult(&args(1, 2.0), &i, &out(44)), 0.0);
        assert_eq!(eval_mult(&args(2, 2.0), &i, &out(44)), 0.5);
        // div: 7/6 = 1, 6/7 = 0
        assert_eq!(eval_div(&a, &i, &out(1)), 1.0);
        // zero divisors skipped, nothing scanned -> diff stays 2^32
        let z = buf(&[0, 0]);
        assert!(eval_div(&a, &z, &out(0)) < 1e-100);
        // int product wraps before widening: 65536*65536 = 0 in C int
        let w = buf(&[65536, 65536]);
        assert_eq!(eval_mult(&a, &w, &out(0)), 1.0);
    }

    #[test]
    fn unary_tasks() {
        let a = args(-1, 1.0);
        // log of 1000 = 6.9 -> 6
        assert_eq!(eval_unary(UnaryOp::Log, &a, &buf(&[1000]), &out(6)), 1.0);
        // zero input is treated as 1: log 1 = 0
        assert_eq!(eval_unary(UnaryOp::Log, &a, &buf(&[0]), &out(0)), 1.0);
        assert_eq!(eval_unary(UnaryOp::Log2, &a, &buf(&[1024]), &out(10)), 1.0);
        assert_eq!(eval_unary(UnaryOp::Log10, &a, &buf(&[1000]), &out(3)), 1.0);
        assert_eq!(eval_unary(UnaryOp::Sqrt, &a, &buf(&[-49]), &out(7)), 1.0);
        // sin(0) = 0
        assert_eq!(eval_unary(UnaryOp::Sine, &a, &buf(&[0]), &out(0)), 1.0);
        // cos(0)*1e5 = 100000
        assert_eq!(
            eval_unary(UnaryOp::Cosine, &a, &buf(&[0]), &out(100000)),
            1.0
        );
    }

    #[test]
    fn match_number() {
        let mut a = ArgContainer::default();
        a.set_int(0, 100);
        a.set_int(1, -1);
        a.set_double(0, 4.0);
        assert_eq!(eval_match_number(&a, &out(100)), 1.0);
        assert_eq!(eval_match_number(&a, &out(104)), 0.5);
        assert_eq!(eval_match_number(&a, &out(96)), 0.5);
        a.set_int(1, 3);
        assert_eq!(eval_match_number(&a, &out(104)), 0.0);
        // negative halflife is folded to its absolute value
        a.set_int(1, -1);
        a.set_double(0, -4.0);
        assert_eq!(eval_match_number(&a, &out(104)), 0.5);
    }
}
