//! Task evaluators that need nothing but the context buffers and the task
//! arguments: `Task_Echo`, `Task_DontCare`, `Task_MatchStr`,
//! `Task_FibonacciSequence`, `Task_Optimize`, `Task_CommEcho`, `Task_CommNot`,
//! `Task_AllOnes`, `Task_RoyalRoad`, `Task_RoyalRoadWithDitches`
//! (avida-core/source/main/cTaskLib.cc).

use crate::arg_schema::ArgContainer;
use crate::context::{FibSeqState, TaskContext, TaskState};
use crate::io_buffer::IoBuffer;

/// `Task_Echo`: the output equals one of the stored inputs.
pub(crate) fn eval_echo(input: &IoBuffer, output: &IoBuffer) -> f64 {
    let test_output = output.get(0);
    for i in 0..input.num_stored() {
        if input.get(i) == test_output {
            return 1.0;
        }
    }
    0.0
}

/// `Task_DontCare` (`@WRE`): always succeeds.
pub(crate) fn eval_dont_care() -> f64 {
    1.0
}

/// Number of pattern positions whose bit in `value` matches the pattern read
/// right to left (`'0'` matches a clear bit, `'1'` a set bit); the inner loop
/// shared by `Task_MatchStr` for the output value and each received message.
fn bits_matched(pattern: &[u8], value: i32) -> i32 {
    let len = pattern.len();
    let mut num_matched = 0;
    for j in 0..len {
        let string_index = len - j - 1; // start with last char in string
                                        // `1 << j`: shift counts of 32 and more are undefined in C++; x86
                                        // masks the count, as `wrapping_shl` does.
        let k = 1i32.wrapping_shl(j as u32);
        if (pattern[string_index] == b'0' && (value & k) == 0)
            || (pattern[string_index] == b'1' && (value & k) != 0)
        {
            num_matched += 1;
        }
    }
    num_matched
}

/// `Task_MatchStr`, as the reference binary runs it: the C++ flattens the
/// first `GetNumStored()` outputs (and the received messages) and hands
/// them to `avd_task_eval_match_str` (rust/avida-rust/src/task_lib_helpers.rs).
///
/// Args: string 0 = pattern, int 0 = `partial`, int 1 = `binary`.
///
/// Where this differs from the pre-FFI C++ (cTaskLib.cc at 47f13dadb), the
/// reference is followed:
/// - binary mode compares only the stored outputs: a pattern position at or
///   past `GetNumStored()` never matches (the old C++ read the circular
///   buffer there, which holds stale or never-written slots);
/// - received messages are checked in bit mode only;
/// - `partial` divides by the number of non-`9` positions only when there
///   are some (always none in bit mode, where the old C++ divided by zero
///   and an infinite bonus followed); otherwise by the pattern length;
/// - an empty pattern scores 0.
pub(crate) fn eval_match_str(
    args: &ArgContainer,
    output: &IoBuffer,
    received: Option<&IoBuffer>,
) -> f64 {
    let pattern = args.get_string(0).as_bytes();
    if pattern.is_empty() {
        return 0.0;
    }
    let partial = args.get_int(0);
    let binary = args.get_int(1);
    let out_len = output.num_stored();
    let mut max_num_matched = 0;
    let mut num_real = 0;

    if binary == 0 {
        // Bitwise comparison of the newest output value.
        if out_len > 0 {
            max_num_matched = bits_matched(pattern, output.get(0));
        }
        // A received message may match better.
        if let Some(recv) = received {
            for i in 0..recv.num_stored() {
                max_num_matched = max_num_matched.max(bits_matched(pattern, recv.get(i)));
            }
        }
    } else {
        // Element-wise comparison of the stored outputs ('9' is a wildcard).
        for (j, &p) in pattern.iter().enumerate() {
            if p != b'9' {
                num_real += 1;
            }
            let j = j as i32;
            if j < out_len {
                let o = output.get(j);
                if (p == b'0' && o == 0) || (p == b'1' && o == 1) {
                    max_num_matched += 1;
                }
            }
        }
    }

    // return value between 0 & 1 representing the percentage of string that was matched
    let base_bonus = if partial != 0 && num_real > 0 {
        f64::from(max_num_matched) * 2.0 / f64::from(num_real) - 1.0
    } else {
        f64::from(max_num_matched) * 2.0 / pattern.len() as f64 - 1.0
    };
    if base_bonus > 0.0 {
        base_bonus * base_bonus // pow(base_bonus, 2)
    } else {
        0.0
    }
}

