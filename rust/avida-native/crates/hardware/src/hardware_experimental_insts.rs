//! The `cHardwareExperimental::Inst_*` handlers of the CPU core
//! (avida-core/source/cpu/cHardwareExperimental.cc at 47f13dadb):
//! threads, conditionals, stacks, arithmetic, input/output, heads, label
//! and nop-sequence search, waiting, promoters and reproduction; plus the
//! method-name -> handler table. The environment instructions are in
//! [`crate::hardware_experimental_insts_env`].
//!
//! Every handler returns `Ok(success)` as the C++ returns its `bool`;
//! `Err` is reserved for an unported path reached at run time and for C++
//! `exit()` calls.

use crate::hardware::{
    div_guard, divide_do_mutations, divide_test_fitness_measures, do_slip_mutation,
    do_uniform_copy_mutation, DivGuard,
};
use crate::hardware_cpu_insts::{bit_count, CONSENSUS, CONSENSUS24, MASK24};
use crate::hardware_experimental::{
    DataValue, HardwareExperimental, NUM_NOPS, NUM_REGISTERS, R_AX, R_BX, R_CX, R_DX,
};
use crate::hardware_experimental_lib::EXP_INST_DEFS;
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use avida_genome::inst_lib::{BehavClass, InstLib, InstLibEntry};
use avida_genome::{InstSet, Instruction, InstructionSequence};
use std::sync::{Arc, OnceLock};

/// A `cHardwareExperimental` instruction handler.
pub type ExpInstFn =
    fn(&mut HardwareExperimental, &mut OrgState, &mut dyn OrgInterface) -> Result<bool, String>;

pub(crate) type R = Result<bool, String>;
type Hw = HardwareExperimental;
type W<'a> = &'a mut dyn OrgInterface;

/// `cHardwareExperimental::GetInstLib()`: built once from
/// [`EXP_INST_DEFS`], default instruction 0, null instruction 16.
///
/// QUIRK: `s_n_array` lists 16 nops but `initInstLib` copies only the
/// first `NUM_REGISTERS` (8) into the static name and modifier arrays, so
/// nop-I..nop-P keep an empty nop name and modify register 0 (AX).
pub fn exp_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let entries = EXP_INST_DEFS
            .iter()
            .map(|d| InstLibEntry::new(d.name, d.class, d.flags, d.desc, BehavClass::None))
            .collect();
        let names = [
            "nop-A", "nop-B", "nop-C", "nop-D", "nop-E", "nop-F", "nop-G", "nop-H",
        ];
        let mut nops: Vec<(String, i32)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.to_string(), i as i32))
            .collect();
        nops.extend((NUM_REGISTERS..2 * NUM_NOPS as usize).map(|_| (String::new(), 0)));
        Arc::new(InstLib::new(entries, nops, 0, 16))
    })
    .clone()
}

/// Error if `is` (an experimental-hardware set) uses an instruction whose
/// handler is not ported.
pub fn check_exp_inst_set_ported(is: &InstSet) -> Result<(), String> {
    for op in 0..is.size() {
        let idx = is.lib_function_index(Instruction::new(op));
        let Some(def) = EXP_INST_DEFS.get(idx.max(0) as usize) else {
            continue;
        };
        if !exp_handler_ported(def.method) {
            return Err(format!(
                "unsupported: instruction '{}' in instruction set '{}' (C++: cHardwareExperimental.cc:cHardwareExperimental::{})",
                def.name,
                is.inst_set_name(),
                def.method
            ));
        }
    }
    Ok(())
}

/// The handler table, indexed like [`EXP_INST_DEFS`]; `None` for a method
/// that is not ported.
pub fn exp_handler_table() -> &'static [Option<ExpInstFn>] {
    static TABLE: OnceLock<Vec<Option<ExpInstFn>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        EXP_INST_DEFS
            .iter()
            .map(|d| exp_handler(d.method))
            .collect()
    })
}

/// Whether `method` (a `cHardwareExperimental::Inst_*` name) is ported.
pub fn exp_handler_ported(method: &str) -> bool {
    exp_handler(method).is_some()
}

fn exp_handler(method: &str) -> Option<ExpInstFn> {
    core_handler(method).or_else(|| crate::hardware_experimental_insts_env::handler(method))
}

