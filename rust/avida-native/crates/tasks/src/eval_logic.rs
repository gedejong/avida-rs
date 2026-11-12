//! Logic-id computation and the boolean logic tasks.
//!
//! `compute_logic_id` is `cTaskLib::SetupTests`; `eval_logic` is the body of
//! `Task_Not` .. `Task_Equ` (also reused by the `eat-target-*` tasks).

use crate::io_buffer::IoBuffer;

/// The nine standard 1- and 2-input logic functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOp {
    /// `Task_Not`
    Not,
    /// `Task_Nand`
    Nand,
    /// `Task_And`
    And,
    /// `Task_OrNot` (registered as `orn`)
    OrNot,
    /// `Task_Or`
    Or,
    /// `Task_AndNot` (registered as `andn`)
    AndNot,
    /// `Task_Nor`
    Nor,
    /// `Task_Xor`
    Xor,
    /// `Task_Equ`
    Equ,
}

/// The logic ids (of 3-input truth tables) each logic task accepts
/// (`Task_Not` .. `Task_Equ`, cTaskLib.cc).
fn logic_ids(op: LogicOp) -> &'static [i32] {
    match op {
        LogicOp::Not => &[15, 51, 85],
        LogicOp::Nand => &[63, 95, 119],
        LogicOp::And => &[136, 160, 192],
        LogicOp::OrNot => &[175, 187, 207, 221, 243, 245],
        LogicOp::Or => &[238, 250, 252],
        LogicOp::AndNot => &[10, 12, 34, 48, 68, 80],
        LogicOp::Nor => &[3, 5, 17],
        LogicOp::Xor => &[60, 90, 102],
        LogicOp::Equ => &[153, 165, 195],
    }
}

/// `Task_Not` .. `Task_Equ`: 1.0 when the context's logic id is one of the
/// function's accepted ids, else 0.0.
pub fn eval_logic(op: LogicOp, logic_id: i32) -> f64 {
    if logic_ids(op).contains(&logic_id) {
        1.0
    } else {
        0.0
    }
}

/// `Task_Logic3in_*`: 1.0 when the logic id is any of `targets`.
pub(crate) fn eval_logic3(targets: &[i32], logic_id: i32) -> f64 {
    if targets.contains(&logic_id) {
        1.0
    } else {
        0.0
    }
}