/// `Task_FibonacciSequence`: stateful; `None` states storage (a null
/// `m_task_states` dereference in C++) yields 0.0.
pub(crate) fn eval_fibonacci_sequence(ctx: &mut TaskContext, args: &ArgContainer) -> f64 {
    let Some(id) = ctx.task_entry else {
        return 0.0;
    };
    let test_output = ctx.output_buffer.get(0);
    let Some(states) = ctx.task_states.as_deref_mut() else {
        return 0.0;
    };
    let state = states
        .entry(id)
        .or_insert_with(|| TaskState::FibSeq(FibSeqState::default()));
    let TaskState::FibSeq(state) = state;

    let next = state.seq[0].wrapping_add(state.seq[1]);

    // If output matches next in sequence
    if test_output == next {
        // Increment count and store next value
        state.count = state.count.wrapping_add(1);
        state.seq[(state.count % 2).unsigned_abs() as usize] = next;

        // If past target sequence ending point, return the penalty setting
        if state.count > args.get_int(0) {
            return args.get_double(0);
        }
        return 1.0;
    }
    0.0
}

/// Result of `Task_Optimize`: the quality returned to the caller and the
/// function value `Fx` stored through `SetTaskValue` (`None` when the early
/// "buffer not full yet" return skipped it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OptimizeResult {
    pub quality: f64,
    pub fx: Option<f64>,
}