fn core_handler(method: &str) -> Option<ExpInstFn> {
    let f: ExpInstFn = match method {
        "Inst_Nop" => |_, _, _| Ok(true),
        "Inst_ForkThread" => inst_fork_thread,
        "Inst_ThreadCreate" => inst_thread_create,
        "Inst_ExitThread" => inst_exit_thread,
        "Inst_IdThread" => inst_id_thread,
        "Inst_IfNEqu" => |hw, _, _| if_two(hw, |a, b| a == b),
        "Inst_IfLess" => |hw, _, _| if_two(hw, |a, b| a >= b),
        "Inst_IfNotZero" => |hw, _, _| if_one(hw, |a| a == 0),
        "Inst_IfEqualZero" => |hw, _, _| if_one(hw, |a| a != 0),
        "Inst_IfGreaterThanZero" => |hw, _, _| if_one(hw, |a| a <= 0),
        "Inst_IfLessThanZero" => |hw, _, _| if_one(hw, |a| a >= 0),
        "Inst_IfGtrX" => |hw, _, w| if_x(hw, w, |b, x| b <= x),
        "Inst_IfEquX" => |hw, _, w| if_x(hw, w, |b, x| b != x),
        "Inst_IfConsensus" => |hw, _, _| {
            let op1 = hw.find_modified_register(R_BX);
            if bit_count(hw.reg(op1) as u32) < CONSENSUS {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfConsensus24" => |hw, _, _| {
            let op1 = hw.find_modified_register(R_BX);
            if bit_count(hw.reg(op1) as u32 & MASK24) < CONSENSUS24 {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfLessConsensus" => |hw, _, _| {
            let op1 = hw.find_modified_register(R_BX);
            let op2 = hw.find_modified_next_register(op1);
            if bit_count(hw.reg(op1) as u32) >= bit_count(hw.reg(op2) as u32) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfLessConsensus24" => |hw, _, _| {
            let op1 = hw.find_modified_register(R_BX);
            let op2 = hw.find_modified_next_register(op1);
            if bit_count(hw.reg(op1) as u32 & MASK24) >= bit_count(hw.reg(op2) as u32 & MASK24) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfStackGreater" => inst_if_stack_greater,
        "Inst_Label" => |hw, _, w| {
            hw.rl(w);
            Ok(true)
        },
        "Inst_Pop" => inst_pop,
        "Inst_Push" => inst_push,
        "Inst_PopAll" => inst_pop_all,
        "Inst_PushAll" => inst_push_all,
        "Inst_SwitchStack" => |hw, _, _| {
            hw.switch_stack();
            Ok(true)
        },
        "Inst_SwapStackTop" => inst_swap_stack_top,
        "Inst_Swap" => inst_swap,
        "Inst_ShiftR" => |hw, _, _| unary_src(hw, |v| v >> 1),
        "Inst_ShiftL" => |hw, _, _| unary_src(hw, |v| v.wrapping_shl(1)),
        "Inst_Inc" => |hw, _, _| unary_src(hw, |v| v.wrapping_add(1)),
        "Inst_Dec" => |hw, _, _| unary_src(hw, |v| v.wrapping_sub(1)),
        "Inst_Zero" => |hw, _, _| {
            let r = hw.find_modified_register(R_BX);
            hw.set_ivp(r, 0);
            Ok(true)
        },
        "Inst_One" => |hw, _, _| {
            let r = hw.find_modified_register(R_BX);
            hw.set_ivp(r, 1);
            Ok(true)
        },
        "Inst_Rand" => inst_rand,
        "Inst_Mult100" => inst_mult100,
        "Inst_Add" => |hw, _, _| binary(hw, |a, b| a.wrapping_add(b)),
        "Inst_Sub" => |hw, _, _| binary(hw, |a, b| a.wrapping_sub(b)),
        "Inst_Mult" => |hw, _, _| binary(hw, |a, b| a.wrapping_mul(b)),
        "Inst_Nand" => |hw, _, _| binary(hw, |a, b| !(a & b)),
        "Inst_Div" => inst_div,
        "Inst_Mod" => inst_mod,
        "Inst_HeadAlloc" => inst_head_alloc,
        "Inst_TaskIO" => |hw, org, w| task_io(hw, org, w, false),
        "Inst_TaskIOExpire" => |hw, org, w| task_io(hw, org, w, true),
        "Inst_TaskInput" => inst_task_input,
        "Inst_TaskOutput" => |hw, org, w| task_output(hw, org, w, true, false, false),
        "Inst_TaskOutputZero" => |hw, org, w| task_output(hw, org, w, true, true, false),
        "Inst_TaskOutputExpire" => |hw, org, w| task_output(hw, org, w, false, false, true),
        "Inst_DemeIO" => inst_deme_io,
        "Inst_MoveHead" => inst_move_head,
        "Inst_MoveHeadIfNEqu" => |hw, _, _| move_head_if(hw, |a, b| a != b),
        "Inst_MoveHeadIfLess" => |hw, _, _| move_head_if(hw, |a, b| a < b),
        "Inst_Goto" => |hw, _, w| {
            hw.rl(w);
            goto_label(hw, w);
            Ok(true)
        },
        "Inst_GotoIfNEqu" => |hw, _, w| goto_if(hw, w, |a, b| a != b),
        "Inst_GotoIfLess" => |hw, _, w| goto_if(hw, w, |a, b| a < b),
        "Inst_GotoConsensus" => |hw, _, w| {
            if bit_count(hw.reg(R_BX) as u32) < CONSENSUS {
                return Ok(true);
            }
            hw.rl(w);
            rotate_label(hw);
            goto_label(hw, w);
            Ok(true)
        },
        "Inst_GotoConsensus24" => |hw, _, w| {
            if bit_count(hw.reg(R_BX) as u32 & MASK24) < CONSENSUS24 {
                return Ok(true);
            }
            hw.rl(w);
            rotate_label(hw);
            goto_label(hw, w);
            Ok(true)
        },
        "Inst_JumpHead" => inst_jump_head,
        "Inst_GetHead" => inst_get_head,
        "Inst_IfCopiedCompLabel" => |hw, _, w| if_copied(hw, w, true, false),
        "Inst_IfCopiedDirectLabel" => |hw, _, w| if_copied(hw, w, false, false),
        "Inst_IfCopiedCompSeq" => |hw, _, w| if_copied(hw, w, true, true),
        "Inst_IfCopiedDirectSeq" => |hw, _, w| if_copied(hw, w, false, true),
        "Inst_HeadDivide" => |hw, org, w| head_divide(hw, org, w, false),
        "Inst_HeadDivideSex" => |hw, org, w| head_divide(hw, org, w, true),
        "Inst_HeadRead" => inst_head_read,
        "Inst_HeadWrite" => inst_head_write,
        "Inst_HeadCopy" => inst_head_copy,
        "Inst_Search_Label_Comp_S" => |hw, _, w| search(hw, w, true, Search::LabelStart),
        "Inst_Search_Label_Comp_F" => |hw, _, w| search(hw, w, true, Search::LabelForward),
        "Inst_Search_Label_Comp_B" => |hw, _, w| search(hw, w, true, Search::LabelBackward),
        "Inst_Search_Label_Direct_S" => |hw, _, w| search(hw, w, false, Search::LabelStart),
        "Inst_Search_Label_Direct_F" => |hw, _, w| search(hw, w, false, Search::LabelForward),
        "Inst_Search_Label_Direct_B" => |hw, _, w| search(hw, w, false, Search::LabelBackward),
        "Inst_Search_Seq_Comp_S" => |hw, _, w| search(hw, w, true, Search::SeqStart),
        "Inst_Search_Seq_Comp_F" => |hw, _, w| search(hw, w, true, Search::SeqForward),
        "Inst_Search_Seq_Comp_B" => |hw, _, w| search(hw, w, true, Search::SeqBackward),
        "Inst_Search_Seq_Direct_S" => |hw, _, w| search(hw, w, false, Search::SeqStart),
        "Inst_Search_Seq_Direct_F" => |hw, _, w| search(hw, w, false, Search::SeqForward),
        "Inst_Search_Seq_Direct_B" => |hw, _, w| search(hw, w, false, Search::SeqBackward),
        "Inst_SetFlow" => inst_set_flow,
        "Inst_WaitCondition_Equal" => |hw, _, _| wait_condition(hw, Wait::Equal),
        "Inst_WaitCondition_Less" => |hw, _, _| wait_condition(hw, Wait::Less),
        "Inst_WaitCondition_Greater" => |hw, _, _| wait_condition(hw, Wait::Greater),
        "Inst_Promoter" => |_, _, _| Ok(true),
        "Inst_Terminate" => |hw, org, w| {
            hw.promoter_terminate(org, w)?;
            Ok(true)
        },
        "Inst_TerminateConsensus" => |hw, org, w| {
            let op1 = hw.find_modified_register(R_DX);
            if bit_count(hw.reg(op1) as u32) < CONSENSUS {
                hw.promoter_terminate(org, w)?;
            }
            Ok(true)
        },
        "Inst_TerminateConsensus24" => |hw, org, w| {
            let op1 = hw.find_modified_register(R_DX);
            if bit_count(hw.reg(op1) as u32 & MASK24) < CONSENSUS24 {
                hw.promoter_terminate(org, w)?;
            }
            Ok(true)
        },
        "Inst_Regulate" => inst_regulate,
        "Inst_RegulateSpecificPromoters" => inst_regulate_specific_promoters,
        "Inst_SenseRegulate" => |hw, org, w| {
            hw.sense_regulate(org, w);
            Ok(true)
        },
        "Inst_Numberate" => |hw, _, w| do_numberate(hw, w, 0),
        "Inst_Numberate24" => |hw, _, w| do_numberate(hw, w, 24),
        "Inst_BitConsensus" => |hw, _, _| bit_consensus(hw, false),
        "Inst_BitConsensus24" => |hw, _, _| bit_consensus(hw, true),
        "Inst_Execurate" => |hw, _, _| {
            let r = hw.find_modified_register(R_BX);
            let v = hw.threads[hw.ct()].execurate() as i32;
            hw.set_ivp(r, v);
            Ok(true)
        },
        "Inst_Execurate24" => |hw, _, _| {
            let r = hw.find_modified_register(R_BX);
            let v = (MASK24 & hw.threads[hw.ct()].execurate()) as i32;
            hw.set_ivp(r, v);
            Ok(true)
        },
        "Inst_Repro" => inst_repro,
        "Inst_Die" => |_, org, w| {
            org.die(w);
            Ok(true)
        },
        "Inst_ScrambleReg" => |hw, _, w| {
            for i in 0..NUM_REGISTERS {
                let v = w.rng().get_double() as i32;
                hw.set_iv(R_AX + i, v, true, false, false);
            }
            Ok(true)
        },
        _ => return None,
    };
    Some(f)
}

// ----------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------

/// `FromSensor(reg)` / `FromMessage(reg)` of the current thread.
#[inline]
fn flags_of(hw: &Hw, r: usize) -> (bool, bool) {
    let v = hw.regv(r);
    (v.from_sensor, v.from_message)
}

/// `m_from_sensor = FromSensor(r)` and `m_from_message = FromMessage(r)`.
#[inline]
pub(crate) fn mark_from(hw: &mut Hw, r: usize) {
    let (s, m) = flags_of(hw, r);
    hw.from_sensor = s;
    hw.from_message = m;
}

/// The same for two registers (`||`).
#[inline]
fn mark_from2(hw: &mut Hw, a: usize, b: usize) {
    let (s1, m1) = flags_of(hw, a);
    let (s2, m2) = flags_of(hw, b);
    hw.from_sensor = s1 || s2;
    hw.from_message = m1 || m2;
}

/// `GetLabel().Rotate(1, NUM_NOPS)`.
fn rotate_label(hw: &mut Hw) {
    let ct = hw.ct();
    hw.threads[ct].next_label.rotate(1, NUM_NOPS);
}

/// `getIP().Set(FindLabelForward(true))`.
fn goto_label(hw: &mut Hw, w: W<'_>) {
    let m = w.config().max_label_exe_size;
    let found = hw.find_label_forward(true, m);
    hw.head_ref(HEAD_IP).set_from(&found);
}

// ----------------------------------------------------------------------
// Threads
// ----------------------------------------------------------------------

fn inst_fork_thread(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    hw.ip_advance();
    let max = w.config().max_cpu_threads;
    if !hw.fork_thread(max) {
        org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_WARNING, "");
    }
    Ok(true)
}

fn inst_thread_create(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let head_used = hw.find_modified_head(HEAD_FLOW);
    let start = hw.get_head(head_used);
    let success = hw.thread_create(start, w.config().max_cpu_threads);
    if !success {
        org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_WARNING, "");
    }
    if success {
        org.phenotype.is_multi_thread = true;
    }
    Ok(success)
}

fn inst_exit_thread(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    if !hw.exit_thread() {
        org.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_WARNING, "");
    } else {
        hw.advance_ip = false;
    }
    Ok(true)
}

