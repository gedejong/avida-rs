//! Task evaluators that need nothing but the context buffers and the task
//! arguments: `Task_Echo`, `Task_DontCare`, `Task_MatchStr`,
//! `Task_FibonacciSequence`, `Task_Optimize`, `Task_CommEcho`, `Task_CommNot`,
//! `Task_AllOnes`, `Task_RoyalRoad`, `Task_RoyalRoadWithDitches`
//! (avida-core/source/main/cTaskLib.cc).

use avida_apto::fmt_double;

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

/// Where the reference binary evaluates `Task_Optimize`.
const OPTIMIZE_AT: &str = "main/cTaskLib.cc:cTaskLib::Task_Optimize, which hands the outputs to \
     avd_task_eval_optimize in rust/avida-rust/src/task_lib_helpers.rs";

/// The number of variables `function` reads by index in the reference
/// evaluator (`vars[0]`, `vars[1]`; `tempVars[0]` for function 19).
fn optimize_vars_needed(function: i32) -> i32 {
    match function {
        2..=8 | 13..=16 => 2,
        1 | 9..=12 | 17 | 19 => 1,
        _ => 0,
    }
}

/// `Load_Optimize`: the arguments for which the reference binary does not
/// finish an evaluation of `Task_Optimize`, as an `unsupported:` message.
/// Every case below was run on the reference binary (an analyze-mode
/// `RECALCULATE` of organisms that fill their output buffer), and is tested
/// in the order the helper meets it:
///
/// - `function=20` reads the string argument 0, which `Load_Optimize` never
///   declares: `cArgContainer::GetString(0)` indexes an empty array and the
///   reference binary dies of a segmentation fault.
/// - A negative `numvars` with any function that decodes variables (all but
///   18 and 20 to 23) or with function 19 becomes a huge `usize` in the
///   helper, whose `vec![0.0; numvars]` panics; the panic aborts the
///   process.
/// - A negative `varlength` with the binary decoding, or with function 19
///   and `numvars` 2 or more, becomes a loop over about 2^64 positions that
///   the reference binary does not finish.
/// - Fewer variables than the function reads (`numvars` 0 for functions 1
///   to 17 and 19, 1 for functions 2 to 8 and 13 to 16): index out of
///   bounds in the helper, which aborts.
/// - A negative `varlength` with function 21 loops the same way as soon as
///   the outputs it reads are all 1 (it stops at the first 0).
///
/// The C++ fails only when it evaluates the task; the port refuses the
/// environment when it is loaded instead.
pub(crate) fn optimize_unsupported(args: &ArgContainer) -> Option<String> {
    let function = args.get_int(0);
    let binary = args.get_int(1);
    let varlength = args.get_int(2);
    let numvars = args.get_int(3);
    // Functions other than 18 to 23 decode the outputs into variables.
    let decodes = !(18..=23).contains(&function);
    let needed = optimize_vars_needed(function);
    let why = if function == 20 {
        "function=20: GetString(0) of an argument Load_Optimize does not declare \
         (the reference binary crashes)"
            .to_string()
    } else if numvars < 0 && (decodes || function == 19) {
        format!(
            "numvars={numvars} with function={function} \
             (the reference binary aborts allocating the variables)"
        )
    } else if varlength < 0 && ((decodes && binary != 0) || (function == 19 && numvars >= 2)) {
        format!(
            "varlength={varlength} with function={function}{} \
             (the reference binary loops over 2^64 positions)",
            if decodes { ",binary=1" } else { "" }
        )
    } else if needed > 0 && numvars < needed {
        format!(
            "numvars={numvars} with function={function}, which reads {needed} variable(s) \
             (the reference binary aborts on the out-of-bounds index)"
        )
    } else if varlength < 0 && function == 21 {
        format!(
            "varlength={varlength} with function=21 \
             (the reference binary loops over 2^64 positions when the outputs are all 1)"
        )
    } else {
        return None;
    };
    Some(format!("unsupported: optimize {why} (C++: {OPTIMIZE_AT})"))
}