/// `Task_Optimize`.
///
/// Args: int 0 = function, 1 = binary, 2 = varlength, 3 = numvars; double
/// 0 = basepow, 1 = maxFx, 2 = minFx, 3 = thresh, 4 = threshMax; string 0 =
/// target for function 20.
///
/// QUIRK: an unknown function id sets `quality = .001` in the C++ `default:`
/// case, but the threshold section below overwrites it, so the result is the
/// quality for `Fx == 0`.
///
/// The C++ prints `quality > 1!` to stdout when the quality exceeds 1; that
/// diagnostic is not reproduced (callers can test the returned value).
pub(crate) fn eval_optimize(args: &ArgContainer, output: &IoBuffer) -> OptimizeResult {
    // if the org hasn't output yet enough numbers, just return without completing any tasks
    if output.num_stored() < output.capacity() {
        return OptimizeResult {
            quality: 0.0,
            fx: None,
        };
    }

    let function = args.get_int(0);
    let binary = args.get_int(1);
    let varlength = args.get_int(2);
    let numvars = args.get_int(3);
    let nv = numvars.max(0);

    // get however many variables need, turn them into doubles between 0 and 1.
    // Padded to two entries so that functions that read vars[1] stay defined
    // when numvars < 2 (undefined in C++).
    let mut vars = vec![0.0f64; nv.max(2) as usize];

    let mut fx = 0.0f64;

    // some of the problems don't need double variables but use the bit string as a bit string
    // string match, Fx=length - num_matched (0 best, length worst)
    if function == 20 {
        let string_to_match = args.get_string(0).as_bytes();
        let mut matched = 0;
        for i in 0..varlength {
            let ch = string_to_match.get(i as usize).copied().unwrap_or(0);
            let o = output.get(i);
            if (ch == b'0' && o == 0) || (ch == b'1' && o == 1) {
                matched += 1;
            }
        }
        fx = f64::from(varlength.wrapping_sub(matched));
    }
    // string with all 1's at beginning until pattern 0101, reward for # 1's
    else if function == 21 {
        let mut num_ones = 0;
        let mut pat_found = false;
        let mut i = 0;
        while i < varlength.wrapping_sub(3) {
            if output.get(i) == 1 {
                num_ones += 1;
            } else {
                if output.get(i + 1) == 1 && output.get(i + 2) == 0 && output.get(i + 3) == 1 {
                    pat_found = true;
                }
                break;
            }
            i += 1;
        }
        fx = if pat_found {
            f64::from(varlength.wrapping_sub(4).wrapping_sub(num_ones))
        } else {
            f64::from(varlength)
        };
    }
    // simply rewared for number of 1's at beginning of string, maxFx=length of string
    else if function == 22 {
        let mut num_ones = 0;
        for i in 0..varlength {
            if output.get(i) == 1 {
                num_ones += 1;
            } else {
                break;
            }
        }
        fx = f64::from(varlength.wrapping_sub(num_ones));
    }
    // simply reward for number 0's in string
    else if function == 23 {
        let mut num_zeros = 0;
        for i in 0..varlength {
            if output.get(i) == 0 {
                num_zeros += 1;
            } else {
                break;
            }
        }
        fx = f64::from(varlength.wrapping_sub(num_zeros));
    } else if function == 18 {
        let mut tot = 0i32;
        for i in 0..30 {
            tot = tot.wrapping_add(output.get(i));
        }
        fx = f64::from(tot.wrapping_add(1));
    } else if function == 19 {
        let mut temp_vars = vec![0.0f64; nv.max(1) as usize];
        for i in 0..30 {
            temp_vars[0] += f64::from(output.get(i));
        }

        let len = varlength;
        let mut i = len.wrapping_sub(1);
        while i >= 0 {
            for j in 1..numvars {
                let idx = 30i32
                    .wrapping_add(i)
                    .wrapping_add(len.wrapping_mul(numvars.wrapping_sub(j).wrapping_sub(1)));
                temp_vars[(j - 1) as usize] += f64::from(output.get(idx));
            }
            i -= 1;
        }

        let mut gx = 0i32;
        for tv in temp_vars.iter().take(nv as usize).skip(1) {
            if *tv == 5.0 {
                gx += 1;
            } else {
                gx += (*tv as i32) + 2;
            }
        }
        // `Gx * (1 / (1 + tempVars[0]))`: int 1 over a double.
        fx = f64::from(gx) * (1.0 / (1.0 + temp_vars[0]));
    } else {
        if binary != 0 {
            let len = varlength;
            let base_pow = args.get_double(0);

            let mut temp_vars = vec![0.0f64; nv.max(1) as usize];
            let mut tot = 0.0f64;
            let mut i = len.wrapping_sub(1);
            while i >= 0 {
                for j in 0..numvars {
                    let idx =
                        i.wrapping_add(len.wrapping_mul(numvars.wrapping_sub(j).wrapping_sub(1)));
                    temp_vars[j as usize] += f64::from(output.get(idx))
                        * base_pow.powf(f64::from(len.wrapping_sub(1).wrapping_sub(i)));
                }
                tot += base_pow.powf(f64::from(i));
                i -= 1;
            }
            for i in 0..nv as usize {
                vars[i] = temp_vars[i] / tot;
            }
        } else {
            for j in 0..nv {
                // `/ 0xffffffff`: the divisor is an unsigned int converted to double.
                vars[j as usize] = f64::from(output.get(j)) / 4294967295.0;
            }
        }

        for v in vars.iter_mut().take(nv as usize) {
            *v = v.clamp(0.0, 1.0); // NaN stays NaN, as with the C++ comparisons
        }

        // The literal 3.14159 (not PI) is what the C++ uses.
        #[allow(clippy::approx_constant)]
        const PI_APPROX: f64 = 3.14159;

        // The reference binary evaluates this task in the Rust helper
        // `avd_task_eval_optimize` (rust/avida-rust/src/task_lib_helpers.rs),
        // which fuses no multiply-add (`otool -tv`: no `fmadd`/`fmsub`), so
        // the expressions below are its unfused ones rather than the
        // contracted pre-Rust C++. It also averages the g(x) variables as
        // `(sum of vars[1..]) / (numvars - 1)`, where the C++ summed
        // `vars[i] / (numvars - 1)`.
        let gx_sum = |vars: &[f64]| -> f64 {
            let n1 = f64::from(numvars.wrapping_sub(1));
            if n1 > 0.0 {
                vars.iter().skip(1).take((nv - 1) as usize).sum::<f64>() / n1
            } else {
                0.0
            }
        };
        match function {
            1 => fx = vars[0],                                                      // F1
            2 => fx = (1.0 + vars[1]) * (1.0 - (vars[0] / (1.0 + vars[1])).sqrt()), // F2
            3 => {
                let t = vars[0] / (1.0 + vars[1]);
                fx = (1.0 + vars[1]) * (1.0 - t * t); // F3
            }
            4 => {
                let q = vars[0] / (1.0 + vars[1]);
                fx = (1.0 + vars[1]) * (1.0 - q.sqrt() - q * (PI_APPROX * vars[0] * 10.0).sin());
            }
            5 => {
                vars[0] *= -2.0;
                fx = vars[0] * vars[0] + vars[1] * vars[1];
            }
            6 => {
                vars[0] *= -2.0;
                fx = (vars[0] + 2.0) * (vars[0] + 2.0) + vars[1] * vars[1];
            }
            7 => {
                vars[0] *= 4.0;
                fx = vars[0].sqrt() + vars[1];
            }
            8 => {
                vars[0] *= 4.0;
                fx = (4.0 - vars[0]).sqrt() + vars[1];
            }
            9 => {
                let gx = 1.0 + 9.0 * gx_sum(&vars);
                fx = gx * (1.0 - (vars[0] / gx).sqrt());
            }
            10 => {
                let gx = 1.0 + 9.0 * gx_sum(&vars);
                let t = vars[0] / gx;
                fx = gx * (1.0 - t * t);
            }
            11 => {
                let gx = 1.0 + 9.0 * gx_sum(&vars);
                let q = vars[0] / gx;
                fx = gx * (1.0 - q.sqrt() - q * (PI_APPROX * vars[0] * 10.0).sin());
            }
            12 => {
                vars[0] = vars[0] * 0.9 + 0.1;
                fx = vars[0];
            }
            13 => {
                vars[0] = vars[0] * 0.9 + 0.1;
                vars[1] *= 5.0;
                fx = (1.0 + vars[1]) / vars[0];
            }
            14 => {
                vars[0] = vars[0] * 6.0 - 3.0;
                vars[1] = vars[1] * 6.0 - 3.0;
                let s = vars[0] * vars[0] + vars[1] * vars[1];
                fx = 0.5 * s + s.sin();
            }
            15 => {
                vars[0] = vars[0] * 6.0 - 3.0;
                vars[1] = vars[1] * 6.0 - 3.0;
                let a = 3.0 * vars[0] - 2.0 * vars[1] + 4.0;
                let b = vars[0] - vars[1] + 1.0;
                fx = (a * a) / 8.0 + (b * b) / 27.0 + 15.0;
            }
            16 => {
                vars[0] = vars[0] * 6.0 - 3.0;
                vars[1] = vars[1] * 6.0 - 3.0;
                let e = (-(vars[0] * vars[0]) - vars[1] * vars[1]).exp();
                fx = 1.0 / (vars[0] * vars[0] + vars[1] * vars[1] + 1.0) - 1.1 * e;
            }
            17 => {
                let mut sum = 0.0;
                for i in 1..nv as usize {
                    let t = vars[i] * 6.0 - 3.0;
                    sum += (t * t - 10.0 * (4.0 * PI_APPROX * t).cos()) / 10.0;
                }
                let gx = 10.0 + sum;
                fx = gx * (1.0 - (vars[0] / gx).sqrt());
            }
            _ => {} // QUIRK: quality = .001 here is overwritten below.
        }
    }

    let max_fx = args.get_double(1);
    let min_fx = args.get_double(2);
    let thresh = args.get_double(3);
    let thresh_max = args.get_double(4);
    let quality = if thresh < 0.0 {
        let q1 = max_fx - fx + 0.001;
        let q2 = max_fx - min_fx + 0.001;
        // The C++ asserts q1 > 0 and q2 > 0.
        q1 / q2
    } else if thresh_max < 0.0 {
        if fx <= (max_fx - min_fx) * thresh + min_fx {
            1.0
        } else {
            0.0
        }
    } else if fx >= (max_fx - min_fx) * thresh + min_fx
        && fx <= (max_fx - min_fx) * thresh_max + min_fx
    {
        1.0
    } else {
        0.0
    };

    // because want org to only have 1 shot to use outputs for all functions at once, even if they
    // output numbers that give a quality of 0 on a function, still want to mark it as completed
    // so give it a very low quality instead of 0
    let quality = if quality < 0.001 { 0.001 } else { quality };
    OptimizeResult {
        quality,
        fx: Some(fx),
    }
}