fn inst_id_thread(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let id = hw.threads[hw.ct()].id();
    hw.set_ivp(reg_used, id);
    Ok(true)
}

// ----------------------------------------------------------------------
// Conditionals
// ----------------------------------------------------------------------

/// `Inst_IfNEqu` / `Inst_IfLess`: skip when `skip(op1, op2)`.
fn if_two(hw: &mut Hw, skip: fn(i32, i32) -> bool) -> R {
    let op1 = hw.find_modified_register(R_BX);
    let op2 = hw.find_modified_next_register(op1);
    if skip(hw.reg(op1), hw.reg(op2)) {
        hw.ip_advance();
    }
    mark_from2(hw, op1, op2);
    Ok(true)
}

/// `Inst_IfNotZero` and the other single-register tests.
fn if_one(hw: &mut Hw, skip: fn(i32) -> bool) -> R {
    let op1 = hw.find_modified_register(R_BX);
    if skip(hw.reg(op1)) {
        hw.ip_advance();
    }
    mark_from(hw, op1);
    Ok(true)
}

/// `Inst_IfGtrX` / `Inst_IfEquX`.
fn if_x(hw: &mut Hw, w: W<'_>, skip: fn(i32, i32) -> bool) -> R {
    let mut value_to_compare: i32 = 1;
    hw.rl(w);
    let label = hw.label().clone();
    for i in 0..label.get_size() {
        let l = label.get(i);
        if l == R_AX as i32 {
            value_to_compare = value_to_compare.wrapping_mul(-1);
        } else {
            value_to_compare = value_to_compare.wrapping_shl(l as u32);
        }
    }
    if skip(hw.reg(R_BX), value_to_compare) {
        hw.ip_advance();
    }
    mark_from(hw, R_BX);
    Ok(true)
}