/// `cTaskLib::SetupTests`: derive the logic id (0..=255) from the first three
/// inputs and the newest output, or -1 when the output is not a consistent
/// boolean function of the inputs.
///
/// Missing inputs (`num_inputs` below 3) are mirrored into the unused truth
/// table positions exactly as the C++ does.
pub fn compute_logic_id(input_buffer: &IoBuffer, output_buffer: &IoBuffer) -> i32 {
    let num_inputs = input_buffer.num_stored();
    let mut test_inputs = [0i32; 3];
    for (i, slot) in test_inputs.iter_mut().enumerate() {
        *slot = if num_inputs > i as i32 {
            input_buffer.get(i as i32)
        } else {
            0
        };
    }

    let mut test_output = 0i32;
    if output_buffer.num_stored() != 0 {
        test_output = output_buffer.get(0);
    }

    // logic_out[i] == -1 means "not yet set".
    let mut logic_out = [-1i32; 8];

    let mut func_ok = true;
    for _ in 0..32 {
        let mut logic_pos = 0i32;
        for (i, inp) in test_inputs.iter().enumerate() {
            logic_pos += (inp & 1) << i;
        }
        let logic_pos = logic_pos as usize;

        if logic_out[logic_pos] != -1 && logic_out[logic_pos] != (test_output & 1) {
            func_ok = false;
            break;
        } else {
            logic_out[logic_pos] = test_output & 1;
        }

        test_output >>= 1;
        for inp in test_inputs.iter_mut() {
            *inp >>= 1;
        }
    }

    if !func_ok {
        return -1;
    }

    if num_inputs < 1 {
        // 000 -> 001
        logic_out[1] = logic_out[0];
    }
    if num_inputs < 2 {
        // 000 -> 010; 001 -> 011
        logic_out[2] = logic_out[0];
        logic_out[3] = logic_out[1];
    }
    if num_inputs < 3 {
        // 000->100; 001->101; 010->110; 011->111
        logic_out[4] = logic_out[0];
        logic_out[5] = logic_out[1];
        logic_out[6] = logic_out[2];
        logic_out[7] = logic_out[3];
    }

    // The C++ asserts every slot is 0 or 1 here. With 32 tested bit positions
    // and at most 3 inputs every slot is reached, so this only fails when the
    // caller passes inconsistent buffer state (e.g. `num_inputs` of 0 with
    // inputs that never visit all positions); -1 slots then contribute as in
    // C++ release builds (`-1 << i` summed).
    let mut logicid = 0i32;
    for (i, v) in logic_out.iter().enumerate() {
        logicid = logicid.wrapping_add(v.wrapping_shl(i as u32));
    }
    logicid
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(vals: &[i32]) -> IoBuffer {
        let mut b = IoBuffer::new(vals.len().max(3));
        for &v in vals {
            b.add(v);
        }
        b
    }

    // Standard Avida test inputs: A, B, C are the bit patterns 0x55555555,
    // 0x33333333, 0x0f0f0f0f. input_buffer[0] is the NEWEST input, so the
    // buffer is filled C-first.
    const A: i32 = 0x5555_5555;
    const B: i32 = 0x3333_3333;
    const C: i32 = 0x0f0f_0f0f;

    fn logic_id_of(out: i32) -> i32 {
        let inputs = buf(&[C, B, A]); // get(0) == A
        let output = buf(&[out]);
        compute_logic_id(&inputs, &output)
    }

    #[test]
    fn nine_standard_tasks_by_hand_computed_triples() {
        // With inputs[0]=A, [1]=B, [2]=C: a function of A,B only has logic ids
        // built from truth-table bit position p = a + 2b + 4c.
        // NOT A = ~A -> table bits where a==0: positions 0,2,4,6 -> 0b01010101 = 85.
        assert_eq!(logic_id_of(!A), 85);
        assert_eq!(eval_logic(LogicOp::Not, logic_id_of(!A)), 1.0);
        // NAND(A,B): 1 unless a&b -> positions with a&b are 3,7 -> 0b00110111?
        // bits set at 0,1,2,4,5,6 = 1+2+4+16+32+64 = 119.
        assert_eq!(logic_id_of(!(A & B)), 119);
        assert_eq!(eval_logic(LogicOp::Nand, 119), 1.0);
        // AND(A,B): positions 3,7 -> 8+128 = 136
        assert_eq!(logic_id_of(A & B), 136);
        assert_eq!(eval_logic(LogicOp::And, 136), 1.0);
        // ORN(A,B) = A | ~B: positions with a==1 or b==0: 0,1,3,4,5,7 -> 1+2+8+16+32+128 = 187
        assert_eq!(logic_id_of(A | !B), 187);
        assert_eq!(eval_logic(LogicOp::OrNot, 187), 1.0);
        // OR(A,B): not (a==0&&b==0): positions 1,2,3,5,6,7 -> 2+4+8+32+64+128 = 238
        assert_eq!(logic_id_of(A | B), 238);
        assert_eq!(eval_logic(LogicOp::Or, 238), 1.0);
        // ANDN(A,B) = A & ~B: a==1,b==0: positions 1,5 -> 2+32 = 34
        assert_eq!(logic_id_of(A & !B), 34);
        assert_eq!(eval_logic(LogicOp::AndNot, 34), 1.0);
        // NOR(A,B): a==0,b==0: positions 0,4 -> 1+16 = 17
        assert_eq!(logic_id_of(!(A | B)), 17);
        assert_eq!(eval_logic(LogicOp::Nor, 17), 1.0);
        // XOR(A,B): a!=b: positions 1,2,5,6 -> 2+4+32+64 = 102
        assert_eq!(logic_id_of(A ^ B), 102);
        assert_eq!(eval_logic(LogicOp::Xor, 102), 1.0);
        // EQU(A,B): a==b: positions 0,3,4,7 -> 1+8+16+128 = 153
        assert_eq!(logic_id_of(!(A ^ B)), 153);
        assert_eq!(eval_logic(LogicOp::Equ, 153), 1.0);
    }

    #[test]
    fn logic_tasks_reject_other_ids() {
        for id in 0..256 {
            let hits = [
                LogicOp::Not,
                LogicOp::Nand,
                LogicOp::And,
                LogicOp::OrNot,
                LogicOp::Or,
                LogicOp::AndNot,
                LogicOp::Nor,
                LogicOp::Xor,
                LogicOp::Equ,
            ]
            .iter()
            .filter(|&&op| eval_logic(op, id) == 1.0)
            .count();
            assert!(hits <= 1, "id {} accepted by several tasks", id);
        }
        assert_eq!(eval_logic(LogicOp::Not, -1), 0.0);
        assert_eq!(eval_logic(LogicOp::Xor, 0), 0.0);
    }

    #[test]
    fn inconsistent_output_gives_minus_one() {
        // Output that is not a function of the inputs (all inputs equal, output
        // alternates).
        let inputs = buf(&[0, 0, 0]);
        let output = buf(&[0x5555_5555]);
        assert_eq!(compute_logic_id(&inputs, &output), -1);
    }

    #[test]
    fn fewer_than_three_inputs_are_mirrored() {
        // One input A, output = ~A: NOT on a single input. With one input the
        // table is mirrored across b and c: ids 85 (c,b mirrored from a).
        let mut inputs = IoBuffer::new(3);
        inputs.add(A);
        let output = buf(&[!A]);
        assert_eq!(compute_logic_id(&inputs, &output), 85);
        // Echo of the single input: 0b10101010 = 170.
        assert_eq!(compute_logic_id(&inputs, &buf(&[A])), 170);
    }

    #[test]
    fn empty_output_buffer_is_zero_output() {
        let inputs = buf(&[C, B, A]);
        let output = IoBuffer::new(3);
        assert_eq!(compute_logic_id(&inputs, &output), 0);
    }

    #[test]
    fn logic3_matches_listed_ids() {
        assert_eq!(eval_logic3(&[1], 1), 1.0);
        assert_eq!(eval_logic3(&[2, 4, 16], 4), 1.0);
        assert_eq!(eval_logic3(&[2, 4, 16], 5), 0.0);
    }
}