/// `Task_CommEcho`: the output equals a value in a neighbour's input buffer.
pub(crate) fn eval_comm_echo(ctx: &TaskContext) -> f64 {
    let test_output = ctx.output_buffer.get(0);
    for buf in ctx.other_input_buffers {
        for i in 0..buf.num_stored() {
            if test_output == buf.get(i) {
                return 1.0;
            }
        }
    }
    0.0
}

/// `Task_CommNot`: the output is the bitwise NOT (`0 - (x + 1)`) of a value in
/// a neighbour's input buffer.
pub(crate) fn eval_comm_not(ctx: &TaskContext) -> f64 {
    let test_output = ctx.output_buffer.get(0);
    for buf in ctx.other_input_buffers {
        for i in 0..buf.num_stored() {
            if test_output == 0i32.wrapping_sub(buf.get(i).wrapping_add(1)) {
                return 1.0;
            }
        }
    }
    0.0
}

/// `Task_AllOnes`: mean of the first `length` output values.
pub(crate) fn eval_all_ones(args: &ArgContainer, output: &IoBuffer) -> f64 {
    let length = args.get_int(0);
    let mut num_ones = 0.0f64;
    for i in 0..length {
        num_ones += f64::from(output.get(i));
    }
    num_ones / f64::from(length)
}