fn inst_if_stack_greater(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let cur_stack = i32::from(hw.threads[hw.ct()].cur_stack);
    let a = hw.stack_mut(cur_stack).peek().value;
    let b = hw.stack_mut(i32::from(cur_stack == 0)).peek().value;
    if a <= b {
        hw.ip_advance();
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Stacks and registers
// ----------------------------------------------------------------------

fn inst_pop(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    mark_from(hw, reg_used);
    let pop = hw.stack_pop();
    hw.set_iv_src(reg_used, pop.value, &pop);
    Ok(true)
}

fn inst_push(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    mark_from(hw, reg_used);
    let v = hw.regv(reg_used);
    let cs = i32::from(hw.threads[hw.ct()].cur_stack);
    hw.stack_mut(cs).push(&v);
    Ok(true)
}

fn inst_pop_all(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let mut reg_used = hw.find_modified_register(R_BX);
    let mut any_from_sensor = false;
    let mut any_from_message = false;
    for _ in 0..NUM_REGISTERS {
        let (s, m) = flags_of(hw, reg_used);
        any_from_sensor |= s;
        any_from_message |= m;
        let pop = hw.stack_pop();
        hw.set_iv_src(reg_used, pop.value, &pop);
        reg_used += 1;
        if reg_used == NUM_REGISTERS {
            reg_used = 0;
        }
    }
    hw.from_sensor = any_from_sensor;
    hw.from_message = any_from_message;
    Ok(true)
}

fn inst_push_all(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let mut reg_used = hw.find_modified_register(R_BX);
    let mut any_from_sensor = false;
    let mut any_from_message = false;
    for _ in 0..NUM_REGISTERS {
        let (s, m) = flags_of(hw, reg_used);
        any_from_sensor |= s;
        any_from_message |= m;
        let v = hw.regv(reg_used);
        let cs = i32::from(hw.threads[hw.ct()].cur_stack);
        hw.stack_mut(cs).push(&v);
        reg_used += 1;
        if reg_used == NUM_REGISTERS {
            reg_used = 0;
        }
    }
    hw.from_sensor = any_from_sensor;
    hw.from_message = any_from_message;
    Ok(true)
}

fn inst_swap_stack_top(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let v0 = hw.stack_mut(0).pop();
    let v1 = hw.stack_mut(1).pop();
    hw.stack_mut(0).push(&v1);
    hw.stack_mut(1).push(&v0);
    Ok(true)
}

/// `Inst_Swap`: through `DataValue::operator=`, so the sensor and message
/// flags stay with the registers.
fn inst_swap(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let op1 = hw.find_modified_register(R_BX);
    let op2 = hw.find_modified_next_register(op1);
    let ct = hw.ct();
    let v1 = hw.threads[ct].reg[op1];
    let v2 = hw.threads[ct].reg[op2];
    hw.threads[ct].reg[op1].assign(&v2);
    hw.threads[ct].reg[op2].assign(&v1);
    Ok(true)
}

/// `Inst_ShiftR`, `ShiftL`, `Inc`, `Dec`: `setInternalValue(r, f(v), reg)`.
fn unary_src(hw: &mut Hw, f: fn(i32) -> i32) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    mark_from(hw, reg_used);
    let v = hw.regv(reg_used);
    hw.set_iv_src(reg_used, f(v.value), &v);
    Ok(true)
}

fn inst_rand(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let randsign = if w.rng().get_uint_range(0, 2) != 0 {
        -1
    } else {
        1
    };
    let v = w.rng().get_int(i32::MAX).wrapping_mul(randsign);
    hw.set_ivp(reg_used, v);
    Ok(true)
}

fn inst_mult100(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    mark_from(hw, reg_used);
    let v = hw.reg(reg_used).wrapping_mul(100);
    hw.set_ivp(reg_used, v);
    Ok(true)
}

/// The three-register arithmetic (`dst = op1 <f> op2`).
fn binary_regs(hw: &mut Hw) -> (usize, usize, usize) {
    let dst = hw.find_modified_register(R_BX);
    let op1 = hw.find_modified_register(dst);
    let op2 = hw.find_modified_next_register(op1);
    mark_from2(hw, op1, op2);
    (dst, op1, op2)
}

fn binary(hw: &mut Hw, f: fn(i32, i32) -> i32) -> R {
    let (dst, op1, op2) = binary_regs(hw);
    let r1 = hw.regv(op1);
    let r2 = hw.regv(op2);
    hw.set_iv_ops(dst, f(r1.value, r2.value), &r1, &r2);
    Ok(true)
}

fn inst_div(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    let (dst, op1, op2) = binary_regs(hw);
    let r1 = hw.regv(op1);
    let r2 = hw.regv(op2);
    match div_guard(r1.value, r2.value) {
        DivGuard::Ok => hw.set_iv_ops(dst, r1.value.wrapping_div(r2.value), &r1, &r2),
        DivGuard::Overflow => {
            org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: Float exception");
        }
        DivGuard::Zero => {
            org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: dividing by 0");
            return Ok(false);
        }
    }
    Ok(true)
}

fn inst_mod(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    let (dst, op1, op2) = binary_regs(hw);
    let r1 = hw.regv(op1);
    let r2 = hw.regv(op2);
    if div_guard(r1.value, r2.value) == DivGuard::Ok {
        // INT_MIN % -1 traps on x86 (SIGFPE); `wrapping_rem` gives 0.
        hw.set_iv_ops(dst, r1.value.wrapping_rem(r2.value), &r1, &r2);
    } else {
        org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "mod: modding by 0");
        return Ok(false);
    }
    Ok(true)
}