/// `Task_Optimize` as the reference binary evaluates it: the C++ checks that
/// the output buffer is full and passes its stored values, newest first, to
/// the Rust helper `avd_task_eval_optimize` (rust/avida-rust/src/
/// task_lib_helpers.rs at the tag `cpp-final`), which computes `Fx` and the
/// quality; the C++ stores `Fx` with `SetTaskValue` and returns the quality.
/// This follows that helper, not the pre-Rust C++, wherever the two differ:
///
/// - Nothing past the stored outputs is read (functions 18 to 23 and the
///   variable decoding stop at the end of the buffer, where the C++ read
///   around it).
/// - An unknown function id gives quality 0.001 and `Fx` 0 at once; the C++
///   went on to compute the quality of `Fx == 0` from the thresholds.
/// - With the proportional quality (`thresh < 0`), `maxFx - Fx + .001 <= 0`
///   or `maxFx - minFx + .001 <= 0` gives 0.001, where the C++ only
///   asserted that neither happens (and divided).
/// - Binary decoding whose powers of `basepow` sum to 0 (`varlength` 0, or
///   `basepow=-1` with an even `varlength`) leaves every variable 0, where
///   the C++ divided by zero.
/// - g(x) of functions 9 to 11 averages as `(sum of vars[1..]) /
///   (numvars - 1)`, where the C++ summed `vars[i] / (numvars - 1)`.
/// - rustc does not contract, so no expression here is a fused multiply-add
///   (the pre-Rust C++ was fused).
///
/// The arguments the helper cannot evaluate are refused when the task is
/// loaded ([`optimize_unsupported`]); the evaluation here stays defined for
/// them all the same. A quality above 1 is returned as it is; the C++ then
/// prints `quality > 1!` ([`optimize_quality_note`]).
///
/// Args: int 0 = function, 1 = binary, 2 = varlength, 3 = numvars; double
/// 0 = basepow, 1 = maxFx, 2 = minFx, 3 = thresh, 4 = threshMax.
pub(crate) fn eval_optimize(args: &ArgContainer, output: &IoBuffer) -> OptimizeResult {
    // if the org hasn't output yet enough numbers, just return without completing any tasks
    if output.num_stored() < output.capacity() {
        return OptimizeResult {
            quality: 0.0,
            fx: None,
        };
    }

    // `tBuffer<int> buf(ctx.GetOutputBuffer())`, flattened for the helper.
    let out: Vec<i32> = (0..output.num_stored()).map(|i| output.get(i)).collect();
    // The helper's early return: quality 0, which the C++ stores as Fx 0.
    let zero = OptimizeResult {
        quality: 0.0,
        fx: Some(0.0),
    };
    if out.is_empty() {
        return zero;
    }

    let function = args.get_int(0);
    let binary = args.get_int(1);
    let varlength = args.get_int(2);
    let numvars = args.get_int(3);
    // `numvars as usize` / `varlength as usize` in the helper: a negative
    // value becomes a huge size. The loader refuses the arguments for which
    // that allocates, indexes or loops past what can be run; here the sizes
    // of allocations and loops are bounded so that nothing panics or hangs.
    let nv = numvars.max(0) as usize;
    let vl = varlength as usize;
    let var = |vars: &[f64], k: usize| vars.get(k).copied().unwrap_or(0.0);

    // The literal 3.14159 (not PI) is what the C++ uses.
    #[allow(clippy::approx_constant)]
    const PI_APPROX: f64 = 3.14159;

    let fx = match function {
        // string match: Fx = varlength - num_matched (0 best, varlength worst)
        20 => {
            let pattern = args.get_string(0).as_bytes();
            if pattern.is_empty() {
                return zero;
            }
            let limit = vl.min(out.len()).min(pattern.len());
            let mut matched = 0i32;
            for i in 0..limit {
                if (pattern[i] == b'0' && out[i] == 0) || (pattern[i] == b'1' && out[i] == 1) {
                    matched += 1;
                }
            }
            f64::from(varlength.wrapping_sub(matched))
        }
        // string with all 1's at beginning until pattern 0101, reward for # 1's.
        // The helper loops up to `varlength - 3` but does nothing past the
        // stored outputs, so the loop stops there.
        21 => {
            let limit = vl.saturating_sub(3).min(out.len());
            let mut num_ones = 0i32;
            let mut pat_found = false;
            for i in 0..limit {
                if out[i] == 1 {
                    num_ones += 1;
                } else {
                    if i + 3 < out.len() && out[i + 1] == 1 && out[i + 2] == 0 && out[i + 3] == 1 {
                        pat_found = true;
                    }
                    break;
                }
            }
            if pat_found {
                f64::from(varlength.wrapping_sub(4).wrapping_sub(num_ones))
            } else {
                f64::from(varlength)
            }
        }
        // simply rewarded for number of 1's at beginning of string
        22 => {
            let num_ones = out.iter().take(vl).take_while(|&&v| v == 1).count() as i32;
            f64::from(varlength.wrapping_sub(num_ones))
        }
        // simply reward for number 0's in string
        23 => {
            let num_zeros = out.iter().take(vl).take_while(|&&v| v == 0).count() as i32;
            f64::from(varlength.wrapping_sub(num_zeros))
        }
        18 => {
            let mut tot = 0i32;
            for &v in out.iter().take(30) {
                tot = tot.wrapping_add(v);
            }
            f64::from(1i32.wrapping_add(tot))
        }
        19 => {
            let mut temp_vars = vec![0.0f64; nv.max(1)];
            for &v in out.iter().take(30) {
                temp_vars[0] += f64::from(v);
            }
            if varlength > 0 {
                for i in (0..vl).rev() {
                    for j in 1..nv {
                        let idx = 30usize
                            .wrapping_add(i)
                            .wrapping_add(vl.wrapping_mul(nv - j - 1));
                        if idx < out.len() {
                            temp_vars[j - 1] += f64::from(out[idx]);
                        }
                    }
                }
            }
            let mut gx = 0i32;
            for tv in temp_vars.iter().take(nv).skip(1) {
                if *tv == 5.0 {
                    gx = gx.wrapping_add(1);
                } else {
                    gx = gx.wrapping_add((*tv as i32).wrapping_add(2));
                }
            }
            // `Gx * (1 / (1 + tempVars[0]))`
            f64::from(gx) * (1.0 / (1.0 + temp_vars[0]))
        }
        _ => {
            // get however many variables need, turn them into doubles between 0 and 1
            let mut vars = vec![0.0f64; nv];
            if binary != 0 {
                let base_pow = args.get_double(0);
                let mut temp_vars = vec![0.0f64; nv];
                let mut tot = 0.0f64;
                if varlength > 0 {
                    for i in (0..vl).rev() {
                        for (j, tv) in temp_vars.iter_mut().enumerate() {
                            let idx = i.wrapping_add(vl.wrapping_mul(nv - j - 1));
                            if idx < out.len() {
                                *tv += f64::from(out[idx]) * base_pow.powf(((vl - 1) - i) as f64);
                            }
                        }
                        tot += base_pow.powf(i as f64);
                    }
                }
                // The helper divides only by a non-zero total.
                if tot != 0.0 {
                    for (v, tv) in vars.iter_mut().zip(&temp_vars) {
                        *v = *tv / tot;
                    }
                }
            } else {
                for (v, &o) in vars.iter_mut().zip(&out) {
                    // `/ 0xffffffff`: the divisor is an unsigned int converted to double.
                    *v = f64::from(o) / 4294967295.0;
                }
            }

            for v in vars.iter_mut() {
                *v = v.clamp(0.0, 1.0); // NaN stays NaN
            }

            // g(x) of functions 9 to 11: the mean of vars[1..].
            let gx_mean = |vars: &[f64]| -> f64 {
                let denom = nv as f64 - 1.0;
                if denom > 0.0 {
                    vars.iter().skip(1).take(nv.wrapping_sub(1)).sum::<f64>() / denom
                } else {
                    0.0
                }
            };
            let (x0, x1) = (var(&vars, 0), var(&vars, 1));
            match function {
                1 => x0,
                2 => (1.0 + x1) * (1.0 - (x0 / (1.0 + x1)).sqrt()),
                3 => {
                    let t = x0 / (1.0 + x1);
                    (1.0 + x1) * (1.0 - t * t)
                }
                4 => {
                    let q = x0 / (1.0 + x1);
                    (1.0 + x1) * (1.0 - q.sqrt() - q * (PI_APPROX * x0 * 10.0).sin())
                }
                5 => {
                    let x = x0 * -2.0;
                    x * x + x1 * x1
                }
                6 => {
                    let x = x0 * -2.0;
                    (x + 2.0) * (x + 2.0) + x1 * x1
                }
                7 => {
                    let x = x0 * 4.0;
                    x.sqrt() + x1
                }
                8 => {
                    let x = x0 * 4.0;
                    (4.0 - x).sqrt() + x1
                }
                9 => {
                    let gx = 1.0 + 9.0 * gx_mean(&vars);
                    gx * (1.0 - (x0 / gx).sqrt())
                }
                10 => {
                    let gx = 1.0 + 9.0 * gx_mean(&vars);
                    let t = x0 / gx;
                    gx * (1.0 - t * t)
                }
                11 => {
                    let gx = 1.0 + 9.0 * gx_mean(&vars);
                    let q = x0 / gx;
                    gx * (1.0 - q.sqrt() - q * (PI_APPROX * x0 * 10.0).sin())
                }
                12 => x0 * 0.9 + 0.1,
                13 => {
                    let x = x0 * 0.9 + 0.1;
                    let y = x1 * 5.0;
                    (1.0 + y) / x
                }
                14 => {
                    let x = x0 * 6.0 - 3.0;
                    let y = x1 * 6.0 - 3.0;
                    let s = x * x + y * y;
                    0.5 * s + s.sin()
                }
                15 => {
                    let x = x0 * 6.0 - 3.0;
                    let y = x1 * 6.0 - 3.0;
                    let a = 3.0 * x - 2.0 * y + 4.0;
                    let b = x - y + 1.0;
                    (a * a) / 8.0 + (b * b) / 27.0 + 15.0
                }
                16 => {
                    let x = x0 * 6.0 - 3.0;
                    let y = x1 * 6.0 - 3.0;
                    1.0 / (x * x + y * y + 1.0) - 1.1 * (-(x * x) - y * y).exp()
                }
                17 => {
                    let mut sum = 0.0;
                    for &v in vars.iter().skip(1).take(nv.wrapping_sub(1)) {
                        let t = v * 6.0 - 3.0;
                        sum += (t * t - 10.0 * (4.0 * PI_APPROX * t).cos()) / 10.0;
                    }
                    let gx = 10.0 + sum;
                    gx * (1.0 - (x0 / gx).sqrt())
                }
                // Unknown function: the helper returns at once.
                _ => {
                    return OptimizeResult {
                        quality: 0.001,
                        fx: Some(0.0),
                    }
                }
            }
        }
    };

    let max_fx = args.get_double(1);
    let min_fx = args.get_double(2);
    let thresh = args.get_double(3);
    let thresh_max = args.get_double(4);
    let quality = if thresh < 0.0 {
        let q1 = max_fx - fx + 0.001;
        let q2 = max_fx - min_fx + 0.001;
        // The C++ asserted q1 > 0 and q2 > 0; the helper gives .001 instead.
        if q1 <= 0.0 || q2 <= 0.0 {
            0.001
        } else {
            q1 / q2
        }
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
    // so give it a very low quality instead of 0 (a NaN quality stays NaN)
    let quality = if quality < 0.001 { 0.001 } else { quality };
    OptimizeResult {
        quality,
        fx: Some(fx),
    }
}

/// What `cTaskLib::Task_Optimize` prints to standard output when the
/// quality exceeds 1: `cout << "\n\nquality > 1!  quality= " << quality <<
/// "  Fx= " << Fx << endl;`.
pub(crate) fn optimize_quality_note(quality: f64, fx: f64) -> String {
    format!(
        "\n\nquality > 1!  quality= {}  Fx= {}\n",
        fmt_double(quality),
        fmt_double(fx)
    )
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
    }

    #[test]
    fn optimize_unknown_function_skips_the_thresholds() {
        // The helper returns quality .001 and Fx 0 for an unknown function
        // (the C++ went on to the threshold code with Fx 0, giving 1 here).
        let o = buf_cap(2, &[0, 0]);
        for args in [
            "function=99",
            "function=0",
            "function=-3",
            "function=99,binary=1",
            "function=99,thresh=0.5",
            "function=99,numvars=0",
        ] {
            let r = eval_optimize(&load(&optimize_schema(), args), &o);
            assert_eq!((r.quality, r.fx), (0.001, Some(0.0)), "{args}");
        }
    }

    #[test]
    fn optimize_proportional_quality_guards_its_terms() {
        let o = buf_cap(2, &[0, 0]);
        // maxFx - minFx + .001 < 0: .001 (the C++ asserted it positive)
        let a = load(&optimize_schema(), "function=1,maxFx=0,minFx=1");
        assert_eq!(eval_optimize(&a, &o).quality, 0.001);
        // ... and == 0, where dividing gave inf
        let a = load(&optimize_schema(), "function=1,maxFx=0,minFx=0.001");
        assert_eq!(eval_optimize(&a, &o).quality, 0.001);
        // maxFx - Fx + .001 <= 0: Fx (about .5) above maxFx 0
        let a = load(&optimize_schema(), "function=1,maxFx=0,minFx=-1");
        let r = eval_optimize(&a, &buf_cap(2, &[0, i32::MAX]));
        assert!(r.fx.unwrap() > 0.001);
        assert_eq!(r.quality, 0.001);
        // Fx below minFx: a quality above 1 is returned as it is
        let a = load(&optimize_schema(), "function=1,maxFx=1,minFx=0.5");
        let r = eval_optimize(&a, &o);
        assert_eq!(r.quality, (1.0 - 0.0 + 0.001) / (1.0 - 0.5 + 0.001));
        assert_eq!(
            optimize_quality_note(r.quality, r.fx.unwrap()),
            "\n\nquality > 1!  quality= 1.998  Fx= 0\n"
        );
    }

    #[test]
    fn optimize_binary_decoding_with_a_zero_total() {
        // Powers of basepow summing to 0 leave every variable 0 (the C++
        // divided by zero, the old port gave NaN).
        let o = from_get(4, &[1, 0, 1, 1]);
        let a = load(
            &optimize_schema(),
            "function=1,binary=1,varlength=2,numvars=2,basepow=-1",
        );
        let r = eval_optimize(&a, &o);
        assert_eq!((r.quality, r.fx), (1.0, Some(0.0)));
        let a = load(&optimize_schema(), "function=1,binary=1,varlength=0");
        let r = eval_optimize(&a, &o);
        assert_eq!((r.quality, r.fx), (1.0, Some(0.0)));
    }

    #[test]
    fn optimize_reads_only_the_stored_outputs() {
        // varlength past the buffer capacity: leading ones / zeros are
        // counted over the 4 stored outputs only.
        let ones = from_get(4, &[1, 1, 1, 1]);
        let zeros = from_get(4, &[0, 0, 0, 0]);
        let a = load(&optimize_schema(), "function=22,varlength=8");
        assert_eq!(eval_optimize(&a, &ones).fx, Some(4.0));
        let a = load(&optimize_schema(), "function=23,varlength=8");
        assert_eq!(eval_optimize(&a, &zeros).fx, Some(4.0));
        // function 21 with all ones and no pattern: Fx = varlength
        let a = load(&optimize_schema(), "function=21,varlength=10");
        assert_eq!(eval_optimize(&a, &ones).fx, Some(10.0));
        // function 19: the first 30 outputs go to tempVars[0]; positions 30
        // and up do not exist in a buffer of 30 and add nothing.
        let thirty = from_get(30, &[1; 30]);
        let a = load(&optimize_schema(), "function=19,varlength=2,numvars=2");
        // Gx = tempVars[1] + 2 = 2; Fx = 2 * (1 / (1 + 30))
        assert_eq!(eval_optimize(&a, &thirty).fx, Some(2.0 * (1.0 / 31.0)));
        // a negative varlength with one variable: no Gx terms, Fx 0
        let a = load(&optimize_schema(), "function=19,varlength=-1,numvars=1");
        let r = eval_optimize(&a, &thirty);
        assert_eq!((r.quality, r.fx), (1.0, Some(0.0)));
        // function 18 sums the first 30 (here all there are)
        let a = load(&optimize_schema(), "function=18");
        assert_eq!(eval_optimize(&a, &thirty).fx, Some(31.0));
    }

    #[test]
    fn optimize_arguments_the_reference_cannot_evaluate() {
        let refused = |args: &str| optimize_unsupported(&load(&optimize_schema(), args));
        for args in [
            "function=20",
            "function=1,numvars=-1",
            "function=99,numvars=-1",
            "function=19,numvars=-1",
            "function=1,numvars=0",
            "function=17,numvars=0",
            "function=19,numvars=0",
            "function=2,numvars=1",
            "function=16,numvars=1",
            "function=1,binary=1,varlength=-1",
            "function=99,binary=1,varlength=-1",
            "function=19,varlength=-1",
            "function=21,varlength=-1",
        ] {
            let msg = refused(args).unwrap_or_else(|| panic!("{args} accepted"));
            assert!(msg.starts_with("unsupported: optimize "), "{msg}");
            assert!(msg.contains("avd_task_eval_optimize"), "{msg}");
        }
        for args in [
            "function=1",
            "function=5",
            "function=1,numvars=1",
            "function=9,numvars=1",
            "function=99,numvars=0",
            "function=18,numvars=-1",
            "function=22,numvars=-1,varlength=-1",
            "function=23,varlength=-1",
            "function=1,varlength=-1",
            "function=1,binary=1,varlength=0",
            // the loop over 2^64 has an empty body with one variable, and
            // the reference binary finishes (quality 1 for any outputs)
            "function=19,varlength=-1,numvars=1",
        ] {
            assert_eq!(refused(args), None, "{args}");
        }
    }

    #[test]
    fn optimize_bit_string_functions() {
        let mut s = optimize_schema();
        // Function 20 reads string 0, which `Load_Optimize` does not declare
        // (the task library refuses function 20); declared here to test the
        // helper's string match.
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