/// `block_size = floor(double(length) / double(block_count))` (assigned to an int).
fn block_size_of(length: i32, block_count: i32) -> i32 {
    (f64::from(length) / f64::from(block_count)).floor() as i32
}

/// `Task_RoyalRoad`: fraction of blocks whose values AND to non-zero.
pub(crate) fn eval_royal_road(args: &ArgContainer, output: &IoBuffer) -> f64 {
    let length = args.get_int(0);
    let block_count = args.get_int(1);
    let block_size = block_size_of(length, block_count);
    let mut total_reward = 0.0f64;

    // Cycle through each block. If a block is correct, then add a reward.
    for i in 0..block_count {
        let mut block_reward = 1i32;
        // AND the elements of each block.
        for j in 0..block_size {
            let current_spot = i.wrapping_mul(block_size).wrapping_add(j);
            block_reward &= output.get(current_spot);
        }
        if block_reward != 0 {
            total_reward += 1.0;
        }
    }
    total_reward / f64::from(block_count)
}

/// `Task_RoyalRoadWithDitches`: A blocks (all ones), B blocks (ones then a
/// `width` ditch of zeros) and X blocks drive a three-state reward machine.
pub(crate) fn eval_royal_road_wd(args: &ArgContainer, output: &IoBuffer) -> f64 {
    let length = args.get_int(0);
    let block_count = args.get_int(1);
    let block_size = block_size_of(length, block_count);
    let width = args.get_int(2);
    let height = args.get_int(3);
    let mut num_b_blocks = 0i32;
    let mut total_reward = 0.0f64;
    let mut next_case = 1;

    for i in 0..block_count {
        let mut block_correct = 1i32;
        let mut block_type = -1; // -1 undefined; 0 X; 1 A; 2 B

        // Identify the type of block... check for block A
        for j in 0..block_size {
            let current_spot = i.wrapping_mul(block_size).wrapping_add(j);
            block_correct &= output.get(current_spot);
        }
        if block_correct != 0 {
            block_type = 1;
        }

        // Check for block B
        if block_type == -1 {
            block_correct = 1;
            for j in 0..block_size {
                let current_spot = i.wrapping_mul(block_size).wrapping_add(j);
                if j < block_size.wrapping_sub(width) {
                    if output.get(current_spot) == 0 {
                        block_correct = 0;
                    }
                } else if output.get(current_spot) == 1 {
                    block_correct = 0;
                }
                // this should escape the loop if the block reward is set to 0.
                if block_correct == 0 {
                    break;
                }
            }
            if block_correct != 0 {
                block_type = 2;
            }
        }

        // Else consider it an X
        if block_type == -1 {
            block_type = 0;
        }

        // Based on the type of block... change states....
        match next_case {
            1 => {
                if block_type == 0 {
                    next_case = 2;
                }
                if block_type == 1 {
                    total_reward = f64::from(num_b_blocks + 2);
                    next_case = 3;
                }
                if block_type == 2 {
                    num_b_blocks += 1;
                }
            }
            2 => {
                if block_type == 1 {
                    total_reward = f64::from(num_b_blocks + 2 - height);
                }
                next_case = 3;
            }
            _ => {}
        }
    }

    total_reward / f64::from(block_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arg_schema::{ArgSchema, ArgType, Feedback};

    fn buf_cap(cap: usize, vals: &[i32]) -> IoBuffer {
        let mut b = IoBuffer::new(cap);
        for &v in vals {
            b.add(v);
        }
        b
    }

    /// Buffer whose `get(i)` returns `vals[i]` (filled newest-last).
    fn from_get(cap: usize, vals: &[i32]) -> IoBuffer {
        let rev: Vec<i32> = vals.iter().rev().copied().collect();
        buf_cap(cap, &rev)
    }

    fn load(schema: &ArgSchema, s: &str) -> ArgContainer {
        let mut fb = Feedback::new();
        ArgContainer::load(s, schema, &mut fb).unwrap_or_else(|| panic!("{:?}", fb))
    }

    fn match_str_schema() -> ArgSchema {
        let mut s = ArgSchema::new();
        s.add_required("string", 0, ArgType::Str);
        s.add_int("partial", 0, 0);
        s.add_int("binary", 1, 1);
        s.add_double("pow", 0, 2.0);
        s
    }

    #[test]
    fn echo() {
        let i = buf_cap(3, &[1, 2, 3]);
        assert_eq!(eval_echo(&i, &buf_cap(3, &[2])), 1.0);
        assert_eq!(eval_echo(&i, &buf_cap(3, &[9])), 0.0);
        assert_eq!(eval_echo(&IoBuffer::new(3), &buf_cap(3, &[0])), 0.0);
        assert_eq!(eval_dont_care(), 1.0);
    }

    #[test]
    fn match_str_binary_mode() {
        let a = load(&match_str_schema(), "string=0101");
        // get(j) is compared with pattern[j]
        let o = from_get(4, &[0, 1, 0, 1]);
        assert_eq!(eval_match_str(&a, &o, None), 1.0); // 4/4 -> (1*2-1)^2
                                                       // get = 1,1,0,1: positions 1,2,3 match: base = 3*2/4 - 1 = 0.5 -> 0.25
        let o = from_get(4, &[1, 1, 0, 1]);
        assert_eq!(eval_match_str(&a, &o, None), 0.25);
        // half matched -> 0
        let o = from_get(4, &[0, 0, 1, 1]);
        assert_eq!(eval_match_str(&a, &o, None), 0.0);
    }

    #[test]
    fn match_str_bit_mode_and_received() {
        let a = load(&match_str_schema(), "string=1100, binary=0");
        // 4-char pattern against the low 4 bits: 0b1100 = 12
        assert_eq!(eval_match_str(&a, &buf_cap(2, &[12]), None), 1.0);
        // 0b1000: matches bits for '1','0','0' on positions... 3 of 4 matched
        assert_eq!(eval_match_str(&a, &buf_cap(2, &[8]), None), 0.25);
        // no output yet: nothing matched
        assert_eq!(eval_match_str(&a, &IoBuffer::new(2), None), 0.0);
        // a received message may do better than the output
        let recv = buf_cap(2, &[12]);
        assert_eq!(eval_match_str(&a, &buf_cap(2, &[8]), Some(&recv)), 1.0);
    }

    #[test]
    fn match_str_binary_mode_ignores_received_messages() {
        // The reference checks received messages in bit mode only.
        let a = load(&match_str_schema(), "string=1111");
        let recv = buf_cap(2, &[15]);
        assert_eq!(eval_match_str(&a, &IoBuffer::new(4), Some(&recv)), 0.0);
    }

    #[test]
    fn match_str_binary_mode_compares_stored_outputs_only() {
        let a = load(&match_str_schema(), "string=0000");
        // One output stored: the never-written slots behind it do not count
        // as zeros. 1 of 4 matched -> base -0.5 -> 0.
        assert_eq!(eval_match_str(&a, &buf_cap(4, &[0]), None), 0.0);
        // Three stored zeros: 3 of 4 -> base 0.5 -> 0.25.
        assert_eq!(eval_match_str(&a, &buf_cap(4, &[0, 0, 0]), None), 0.25);
    }

    #[test]
    fn match_str_partial_in_bit_mode_uses_the_pattern_length() {
        // Bit mode counts no real positions, so `partial` falls back to the
        // pattern length (the old C++ divided by zero).
        let a = load(&match_str_schema(), "string=0011,binary=0,partial=1");
        assert_eq!(eval_match_str(&a, &buf_cap(2, &[3]), None), 1.0);
        // 0b0111: 3 of 4 bits -> base 0.5 -> 0.25
        assert_eq!(eval_match_str(&a, &buf_cap(2, &[7]), None), 0.25);
    }

    #[test]
    fn match_str_partial_and_wildcards() {
        let a = load(&match_str_schema(), "string=1990,partial=1");
        // out[0]=1 (match '1'), out[3]=0 (match '0'); '9' wildcards do not count
        let o = from_get(4, &[1, 0, 0, 0]);
        // num_real = 2, max matched = 2 -> base 1.0
        assert_eq!(eval_match_str(&a, &o, None), 1.0);
    }

    #[test]
    fn fibonacci_sequence_state() {
        let mut s = ArgSchema::new();
        s.add_required("target", 0, ArgType::Int);
        s.add_double("penalty", 0, 0.0);
        let a = load(&s, "target=2,penalty=0.5");
        let mut states = crate::context::TaskStates::new();
        let i = IoBuffer::new(3);
        // seq starts {1, 0}: next = 1, then seq[1]=1 -> next 2, seq[0]=2 -> next 3 ...
        let expect = [(1, 1.0), (2, 1.0), (3, 0.5), (5, 0.5), (4, 0.0)];
        for (out_v, want) in expect {
            let o = buf_cap(3, &[out_v]);
            let mut ctx = TaskContext::simple(&i, &o);
            ctx.set_task_entry(7);
            ctx.set_task_states(&mut states);
            assert_eq!(eval_fibonacci_sequence(&mut ctx, &a), want, "out {}", out_v);
        }
        // without storage nothing can be rewarded
        let o = buf_cap(3, &[1]);
        let mut ctx = TaskContext::simple(&i, &o);
        ctx.set_task_entry(7);
        assert_eq!(eval_fibonacci_sequence(&mut ctx, &a), 0.0);
    }

    #[test]
    fn comm_tasks() {
        let n1 = buf_cap(3, &[10, 20]);
        let n2 = buf_cap(3, &[30]);
        let others = [&n1, &n2];
        let i = IoBuffer::new(3);
        let o = buf_cap(3, &[30]);
        let mut ctx = TaskContext::simple(&i, &o);
        ctx.other_input_buffers = &others;
        assert_eq!(eval_comm_echo(&ctx), 1.0);
        let o = buf_cap(3, &[-21]);
        let mut ctx = TaskContext::simple(&i, &o);
        ctx.other_input_buffers = &others;
        assert_eq!(eval_comm_not(&ctx), 1.0); // ~20
        assert_eq!(eval_comm_echo(&ctx), 0.0);
    }

    #[test]
    fn all_ones_and_royal_road() {
        let mut s = ArgSchema::new();
        s.add_int("length", 0, 0);
        s.add_int("block_count", 1, 0);
        s.add_int("width", 2, 0);
        s.add_int("height", 3, 0);
        let a = load(&s, "length=4");
        let o = buf_cap(4, &[1, 1, 0, 1]);
        assert_eq!(eval_all_ones(&a, &o), 0.75);
        // default length 0: 0/0 = NaN, as in C++
        assert!(eval_all_ones(&load(&s, ""), &o).is_nan());

        // royal road: 4 values, 2 blocks (values newest-first: [1,1,0,1] -> blocks [1,1],[0,1])
        let a = load(&s, "length=4,block_count=2");
        assert_eq!(eval_royal_road(&a, &o), 0.5);
        // QUIRK: block_size 0 (length < block_count) rewards every block
        let a = load(&s, "length=1,block_count=2");
        assert_eq!(eval_royal_road(&a, &o), 1.0);
        // block_count 0: 0/0
        assert!(eval_royal_road(&load(&s, "length=4"), &o).is_nan());
    }

    #[test]
    fn royal_road_with_ditches() {
        let mut s = ArgSchema::new();
        s.add_int("length", 0, 0);
        s.add_int("block_count", 1, 0);
        s.add_int("width", 2, 0);
        s.add_int("height", 3, 0);
        // 3 blocks of 2. Block B = [1, 0] (width 1). Order newest-first.
        let a = load(&s, "length=6,block_count=3,width=1,height=1");
        // blocks: B, B, A -> reward = num_b(2)+2 = 4 -> 4/3
        let o = from_get(6, &[1, 0, 1, 0, 1, 1]);
        // blocks [1,0] [1,0] [1,1] = B, B, A
        assert_eq!(eval_royal_road_wd(&a, &o), 4.0 / 3.0);
        // X, X, A: the second X ends the machine before A: no reward
        let o = from_get(6, &[0, 0, 0, 0, 1, 1]);
        assert_eq!(eval_royal_road_wd(&a, &o), 0.0);
        // X, A, X: A right after the X pays 0 + 2 - height(1) = 1
        let o = from_get(6, &[0, 0, 1, 1, 0, 0]);
        assert_eq!(eval_royal_road_wd(&a, &o), 1.0 / 3.0);
    }

    fn optimize_schema() -> ArgSchema {
        let mut s = ArgSchema::new();
        s.add_required("function", 0, ArgType::Int);
        s.add_int("binary", 1, 0);
        s.add_int("varlength", 2, 8);
        s.add_int("numvars", 3, 2);
        s.add_double("basepow", 0, 2.0);
        s.add_double("maxFx", 1, 1.0);
        s.add_double("minFx", 2, 0.0);
        s.add_double("thresh", 3, -1.0);
        s.add_double("threshMax", 4, -1.0);
        s
    }

    #[test]
    fn optimize_waits_for_a_full_buffer() {
        let a = load(&optimize_schema(), "function=1");
        let o = buf_cap(2, &[5]);
        let r = eval_optimize(&a, &o);
        assert_eq!((r.quality, r.fx), (0.0, None));
    }

    #[test]
    fn optimize_f1_continuous() {
        // F1: Fx = x0 where x0 = out[0] / 0xffffffff, clamped to [0,1].
        let a = load(&optimize_schema(), "function=1");
        // output[0] newest = 0 -> Fx = 0: quality = (1 - 0 + .001)/(1 - 0 + .001) = 1
        let o = buf_cap(2, &[7, 0]);
        let r = eval_optimize(&a, &o);
        assert_eq!(r.fx, Some(0.0));
        assert_eq!(r.quality, 1.0);
        // output[0] = -1 (all ones as unsigned would be 1.0, but it is int -1 -> clamp to 0)
        let o = buf_cap(2, &[0, -1]);
        assert_eq!(eval_optimize(&a, &o).fx, Some(0.0));
        // large positive value clamps to 1: quality = (1-1+.001)/(1.001)
        let o = buf_cap(2, &[0, i32::MAX]);
        let r = eval_optimize(&a, &o);
        let fx = f64::from(i32::MAX) / 4294967295.0;
        assert_eq!(r.fx, Some(fx));
        assert_eq!(r.quality, (1.0 - fx + 0.001) / (1.0 - 0.0 + 0.001));
    }

    #[test]
    fn optimize_threshold_modes_and_floor() {
        let o = buf_cap(2, &[0, 0]);
        // single threshold: Fx 0 <= (1-0)*0.5 + 0 -> 1
        let a = load(&optimize_schema(), "function=1,thresh=0.5");
        assert_eq!(eval_optimize(&a, &o).quality, 1.0);
        // band that excludes Fx = 0 -> floor at .001
        let a = load(&optimize_schema(), "function=1,thresh=0.25,threshMax=0.5");
        assert_eq!(eval_optimize(&a, &o).quality, 0.001);
        // unknown function: Fx stays 0, quality computed from the threshold code
        let a = load(&optimize_schema(), "function=99");
        let r = eval_optimize(&a, &o);
        assert_eq!(r.fx, Some(0.0));
        assert_eq!(r.quality, 1.0);
    }

    #[test]
    fn optimize_bit_string_functions() {
        let mut s = optimize_schema();
        s.add_required("string", 0, ArgType::Str);
        // function 22: leading ones, varlength 4 (buffer capacity 4)
        let a = load(&s, "function=22,varlength=4,string=0000");
        // get = 1,1,0,1 -> two leading ones -> Fx = 4 - 2 = 2
        let o = from_get(4, &[1, 1, 0, 1]);
        assert_eq!(eval_optimize(&a, &o).fx, Some(2.0));
        // function 23: leading zeros
        let a = load(&s, "function=23,varlength=4,string=0000");
        let o = from_get(4, &[0, 0, 1, 1]);
        assert_eq!(eval_optimize(&a, &o).fx, Some(2.0));
        // function 20: match "1010" against get = 1,0,1,0
        let a = load(&s, "function=20,varlength=4,string=1010");
        let o = from_get(4, &[1, 0, 1, 0]);
        assert_eq!(eval_optimize(&a, &o).fx, Some(0.0));
        // get = 1,0,1,1: three of four positions match
        let o = from_get(4, &[1, 0, 1, 1]);
        assert_eq!(eval_optimize(&a, &o).fx, Some(1.0));
    }

    #[test]
    fn optimize_binary_decoding() {
        // two variables, 2 bits each, base 2: vars decode as in C++.
        let a = load(
            &optimize_schema(),
            "function=1,binary=1,varlength=2,numvars=2",
        );
        // capacity 4. get(i + len*(nv-j-1)): var0 reads get(2),get(3); var1 get(0),get(1)
        // var0: out[3]*2^(1-1) ... loop i=1: t0 += out[1+2*(2-0-1)=3]*2^(0); i=0: t0 += out[2]*2^1
        let o = from_get(4, &[0, 0, 1, 0]);
        let r = eval_optimize(&a, &o);
        // t0 = out[3]*1 + out[2]*2 = 0 + 2 = 2; tot = 2^1 + 2^0 = 3 -> x0 = 2/3
        assert_eq!(r.fx, Some(2.0 / 3.0));
    }
}