fn inst_head_alloc(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let dst = hw.find_modified_register(R_AX);
    let cur_size = hw.mem_size_i();
    let range = w.config().offspring_size_range;
    let alloc_size = ((range * f64::from(cur_size)) as i32).min(MAX_GENOME_LENGTH - cur_size);
    if hw.allocate_main(org, w, alloc_size) {
        hw.set_ivp(dst, cur_size);
        Ok(true)
    } else {
        Ok(false)
    }
}

// ----------------------------------------------------------------------
// Input / output
// ----------------------------------------------------------------------

/// `Inst_TaskIO` / `Inst_TaskIOExpire`.
fn task_io(hw: &mut Hw, org: &mut OrgState, w: W<'_>, expire: bool) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let reg = hw.regv(reg_used);
    if expire && hw.io_expire && reg.env_component && reg.oldest_component < hw.last_output {
        return Ok(false);
    }
    hw.org_do_output(org, w, Some(reg.value), false)?;
    hw.last_output = hw.cycle_count;
    let value_in = org.get_next_input(w);
    hw.set_iv(reg_used, value_in, true, false, false);
    org.do_input(value_in);
    Ok(true)
}

fn inst_task_input(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let value_in = org.get_next_input(w);
    hw.set_iv(reg_used, value_in, true, false, false);
    org.do_input(value_in);
    Ok(true)
}

/// `Inst_TaskOutput` (`mark`), `Inst_TaskOutputZero` (`mark`, `zero`) and
/// `Inst_TaskOutputExpire` (`expire`).
fn task_output(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    mark: bool,
    zero: bool,
    expire: bool,
) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    if mark {
        mark_from(hw, reg_used);
    }
    let reg = hw.regv(reg_used);
    if expire && hw.io_expire && reg.env_component && reg.oldest_component < hw.last_output {
        return Ok(false);
    }
    hw.org_do_output(org, w, Some(reg.value), false)?;
    hw.last_output = hw.cycle_count;
    if zero {
        hw.set_ivp(reg_used, 0);
    }
    Ok(true)
}

fn inst_deme_io(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let v = hw.reg(reg_used);
    w.do_deme_output(v);
    let value_in = w.get_next_deme_input();
    if value_in != -1 {
        hw.set_iv(reg_used, value_in, true, false, false);
        w.do_deme_input(value_in);
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Heads and flow
// ----------------------------------------------------------------------

fn inst_move_head(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let head_used = hw.find_modified_head(HEAD_IP);
    let target = hw.find_modified_head(HEAD_FLOW);
    let t = hw.get_head(target);
    hw.head_ref(head_used).set_from(&t);
    if head_used == HEAD_IP {
        hw.advance_ip = false;
    }
    Ok(true)
}

fn move_head_if(hw: &mut Hw, cond: fn(i32, i32) -> bool) -> R {
    let op1 = hw.find_modified_register(R_BX);
    let op2 = hw.find_modified_next_register(op1);
    let head_used = hw.find_modified_head(HEAD_IP);
    let target = hw.find_modified_head(HEAD_FLOW);
    if cond(hw.reg(op1), hw.reg(op2)) {
        let t = hw.get_head(target);
        hw.head_ref(head_used).set_from(&t);
        if head_used == HEAD_IP {
            hw.advance_ip = false;
        }
    }
    Ok(true)
}

fn goto_if(hw: &mut Hw, w: W<'_>, cond: fn(i32, i32) -> bool) -> R {
    let op1 = hw.find_modified_register(R_BX);
    let op2 = hw.find_modified_next_register(op1);
    hw.rl(w);
    if cond(hw.reg(op1), hw.reg(op2)) {
        goto_label(hw, w);
    }
    Ok(true)
}

fn inst_jump_head(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let head_used = hw.find_modified_head(HEAD_IP);
    let reg = hw.find_modified_register(R_CX);
    mark_from(hw, reg);
    let v = hw.reg(reg);
    hw.head_jump(head_used, v);
    if head_used == HEAD_IP {
        hw.advance_ip = false;
    }
    Ok(true)
}

fn inst_get_head(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let head_used = hw.find_modified_head(HEAD_IP);
    let reg = hw.find_modified_register(R_CX);
    let p = hw.get_head(head_used).position();
    hw.set_ivp(reg, p);
    Ok(true)
}

/// `Inst_IfCopied{Comp,Direct}{Label,Seq}`.
fn if_copied(hw: &mut Hw, w: W<'_>, comp: bool, seq: bool) -> R {
    hw.rl(w);
    if comp {
        rotate_label(hw);
    }
    let t = &hw.threads[hw.ct()];
    let other = if seq { &t.read_seq } else { &t.read_label };
    if t.next_label != *other {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_HeadDivide` / `Inst_HeadDivideSex`.
fn head_divide(hw: &mut Hw, org: &mut OrgState, w: W<'_>, sex: bool) -> R {
    org.phenotype.divide_sex = sex;
    org.phenotype.cross_num = i32::from(sex);
    hw.adjust_heads();
    let divide_pos = hw.get_head(HEAD_READ).position();
    let mut child_end = hw.get_head(HEAD_WRITE).position();
    if child_end == 0 {
        child_end = hw.mem_size_i();
    }
    let extra_lines = hw.mem_size_i() - child_end;
    let ret_val = hw.divide_main(org, w, divide_pos, extra_lines, 1.0)?;
    hw.adjust_heads();
    Ok(ret_val)
}

fn inst_head_read(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let head_id = hw.find_modified_head(HEAD_READ);
    let dst = hw.find_modified_register(R_AX);
    hw.head_adjust(head_id);
    let read_inst = if org.mut_rates.test_copy_mut(w.rng()) {
        hw.base.inst_set.random_inst(w.rng())
    } else {
        hw.get_head(head_id).get_inst(&hw.memory)
    };
    hw.set_ivp(dst, read_inst.op());
    hw.read_inst(read_inst);
    if hw.slip_read_head && org.mut_rates.test_copy_slip(w.rng()) {
        let size = hw.mem_size_i();
        let p = w.rng().get_int(size);
        hw.head_set_pos(head_id, p);
    }
    hw.head_advance(head_id);
    Ok(true)
}

/// `doSlipMutation(ctx, m_memory, from)`.
fn slip_mutation(hw: &mut Hw, w: W<'_>, from: i32) -> Result<(), String> {
    let is = hw.base.inst_set.clone();
    let verbosity = w.verbosity();
    let cfg = w.config().clone();
    do_slip_mutation(&is, &cfg, verbosity, w.rng(), &mut hw.memory, from)
}

/// `doUniformCopyMutation(ctx, head)`.
fn uniform_copy_mutation(hw: &mut Hw, w: W<'_>, head: &HeadCpu) {
    let is = hw.base.inst_set.clone();
    let cfg = w.config().clone();
    do_uniform_copy_mutation(&is, &cfg, w.rng(), head, &mut hw.memory);
}

fn inst_head_write(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let head_id = hw.find_modified_head(HEAD_WRITE);
    let src = hw.find_modified_register(R_AX);
    hw.head_adjust(head_id);
    let active_head = hw.get_head(head_id);
    let mut value = hw.reg(src);
    if value < 0 || value >= hw.base.inst_set.size() {
        value = 0;
    }
    active_head.set_inst(&mut hw.memory, Instruction::new(value));
    active_head.set_flag_copied(&mut hw.memory);

    if org.mut_rates.test_copy_ins(w.rng()) {
        let inst = hw.base.inst_set.random_inst(w.rng());
        active_head.insert_inst(&mut hw.memory, inst);
    }
    if org.mut_rates.test_copy_del(w.rng()) {
        active_head.remove_inst(&mut hw.memory);
    }
    if org.mut_rates.test_copy_uniform(w.rng()) {
        uniform_copy_mutation(hw, w, &active_head);
    }
    if !hw.slip_read_head && org.mut_rates.test_copy_slip(w.rng()) {
        slip_mutation(hw, w, active_head.position())?;
    }
    hw.head_advance(head_id);
    Ok(true)
}

fn inst_head_copy(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let size = hw.mem_size_i();
    hw.head_ref(HEAD_READ).adjust(size, 1);
    hw.head_ref(HEAD_WRITE).adjust(size, 1);
    let read_head = hw.get_head(HEAD_READ);
    let write_head = hw.get_head(HEAD_WRITE);

    let mut read_inst = read_head.get_inst(&hw.memory);
    hw.read_inst(read_inst);
    if org.mut_rates.test_copy_mut(w.rng()) {
        read_inst = hw.base.inst_set.random_inst(w.rng());
        write_head.set_flag_mutated(&mut hw.memory);
        write_head.set_flag_copy_mut(&mut hw.memory);
    }
    write_head.set_inst(&mut hw.memory, read_inst);
    write_head.set_flag_copied(&mut hw.memory);

    if org.mut_rates.test_copy_ins(w.rng()) {
        let inst = hw.base.inst_set.random_inst(w.rng());
        write_head.insert_inst(&mut hw.memory, inst);
    }
    if org.mut_rates.test_copy_del(w.rng()) {
        write_head.remove_inst(&mut hw.memory);
    }
    if org.mut_rates.test_copy_uniform(w.rng()) {
        uniform_copy_mutation(hw, w, &write_head);
    }
    if org.mut_rates.test_copy_slip(w.rng()) {
        if hw.slip_read_head {
            let size = hw.mem_size_i();
            let p = w.rng().get_int(size);
            hw.head_set_pos(HEAD_READ, p);
        } else {
            slip_mutation(hw, w, write_head.position())?;
        }
    }
    hw.head_advance(HEAD_READ);
    hw.head_advance(HEAD_WRITE);
    Ok(true)
}

#[derive(Clone, Copy)]
enum Search {
    LabelStart,
    LabelForward,
    SeqStart,
    SeqForward,
    /// `FindLabelBackward` (returns the IP).
    LabelBackward,
    /// `FindNopSequenceBackward` (returns the IP).
    SeqBackward,
}

/// `Inst_Search_{Label,Seq}_{Comp,Direct}_{S,F,B}`.
fn search(hw: &mut Hw, w: W<'_>, comp: bool, kind: Search) -> R {
    hw.rl(w);
    if comp {
        rotate_label(hw);
    }
    let m = w.config().max_label_exe_size;
    let found = match kind {
        Search::LabelStart => hw.find_label_start(true, m),
        Search::LabelForward => hw.find_label_forward(true, m),
        Search::SeqStart => hw.find_nop_sequence_start(true, m),
        Search::SeqForward => hw.find_nop_sequence_forward(true, m),
        Search::LabelBackward => hw.find_label_backward(),
        Search::SeqBackward => hw.find_nop_sequence_backward(),
    };
    hw.head_ref(HEAD_FLOW).set_from(&found);
    hw.head_advance(HEAD_FLOW);
    Ok(true)
}

fn inst_set_flow(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_CX);
    mark_from(hw, reg_used);
    let v = hw.reg(reg_used);
    hw.head_set_pos(HEAD_FLOW, v);
    Ok(true)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    Equal,
    Less,
    Greater,
}

/// `Inst_WaitCondition_{Equal,Less,Greater}`.
fn wait_condition(hw: &mut Hw, kind: Wait) -> R {
    let wait_value = hw.find_modified_register(R_BX);
    let check_reg = hw.find_modified_register(R_DX);
    let wait_dst = hw.find_modified_register(wait_value);
    mark_from(hw, wait_value);

    let ct = hw.ct();
    let target = hw.threads[ct].reg[wait_value].value;
    for i in 0..hw.threads.len() {
        if i == hw.cur_thread {
            continue;
        }
        let other = hw.threads[i].reg[check_reg];
        let met = match kind {
            Wait::Equal => other.value == target,
            Wait::Less => other.value < target,
            Wait::Greater => other.value > target,
        };
        if met {
            hw.set_iv_src(wait_dst, other.value, &other);
            return Ok(true);
        }
    }
    if hw.waiting_threads as usize == hw.threads.len() - 1 {
        return Ok(false);
    }
    hw.waiting_threads = (hw.waiting_threads + 1) & 0xF;
    let t = &mut hw.threads[ct];
    t.active = false;
    t.wait_equal = kind == Wait::Equal;
    t.wait_less = kind == Wait::Less;
    t.wait_greater = kind == Wait::Greater;
    t.wait_reg = (check_reg as u32) & 0xF;
    t.wait_value = target;
    t.wait_dst = (wait_dst as u32) & 0xF;
    Ok(true)
}

// ----------------------------------------------------------------------
// Promoters and bit codes
// ----------------------------------------------------------------------

fn inst_regulate(hw: &mut Hw, _org: &mut OrgState, _w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let code = hw.reg(reg_used);
    for p in hw.promoters.iter_mut() {
        p.regulation = code;
    }
    Ok(true)
}

fn inst_regulate_specific_promoters(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let reg_promoter = hw.find_modified_next_register(reg_used);
    let regulation_code = hw.reg(reg_used);
    let regulation_promoter = hw.reg(reg_promoter);
    let exe_length = w.config().promoter_exe_length;
    for p in hw.promoters.iter_mut() {
        let mut test_p_code = p.bit_code;
        let mut test_r_code = regulation_promoter;
        let mut bit_count = 0;
        for _ in 0..exe_length {
            if (test_p_code & 1) == (test_r_code & 1) {
                bit_count += 1;
            }
            test_p_code >>= 1;
            test_r_code >>= 1;
        }
        if bit_count >= exe_length / 2 {
            p.regulation = regulation_code;
        }
    }
    Ok(true)
}

/// `Do_Numberate(ctx, num_bits)`.
fn do_numberate(hw: &mut Hw, w: W<'_>, num_bits: i32) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    hw.ip_advance();
    hw.advance_ip = false;
    let pos = hw.ip_head().position();
    let num = hw.numberate(pos, 1, num_bits, w.config().inst_code_length);
    hw.set_ivp(reg_used, num);
    Ok(true)
}

/// `Inst_BitConsensus` / `Inst_BitConsensus24`.
fn bit_consensus(hw: &mut Hw, b24: bool) -> R {
    let reg_used = hw.find_modified_register(R_BX);
    let op1 = hw.find_modified_next_register(reg_used);
    let val: DataValue = hw.regv(op1);
    let set = if b24 {
        bit_count(val.value as u32 & MASK24) >= CONSENSUS24
    } else {
        bit_count(val.value as u32) >= CONSENSUS
    };
    hw.set_iv_src(reg_used, i32::from(set), &val);
    Ok(true)
}

// ----------------------------------------------------------------------
// Reproduction
// ----------------------------------------------------------------------

/// `Inst_Repro(ctx)`: copy the whole memory as the offspring.
pub(crate) fn inst_repro(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if org.phenotype.cur_bonus < w.config().required_bonus {
        return Ok(false);
    }
    let seq: InstructionSequence = hw.memory.to_sequence();
    org.offspring_genome = hw.make_offspring(seq);
    org.phenotype.child_copied_size = hw.mem_size_i();
    let lines_executed = (0..hw.mem_size_i())
        .filter(|i| hw.memory.flag_executed(*i))
        .count() as i32;
    org.phenotype.executed_size = lines_executed;

    let is = hw.base.inst_set.clone();
    let prey_mut_off = w.config().prey_mut_off != 0;
    if org.mut_rates.copy.mut_prob > 0.0 && (org.forage_target < -1 || !prey_mut_off) {
        let n = org.offspring_genome.representation().size();
        for i in 0..n {
            if org.mut_rates.test_copy_mut(w.rng()) {
                let r = is.random_inst(w.rng());
                org.offspring_genome.representation_mut().set(i, r);
            }
        }
    }
    if org.forage_target < -1 || !prey_mut_off {
        divide_do_mutations(&is, org, w, &mut hw.memory, 1.0, i32::MAX)?;
    }

    let parent_size = org.initial_genome.representation().size();
    let child_size = org.offspring_genome.representation().size();
    if !hw.check_viable(org, w, parent_size, child_size, true) {
        return Ok(false);
    }
    divide_test_fitness_measures(org, w, false);
    hw.base.reset_ft_costs();
    hw.mal_active = false;
    if w.config().divide_method == DIVIDE_METHOD_SPLIT {
        hw.advance_ip = false;
    }
    let parent_alive = hw.activate_divide(org, w)?;
    if parent_alive {
        if w.config().divide_method == DIVIDE_METHOD_SPLIT {
            hw.full_reset(org, w);
        }
        hw.memory.clear_flags();
    }
    Ok(true)
}
