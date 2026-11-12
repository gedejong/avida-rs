//! The `cHardwareBCR` instruction library (`cHardwareBCR::initInstLib`,
//! avida-core/source/cpu/cHardwareBCR.cc at 47f13dadb) and the CPU-core
//! `Inst_*` handlers that `cHardwareBCR` and `cHardwareGP8` share: threads,
//! conditionals, stacks, arithmetic, input/output, heads, label and
//! nop-sequence search, waiting and reproduction. The environment
//! instructions are in [`crate::hardware_bcr_insts_env`].
//!
//! The handlers are generic over [`GeneHw`]. Where the GP8 version of a
//! handler differs from the BCR one only by letting an operand be an
//! immediate-value instruction (`FindModifiedRegister(reg, true)` and
//! `getRegister(ctx, reg)`), one generic handler serves both, switched by
//! [`GeneHw::IMM`]; with `IMM` false it is the BCR text exactly.
//!
//! Every handler returns `Ok(success)` as the C++ returns its `bool`;
//! `Err` is reserved for an unported path reached at run time.

use crate::hardware::{
    div_guard, divide_do_mutations, divide_test_fitness_measures, do_slip_mutation, DivGuard,
};
use crate::hardware_bcr::{
    check_viable, divide_main, full_reset, make_offspring, org_do_output, GeneHead, GeneHw,
    HardwareBcr, H_FLOW, H_IP, H_READ, H_WRITE, R_AX, R_BX, R_CX,
};
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;
use avida_genome::inst_lib::{
    inst_flag as F, BehavClass as B, InstLib, InstLibEntry, InstructionClass as C,
};
use avida_genome::{InstSet, Instruction};
use std::sync::{Arc, OnceLock};

pub(crate) type R = Result<bool, String>;
pub(crate) type W<'a> = &'a mut dyn OrgInterface;

/// An instruction handler of a gene-expression hardware type.
pub(crate) type GeneInstFn<H> = fn(&mut H, &mut OrgState, &mut dyn OrgInterface) -> R;

/// A `cHardwareBCR` instruction handler.
pub type BcrInstFn = fn(&mut HardwareBcr, &mut OrgState, &mut dyn OrgInterface) -> R;

/// One `tInstLibEntry<tMethod>` of `cHardwareBCR`.
#[derive(Debug, Clone, Copy)]
pub struct BcrInstDef {
    /// Instruction name.
    pub name: &'static str,
    /// The `cHardwareBCR::Inst_*` handler.
    pub method: &'static str,
    /// `InstructionClass`
    pub class: C,
    /// `nInstFlag` bits
    pub flags: u32,
    /// `BehavClass`
    pub behav: B,
}

const fn d(name: &'static str, method: &'static str, class: C, flags: u32, behav: B) -> BcrInstDef {
    BcrInstDef {
        name,
        method,
        class,
        flags,
        behav,
    }
}

const N: B = B::None;

/// `s_f_array`, in order: the twelve nops, `NULL` (the null instruction,
/// index 12), then the rest. Descriptions are left out (nothing prints
/// them).
#[rustfmt::skip]
pub static BCR_INST_DEFS: &[BcrInstDef] = &[
    d("nop-A", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-B", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-C", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-D", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-E", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-F", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-G", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-H", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-I", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-J", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-K", "Inst_Nop", C::Nop, F::NOP, N),
    d("nop-L", "Inst_Nop", C::Nop, F::NOP, N),
    d("NULL", "Inst_Nop", C::Nop, 0, N),
    d("nop-X", "Inst_Nop", C::Nop, 0, N),
    d("thread-create", "Inst_ThreadCreate", C::Other, 0, N),
    d("thread-cancel", "Inst_ThreadCancel", C::Other, 0, N),
    d("thread-id", "Inst_ThreadID", C::Other, 0, N),
    d("yield", "Inst_Yield", C::Other, 0, N),
    d("regulate-pause", "Inst_RegulatePause", C::Other, 0, N),
    d("regulate-pause-sp", "Inst_RegulatePauseSP", C::Other, 0, N),
    d("regulate-resume", "Inst_RegulateResume", C::Other, 0, N),
    d("regulate-resume-sp", "Inst_RegulateResumeSP", C::Other, 0, N),
    d("regulate-reset", "Inst_RegulateReset", C::Other, 0, N),
    d("regulate-reset-sp", "Inst_RegulateResetSP", C::Other, 0, N),
    d("if-n-equ", "Inst_IfNEqu", C::Conditional, 0, N),
    d("if-less", "Inst_IfLess", C::Conditional, 0, N),
    d("if-not-0", "Inst_IfNotZero", C::Conditional, 0, N),
    d("if-equ-0", "Inst_IfEqualZero", C::Conditional, 0, N),
    d("if-gtr-0", "Inst_IfGreaterThanZero", C::Conditional, 0, N),
    d("if-less-0", "Inst_IfLessThanZero", C::Conditional, 0, N),
    d("if-gtr-x", "Inst_IfGtrX", C::Conditional, 0, N),
    d("if-equ-x", "Inst_IfEquX", C::Conditional, 0, N),
    d("pop", "Inst_Pop", C::Data, 0, N),
    d("push", "Inst_Push", C::Data, 0, N),
    d("pop-all", "Inst_PopAll", C::Data, 0, N),
    d("push-all", "Inst_PushAll", C::Data, 0, N),
    d("swap-stk", "Inst_SwitchStack", C::Data, 0, N),
    d("swap-stk-top", "Inst_SwapStackTop", C::Data, 0, N),
    d("swap", "Inst_Swap", C::Data, 0, N),
    d("copy-val", "Inst_CopyVal", C::Data, 0, N),
    d("shift-r", "Inst_ShiftR", C::ArithmeticLogic, 0, N),
    d("shift-l", "Inst_ShiftL", C::ArithmeticLogic, 0, N),
    d("inc", "Inst_Inc", C::ArithmeticLogic, 0, N),
    d("dec", "Inst_Dec", C::ArithmeticLogic, 0, N),
    d("zero", "Inst_Zero", C::ArithmeticLogic, 0, N),
    d("one", "Inst_One", C::ArithmeticLogic, 0, N),
    d("rand", "Inst_Rand", C::ArithmeticLogic, 0, N),
    d("add", "Inst_Add", C::ArithmeticLogic, 0, N),
    d("sub", "Inst_Sub", C::ArithmeticLogic, 0, N),
    d("nand", "Inst_Nand", C::ArithmeticLogic, 0, N),
    d("IO", "Inst_TaskIO", C::Environment, F::STALL, B::Action),
    d("input", "Inst_TaskInput", C::Environment, F::STALL, B::Input),
    d("output", "Inst_TaskOutput", C::Environment, F::STALL, B::Action),
    d("mult", "Inst_Mult", C::ArithmeticLogic, 0, N),
    d("div", "Inst_Div", C::ArithmeticLogic, 0, N),
    d("mod", "Inst_Mod", C::ArithmeticLogic, 0, N),
    d("label", "Inst_Label", C::FlowControl, F::LABEL, N),
    d("search-lbl-direct-s", "Inst_Search_Label_Direct_S", C::FlowControl, 0, N),
    d("search-lbl-direct-f", "Inst_Search_Label_Direct_F", C::FlowControl, 0, N),
    d("search-lbl-direct-b", "Inst_Search_Label_Direct_B", C::FlowControl, 0, N),
    d("search-lbl-direct-d", "Inst_Search_Label_Direct_D", C::FlowControl, 0, N),
    d("search-seq-comp-s", "Inst_Search_Seq_Comp_S", C::FlowControl, 0, N),
    d("search-seq-comp-f", "Inst_Search_Seq_Comp_F", C::FlowControl, 0, N),
    d("search-seq-comp-b", "Inst_Search_Seq_Comp_B", C::FlowControl, 0, N),
    d("search-seq-comp-d", "Inst_Search_Seq_Comp_D", C::FlowControl, 0, N),
    d("mov-head", "Inst_MoveHead", C::FlowControl, 0, N),
    d("mov-head-if-n-equ", "Inst_MoveHeadIfNEqu", C::FlowControl, 0, N),
    d("mov-head-if-less", "Inst_MoveHeadIfLess", C::FlowControl, 0, N),
    d("jmp-head", "Inst_JumpHead", C::FlowControl, 0, N),
    d("get-head", "Inst_GetHead", C::FlowControl, 0, N),
    d("set-memory", "Inst_SetMemory", C::FlowControl, 0, N),
    d("promoter", "Inst_Nop", C::FlowControl, F::PROMOTER, N),
    d("terminator", "Inst_Nop", C::FlowControl, F::TERMINATOR, N),
    d("divide", "Inst_Divide", C::Lifecycle, F::STALL, B::Copy),
    d("divide-memory", "Inst_DivideMemory", C::Lifecycle, F::STALL, B::Copy),
    d("h-copy", "Inst_HeadCopy", C::Lifecycle, 0, B::Copy),
    d("h-read", "Inst_HeadRead", C::Lifecycle, 0, B::Copy),
    d("h-write", "Inst_HeadWrite", C::Lifecycle, 0, B::Copy),
    d("if-copied-lbl-comp", "Inst_IfCopiedCompLabel", C::Conditional, 0, N),
    d("if-copied-lbl-direct", "Inst_IfCopiedDirectLabel", C::Conditional, 0, N),
    d("if-copied-seq-comp", "Inst_IfCopiedCompSeq", C::Conditional, 0, N),
    d("if-copied-seq-direct", "Inst_IfCopiedDirectSeq", C::Conditional, 0, N),
    d("did-copy-lbl-comp", "Inst_DidCopyCompLabel", C::Other, 0, N),
    d("did-copy-lbl-direct", "Inst_DidCopyDirectLabel", C::Other, 0, N),
    d("did-copy-seq-comp", "Inst_DidCopyCompSeq", C::Other, 0, N),
    d("did-copy-seq-direct", "Inst_DidCopyDirectSeq", C::Other, 0, N),
    d("repro", "Inst_Repro", C::Lifecycle, F::STALL, B::Copy),
    d("die", "Inst_Die", C::Lifecycle, F::STALL, B::Copy),
    d("wait-cond-equ", "Inst_WaitCondition_Equal", C::Other, F::STALL, N),
    d("wait-cond-less", "Inst_WaitCondition_Less", C::Other, F::STALL, N),
    d("wait-cond-gtr", "Inst_WaitCondition_Greater", C::Other, F::STALL, N),
    d("sg-move", "Inst_SGMove", C::Environment, 0, B::Action),
    d("sg-rotate-l", "Inst_SGRotateL", C::Environment, 0, B::Action),
    d("sg-rotate-r", "Inst_SGRotateR", C::Environment, 0, B::Action),
    d("sg-sense", "Inst_SGSense", C::Environment, 0, B::Input),
    d("move", "Inst_Move", C::Environment, F::STALL, B::Action),
    d("juv-move", "Inst_JuvMove", C::Environment, F::STALL, B::Action),
    d("get-north-offset", "Inst_GetNorthOffset", C::Environment, 0, B::Input),
    d("get-position-offset", "Inst_GetPositionOffset", C::Environment, 0, B::Input),
    d("get-northerly", "Inst_GetNortherly", C::Environment, 0, B::Input),
    d("get-easterly", "Inst_GetEasterly", C::Environment, 0, B::Input),
    d("zero-easterly", "Inst_ZeroEasterly", C::Environment, 0, B::Input),
    d("zero-northerly", "Inst_ZeroNortherly", C::Environment, 0, B::Input),
    d("zero-position-offset", "Inst_ZeroPosOffset", C::Environment, 0, B::Input),
    d("rotate-home", "Inst_RotateHome", C::Environment, F::STALL, B::Action),
    d("rotate-to-unoccupied-cell", "Inst_RotateUnoccupiedCell", C::Environment, F::STALL, B::Action),
    d("rotate-x", "Inst_RotateX", C::Environment, F::STALL, B::Action),
    d("rotate-org-id", "Inst_RotateOrgID", C::Environment, F::STALL, B::Action),
    d("rotate-away-org-id", "Inst_RotateAwayOrgID", C::Environment, F::STALL, B::Action),
    d("sense-resource-id", "Inst_SenseResourceID", C::Environment, F::STALL, B::Input),
    d("sense-nest", "Inst_SenseNest", C::Environment, F::STALL, B::Input),
    d("sense-faced-habitat", "Inst_SenseFacedHabitat", C::Environment, F::STALL, B::Input),
    d("look-ahead", "Inst_LookAhead", C::Environment, F::STALL, B::Input),
    d("look-ahead-intercept", "Inst_LookAheadIntercept", C::Environment, F::STALL, B::Input),
    d("look-ahead-ex", "Inst_LookAheadEX", C::Environment, F::STALL, B::Input),
    d("look-again-ex", "Inst_LookAgainEX", C::Environment, F::STALL, B::Input),
    d("look-ahead-ftx", "Inst_LookAheadFTX", C::Environment, F::STALL, B::Input),
    d("look-again-ftx", "Inst_LookAgainFTX", C::Environment, F::STALL, B::Input),
    d("look-around", "Inst_LookAround", C::Environment, F::STALL, B::Input),
    d("look-around-intercept", "Inst_LookAroundIntercept", C::Environment, F::STALL, B::Input),
    d("look-ft", "Inst_LookFT", C::Environment, F::STALL, B::Input),
    d("look-around-ft", "Inst_LookAroundFT", C::Environment, F::STALL, B::Input),
    d("set-forage-target", "Inst_SetForageTarget", C::Environment, F::STALL, B::Action),
    d("set-ft-once", "Inst_SetForageTargetOnce", C::Environment, F::STALL, B::Action),
    d("set-rand-ft-once", "Inst_SetRandForageTargetOnce", C::Environment, F::STALL, B::Action),
    d("get-forage-target", "Inst_GetForageTarget", C::Environment, 0, B::Input),
    d("collect-specific", "Inst_CollectSpecific", C::Environment, F::STALL, B::Action),
    d("get-res-stored", "Inst_GetResStored", C::Environment, F::STALL, B::Input),
    d("set-opinion", "Inst_SetOpinion", C::Environment, F::STALL, B::Action),
    d("get-opinion", "Inst_GetOpinion", C::Environment, F::STALL, B::Input),
    d("join-group", "Inst_JoinGroup", C::Environment, F::STALL, B::Action),
    d("get-group-id", "Inst_GetGroupID", C::Environment, F::STALL, B::Input),
    d("get-faced-org-id", "Inst_GetFacedOrgID", C::Environment, F::STALL, B::Input),
    d("teach-offspring", "Inst_TeachOffspring", C::Environment, F::STALL, B::Action),
    d("learn-parent", "Inst_LearnParent", C::Environment, F::STALL, B::Action),
    d("modify-simp-display", "Inst_ModifySimpDisplay", C::Environment, F::STALL, B::Action),
    d("read-simp-display", "Inst_ReadLastSimpDisplay", C::Environment, F::STALL, B::Input),
    d("kill-display", "Inst_KillDisplay", C::Environment, F::STALL, B::Action),
    d("attack-prey", "Inst_AttackPrey", C::Environment, F::STALL, B::Action),
    d("attack-ft-prey", "Inst_AttackFTPrey", C::Environment, F::STALL, B::Action),
    d("scramble-registers", "Inst_ScrambleReg", C::Data, F::STALL, B::Input),
];

/// `cHardwareBCR::GetInstLib()`: built once from [`BCR_INST_DEFS`],
/// default instruction 0, null instruction 12.
pub fn bcr_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let entries = BCR_INST_DEFS
            .iter()
            .map(|d| InstLibEntry::new(d.name, d.class, d.flags, "", d.behav))
            .collect();
        let nops = (0..12)
            .map(|i| (format!("nop-{}", (b'A' + i as u8) as char), i))
            .collect();
        Arc::new(InstLib::new(entries, nops, 0, 12))
    })
    .clone()
}

/// The handler table, indexed like [`BCR_INST_DEFS`].
pub fn bcr_handler_table() -> &'static [Option<BcrInstFn>] {
    static TABLE: OnceLock<Vec<Option<BcrInstFn>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        BCR_INST_DEFS
            .iter()
            .map(|d| bcr_handler(d.method))
            .collect()
    })
}

fn bcr_handler(method: &str) -> Option<BcrInstFn> {
    let f: BcrInstFn = match method {
        "Inst_Repro" => inst_repro,
        "Inst_Divide" => inst_divide,
        _ => {
            return core_handler::<HardwareBcr>(method)
                .or_else(|| crate::hardware_bcr_insts_env::env_handler::<HardwareBcr>(method))
        }
    };
    Some(f)
}

/// Error if `is` (a BCR set) uses an instruction whose handler is not
/// ported.
pub fn check_bcr_inst_set_ported(is: &InstSet) -> Result<(), String> {
    let table = bcr_handler_table();
    for op in 0..is.size() {
        let idx = is.lib_function_index(Instruction::new(op)).max(0) as usize;
        if let Some(def) = BCR_INST_DEFS.get(idx) {
            if table.get(idx).copied().flatten().is_none() {
                return Err(format!(
                    "unsupported: instruction '{}' in instruction set '{}' (C++: cHardwareBCR.cc:cHardwareBCR::{})",
                    def.name,
                    is.inst_set_name(),
                    def.method
                ));
            }
        }
    }
    Ok(())
}

/// The CPU-core handlers both hardware types use, by `Inst_*` name.
pub(crate) fn core_handler<H: GeneHw>(method: &str) -> Option<GeneInstFn<H>> {
    let f: GeneInstFn<H> = match method {
        "Inst_Nop" => |_, _, _| Ok(true),
        "Inst_ThreadCreate" => inst_thread_create,
        "Inst_ThreadCancel" => |hw, _, _| {
            let c = hw.core_mut();
            if c.running_threads > 1 {
                let ct = c.ct();
                c.threads[ct].running = false;
                c.running_threads = c.running_threads.wrapping_sub(1) & c.counter_mask;
            }
            Ok(true)
        },
        "Inst_ThreadID" => |hw, _, _| {
            let c = hw.core_mut();
            let r = c.fmr(R_BX);
            let id = c.cur_thread as i32;
            c.set_reg(r, id, false);
            Ok(true)
        },
        "Inst_Yield" => |hw, _, _| {
            let c = hw.core_mut();
            let ct = c.ct();
            c.threads[ct].active = false;
            c.threads[ct].wait_reg = -1;
            Ok(true)
        },
        "Inst_RegulatePause" | "Inst_RegulatePauseSP" => {
            |hw, _, w| regulate(hw, w, Regulate::Pause)
        }
        "Inst_RegulateResume" | "Inst_RegulateResumeSP" => {
            |hw, _, w| regulate(hw, w, Regulate::Resume)
        }
        "Inst_RegulateReset" | "Inst_RegulateResetSP" => {
            |hw, _, w| regulate(hw, w, Regulate::Reset)
        }
        "Inst_Label" => |hw, _, w| {
            let mles = w.config().max_label_exe_size;
            hw.core_mut().read_label_ip(mles);
            Ok(true)
        },
        "Inst_IfNEqu" => |hw, _, w| if_two(hw, w, |a, b| a == b),
        "Inst_IfLess" => |hw, _, w| if_two(hw, w, |a, b| a >= b),
        "Inst_IfNotZero" => |hw, _, w| if_one(hw, w, |a| a == 0),
        "Inst_IfEqualZero" => |hw, _, w| if_one(hw, w, |a| a != 0),
        "Inst_IfGreaterThanZero" => |hw, _, w| if_one(hw, w, |a| a <= 0),
        "Inst_IfLessThanZero" => |hw, _, w| if_one(hw, w, |a| a >= 0),
        "Inst_IfGtrX" => |hw, _, w| if_x(hw, w, |b, x| b <= x),
        "Inst_IfEquX" => |hw, _, w| if_x(hw, w, |b, x| b != x),
        "Inst_Pop" => |hw, _, _| {
            let c = hw.core_mut();
            let r = c.fmr_ex(R_BX as i32, H::IMM);
            let pop = c.stack_pop();
            c.set_reg_src(r as usize, pop.value, &pop);
            Ok(true)
        },
        "Inst_Push" => |hw, _, w| {
            let r = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
            let v = hw.core().regv_ctx(w, r);
            let c = hw.core_mut();
            let cs = i32::from(c.threads[c.ct()].cur_stack);
            c.stack_mut(cs).push(v);
            Ok(true)
        },
        "Inst_PopAll" => |hw, _, _| {
            let c = hw.core_mut();
            let mut r = c.fmr(R_BX);
            for _ in 0..c.n {
                let pop = c.stack_pop();
                c.set_reg_src(r, pop.value, &pop);
                r += 1;
                if r == c.n {
                    r = 0;
                }
            }
            Ok(true)
        },
        "Inst_PushAll" => |hw, _, _| {
            let c = hw.core_mut();
            let mut r = c.fmr(R_BX);
            for _ in 0..c.n {
                let v = c.regv(r);
                let cs = i32::from(c.threads[c.ct()].cur_stack);
                c.stack_mut(cs).push(v);
                r += 1;
                if r == c.n {
                    r = 0;
                }
            }
            Ok(true)
        },
        "Inst_SwitchStack" => |hw, _, _| {
            hw.core_mut().switch_stack();
            Ok(true)
        },
        "Inst_SwapStackTop" => |hw, _, _| {
            let c = hw.core_mut();
            let v0 = c.stack_mut(0).pop();
            let v1 = c.stack_mut(1).pop();
            c.stack_mut(0).push(v1);
            c.stack_mut(1).push(v0);
            Ok(true)
        },
        "Inst_Swap" => |hw, _, _| {
            let c = hw.core_mut();
            let op1 = c.fmr(R_BX);
            let op2 = c.fmnr(op1);
            let ct = c.ct();
            c.threads[ct].reg.swap(op1, op2);
            Ok(true)
        },
        "Inst_CopyVal" => |hw, _, _| {
            let c = hw.core_mut();
            let op1 = c.fmr(R_BX);
            let op2 = c.fmnr(op1);
            let ct = c.ct();
            c.threads[ct].reg[op2] = c.threads[ct].reg[op1];
            Ok(true)
        },
        "Inst_ShiftR" => |hw, _, _| unary(hw, |v| v >> 1),
        "Inst_ShiftL" => |hw, _, _| unary(hw, |v| v.wrapping_shl(1)),
        "Inst_Inc" => |hw, _, _| unary(hw, |v| v.wrapping_add(1)),
        "Inst_Dec" => |hw, _, _| unary(hw, |v| v.wrapping_sub(1)),
        "Inst_Zero" => |hw, _, _| set_const(hw, 0),
        "Inst_One" => |hw, _, _| set_const(hw, 1),
        "Inst_MaxInt" => |hw, _, _| set_const(hw, i32::MAX),
        "Inst_Rand" => |hw, _, w| {
            let r = hw.core_mut().fmr(R_BX);
            let randsign = if w.rng().get_uint_range(0, 2) != 0 {
                -1
            } else {
                1
            };
            let v = w.rng().get_int(i32::MAX).wrapping_mul(randsign);
            hw.core_mut().set_reg(r, v, false);
            Ok(true)
        },
        "Inst_Add" => |hw, org, w| binary(hw, org, w, Bin::Add),
        "Inst_Sub" => |hw, org, w| binary(hw, org, w, Bin::Sub),
        "Inst_Mult" => |hw, org, w| binary(hw, org, w, Bin::Mult),
        "Inst_Div" => |hw, org, w| binary(hw, org, w, Bin::Div),
        "Inst_Mod" => |hw, org, w| binary(hw, org, w, Bin::Mod),
        "Inst_Nand" => |hw, org, w| binary(hw, org, w, Bin::Nand),
        "Inst_SetMemory" => inst_set_memory,
        "Inst_TaskIO" => inst_task_io,
        "Inst_TaskInput" => |hw, org, w| {
            let r = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
            let value_in = org.get_next_input(w);
            hw.core_mut().set_reg(r as usize, value_in, true);
            org.do_input(value_in);
            Ok(true)
        },
        "Inst_TaskOutput" => |hw, org, w| {
            let r = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
            let v = hw.core().regv_ctx(w, r).value;
            org_do_output(hw, org, w, Some(v), false)?;
            let c = hw.core_mut();
            c.last_output = c.cycle_count;
            Ok(true)
        },
        "Inst_MoveHead" => |hw, _, _| {
            let c = hw.core_mut();
            let head_used = c.fmh(H_IP);
            let target = c.fmh(H_FLOW);
            move_head(c, head_used, target);
            Ok(true)
        },
        "Inst_MoveHeadIfNEqu" => |hw, _, _| move_head_if(hw, |a, b| a != b),
        "Inst_MoveHeadIfLess" => |hw, _, _| move_head_if(hw, |a, b| a < b),
        "Inst_JumpHead" => |hw, _, w| {
            let head_used = hw.core_mut().fmh(H_IP);
            let r = hw.core_mut().fmr_ex(R_CX as i32, H::IMM);
            let jump = hw.core().reg_ctx(w, r);
            let c = hw.core_mut();
            let mut h = c.head(head_used);
            c.h_jump(&mut h, jump);
            *c.head_mut(head_used) = h;
            if head_used == H_IP {
                c.advance_ip = false;
            }
            Ok(true)
        },
        "Inst_GetHead" => |hw, _, _| {
            let c = hw.core_mut();
            let head_used = c.fmh(H_IP);
            let r = c.fmr(R_CX);
            let pos = c.head(head_used).pos;
            c.set_reg(r, pos, false);
            Ok(true)
        },
        "Inst_IfCopiedCompLabel" => |hw, _, w| if_copied(hw, w, true, false),
        "Inst_IfCopiedDirectLabel" => |hw, _, w| if_copied(hw, w, false, false),
        "Inst_IfCopiedCompSeq" => |hw, _, w| if_copied(hw, w, true, true),
        "Inst_IfCopiedDirectSeq" => |hw, _, w| if_copied(hw, w, false, true),
        "Inst_DidCopyCompLabel" => |hw, _, w| did_copy(hw, w, true, false),
        "Inst_DidCopyDirectLabel" | "Inst_DidCopyLabel" => |hw, _, w| did_copy(hw, w, false, false),
        "Inst_DidCopyCompSeq" => |hw, _, w| did_copy(hw, w, true, true),
        "Inst_DidCopyDirectSeq" => |hw, _, w| did_copy(hw, w, false, true),
        "Inst_DivideMemory" => |hw, org, w| {
            let r = hw.core_mut().fmr(R_BX);
            let c = hw.core();
            if r < R_BX || c.mem_ids[r] < 0 {
                return Ok(false);
            }
            let ms = i32::from(c.mem_ids[r]);
            let end_of_memory = c.mem_array[ms as usize].size() - 1;
            divide_main(hw, org, w, ms, end_of_memory, 1.0)
        },
        "Inst_HeadRead" => inst_head_read,
        "Inst_HeadWrite" => inst_head_write,
        "Inst_HeadCopy" => inst_head_copy,
        "Inst_Search_Label_Direct_S" | "Inst_Search_Label_S" => {
            |hw, _, w| search(hw, w, false, Dir::Start)
        }
        "Inst_Search_Label_Direct_F" => |hw, _, w| search(hw, w, false, Dir::Forward),
        "Inst_Search_Label_Direct_B" => |hw, _, w| search(hw, w, false, Dir::Backward),
        "Inst_Search_Label_Direct_D" | "Inst_Search_Label_D" => {
            |hw, _, w| search(hw, w, false, Dir::ByBx)
        }
        "Inst_Search_Seq_Comp_S" => |hw, _, w| search(hw, w, true, Dir::Start),
        "Inst_Search_Seq_Comp_F" => |hw, _, w| search(hw, w, true, Dir::Forward),
        "Inst_Search_Seq_Comp_B" => |hw, _, w| search(hw, w, true, Dir::Backward),
        "Inst_Search_Seq_Comp_D" | "Inst_Search_Seq_D" => |hw, _, w| search(hw, w, true, Dir::ByBx),
        "Inst_WaitCondition_Equal" => |hw, _, w| wait_cond(hw, w, Wait::Equal),
        "Inst_WaitCondition_Less" => |hw, _, w| wait_cond(hw, w, Wait::Less),
        "Inst_WaitCondition_Greater" => |hw, _, w| wait_cond(hw, w, Wait::Greater),
        "Inst_Die" => |_, org, w| {
            org.die(w);
            Ok(true)
        },
        _ => return None,
    };
    Some(f)
}

// ----------------------------------------------------------------------
// Threads
// ----------------------------------------------------------------------

/// `Inst_ThreadCreate`: start (or restart) the thread labelled by the
/// following nops at the flow head; faults at `MAX_CPU_THREADS`.
fn inst_thread_create<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    if hw.core().threads.len() as i32 >= w.config().max_cpu_threads {
        org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_ERROR, "");
        return Ok(false);
    }
    let mles = w.config().max_label_exe_size;
    let c = hw.core_mut();
    c.read_label_ip(mles);
    let label = c.label();
    let start = c.head(H_FLOW);
    c.thread_create(org, &label, start);
    Ok(true)
}

#[derive(Clone, Copy)]
enum Regulate {
    Pause,
    Resume,
    Reset,
}

/// `Inst_Regulate{Pause,Resume,Reset}[SP]`: act on every thread whose
/// last read label (`next_label`) contains the label that follows.
///
/// QUIRK: the iterator tests each thread's `next_label`, not its
/// `thread_label`, and a "specific" iterator also accepts containment, so
/// the SP and plain variants are the same.
fn regulate<H: GeneHw>(hw: &mut H, w: W<'_>, what: Regulate) -> R {
    let mles = w.config().max_label_exe_size;
    let c = hw.core_mut();
    c.read_label_ip(mles);
    let label = c.label();
    if label.get_size() == 0 {
        return Ok(false);
    }
    for t in 0..c.threads.len() {
        if !c.threads[t].next_label.contains(&label) {
            continue;
        }
        match what {
            Regulate::Pause => c.threads[t].running = false,
            Regulate::Resume => c.threads[t].running = true,
            Regulate::Reset => {
                let ip = c.threads[t].heads[H_IP];
                c.thread_reset(t, GeneHead::new(0, ip.ms, ip.is_gene));
            }
        }
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Conditionals and arithmetic
// ----------------------------------------------------------------------

/// `FindModifiedRegister(rBX[, true])` and the next register (from the
/// first when it is a register, else from `dflt`).
fn two_ops<H: GeneHw>(hw: &mut H, dflt: i32) -> (i32, i32) {
    let c = hw.core_mut();
    let op1 = c.fmr_ex(dflt, H::IMM);
    let base = if op1 < c.n as i32 { op1 } else { dflt };
    let op2 = c.fmnr_ex(base, H::IMM);
    (op1, op2)
}

/// `Inst_IfNEqu` / `Inst_IfLess`: skip unless the comparison of ?BX? and
/// the next register fails `skip`.
fn if_two<H: GeneHw>(hw: &mut H, w: W<'_>, skip: fn(i32, i32) -> bool) -> R {
    let (op1, op2) = two_ops(hw, R_BX as i32);
    let a = hw.core().reg_ctx(w, op1);
    let b = hw.core().reg_ctx(w, op2);
    if skip(a, b) {
        hw.core_mut().ip_advance();
    }
    Ok(true)
}

/// The single-operand conditionals.
fn if_one<H: GeneHw>(hw: &mut H, w: W<'_>, skip: fn(i32) -> bool) -> R {
    let op1 = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
    let a = hw.core().reg_ctx(w, op1);
    if skip(a) {
        hw.core_mut().ip_advance();
    }
    Ok(true)
}

/// `Inst_IfGtrX` / `Inst_IfEquX`: compare BX with 1, its sign flipped by
/// each nop-A of the label and shifted left by each other nop.
fn if_x<H: GeneHw>(hw: &mut H, w: W<'_>, skip: fn(i32, i32) -> bool) -> R {
    let mles = w.config().max_label_exe_size;
    let c = hw.core_mut();
    c.read_label_ip(mles);
    let label = c.label();
    let mut value_to_compare: i32 = 1;
    for i in 0..label.get_size() {
        let l = label.get(i);
        if l == R_AX as i32 {
            value_to_compare = value_to_compare.wrapping_mul(-1);
        } else {
            value_to_compare = value_to_compare.wrapping_shl(l as u32);
        }
    }
    if skip(c.reg(R_BX), value_to_compare) {
        c.ip_advance();
    }
    Ok(true)
}

/// `Inst_ShiftR`, `Inst_ShiftL`, `Inst_Inc`, `Inst_Dec` on ?BX?.
fn unary<H: GeneHw>(hw: &mut H, f: fn(i32) -> i32) -> R {
    let c = hw.core_mut();
    let r = c.fmr(R_BX);
    let src = c.regv(r);
    c.set_reg_src(r, f(src.value), &src);
    Ok(true)
}

/// `Inst_Zero`, `Inst_One`, `Inst_MaxInt`: ?BX? = `v`.
fn set_const<H: GeneHw>(hw: &mut H, v: i32) -> R {
    let c = hw.core_mut();
    let r = c.fmr(R_BX);
    c.set_reg(r, v, false);
    Ok(true)
}

#[derive(Clone, Copy)]
enum Bin {
    Add,
    Sub,
    Mult,
    Div,
    Mod,
    Nand,
}

/// The two-operand arithmetic: ?BX? = ?op1? (op) ?op2?.
///
/// `div` follows the original source: `INT_MIN / -1` faults ("div: Float
/// exception") and succeeds without a result. (The FFI-migrated reference
/// binary faults on `-INT_MAX / -1` instead and crashes on `INT_MIN /
/// -1`.) `mod` of `INT_MIN` by -1, undefined in C++, is 0.
fn binary<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, op: Bin) -> R {
    let dst = hw.core_mut().fmr(R_BX);
    let (op1, op2) = two_ops(hw, dst as i32);
    let r1 = hw.core().regv_ctx(w, op1);
    let r2 = hw.core().regv_ctx(w, op2);
    let (a, b) = (r1.value, r2.value);
    let v = match op {
        Bin::Add => a.wrapping_add(b),
        Bin::Sub => a.wrapping_sub(b),
        Bin::Mult => a.wrapping_mul(b),
        Bin::Nand => !(a & b),
        Bin::Div => match div_guard(a, b) {
            DivGuard::Ok => a.wrapping_div(b),
            DivGuard::Overflow => {
                org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: Float exception");
                return Ok(true);
            }
            DivGuard::Zero => {
                org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "div: dividing by 0");
                return Ok(false);
            }
        },
        Bin::Mod => {
            if div_guard(a, b) != DivGuard::Ok {
                org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "mod: modding by 0");
                return Ok(false);
            }
            a.wrapping_rem(b)
        }
    };
    hw.core_mut().set_reg_ops(dst, v, &r1, &r2);
    Ok(true)
}

// ----------------------------------------------------------------------
// Memory, input and output
// ----------------------------------------------------------------------

/// `Inst_SetMemory`: point the write head at the start of the memory space
/// labelled ?BX?, creating it (one instruction long) if need be.
fn inst_set_memory<H: GeneHw>(hw: &mut H, _org: &mut OrgState, _w: W<'_>) -> R {
    let c = hw.core_mut();
    let mem_label = c.fmr(R_BX);
    let mut mem_id = i32::from(c.mem_ids[mem_label]);
    if mem_id < 0 {
        mem_id = c.mem_array.len() as i32;
        c.mem_array.push(avida_genome::CpuMemory::new(1));
        c.mem_ids[mem_label] = mem_id as i8;
    }
    let mut h = GeneHead::new(0, mem_id as u32, false);
    c.h_adjust(&mut h);
    let ct = c.ct();
    c.threads[ct].heads[H_WRITE] = h;
    Ok(true)
}

/// `Inst_TaskIO`: output ?BX?, then input into it.
fn inst_task_io<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let r = hw.core_mut().fmr(R_BX);
    let v = hw.core().reg(r);
    org_do_output(hw, org, w, Some(v), false)?;
    let c = hw.core_mut();
    c.last_output = c.cycle_count;
    let value_in = org.get_next_input(w);
    hw.core_mut().set_reg(r, value_in, true);
    org.do_input(value_in);
    Ok(true)
}

// ----------------------------------------------------------------------
// Heads
// ----------------------------------------------------------------------

/// `getHead(head_used).Set(getHead(target))`. The read and write heads
/// may only be moved onto each other; any other move to them goes to the
/// flow head.
fn move_head(c: &mut crate::hardware_bcr::GeneCore, head_used: usize, target: usize) {
    let mut target = target;
    if target == H_WRITE && head_used != H_READ {
        target = H_FLOW;
    }
    if target == H_READ && head_used != H_WRITE {
        target = H_FLOW;
    }
    let t = c.head(target);
    *c.head_mut(head_used) = t;
    if head_used == H_IP {
        c.advance_ip = false;
    }
}

/// `Inst_MoveHeadIfNEqu` / `Inst_MoveHeadIfLess`.
fn move_head_if<H: GeneHw>(hw: &mut H, test: fn(i32, i32) -> bool) -> R {
    let c = hw.core_mut();
    let op1 = c.fmr(R_BX);
    let op2 = c.fmnr(op1);
    let head_used = c.fmh(H_IP);
    let target = c.fmh(H_FLOW);
    if test(c.reg(op1), c.reg(op2)) {
        move_head(c, head_used, target);
    }
    Ok(true)
}

/// `Inst_IfCopied*`: skip unless the label read (complemented with `comp`)
/// is the last label (or nop sequence, `seq`) copied.
fn if_copied<H: GeneHw>(hw: &mut H, w: W<'_>, comp: bool, seq: bool) -> R {
    let matched = copied_label_matches(hw, w, comp, seq);
    if !matched {
        hw.core_mut().ip_advance();
    }
    Ok(true)
}

/// `Inst_DidCopy*`: BX = whether the label read matches the last copied.
fn did_copy<H: GeneHw>(hw: &mut H, w: W<'_>, comp: bool, seq: bool) -> R {
    let matched = copied_label_matches(hw, w, comp, seq);
    hw.core_mut().set_reg(R_BX, i32::from(matched), false);
    Ok(true)
}

fn copied_label_matches<H: GeneHw>(hw: &mut H, w: W<'_>, comp: bool, seq: bool) -> bool {
    let mles = w.config().max_label_exe_size;
    let c = hw.core_mut();
    c.read_label_ip(mles);
    let n = c.n as i32;
    if comp {
        c.label_mut().rotate(1, n);
    }
    let t = &c.threads[c.ct()];
    if seq {
        t.next_label == t.read_seq
    } else {
        t.next_label == t.read_label
    }
}

/// `Inst_HeadRead`: ?AX? = the instruction at ?read head? (a random one on
/// a copy mutation); advance the head.
fn inst_head_read<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let c = hw.core_mut();
    let head_id = c.fmh(H_READ);
    let dst = c.fmr(R_AX);
    c.head_adjust(head_id);
    let read_inst = if org.mut_rates.test_copy_mut(w.rng()) {
        hw.core().is.random_inst(w.rng())
    } else {
        let c = hw.core();
        c.h_inst(&c.head(head_id))
    };
    let c = hw.core_mut();
    c.set_reg(dst, read_inst.op(), false);
    c.read_inst(read_inst);
    if c.slip_read_head && org.mut_rates.test_copy_slip(w.rng()) {
        let size = c.mem(&c.head(head_id)).size();
        let p = w.rng().get_int(size);
        let mut h = c.head(head_id);
        c.h_set_position(&mut h, p);
        *c.head_mut(head_id) = h;
    }
    c.head_advance(head_id);
    Ok(true)
}

/// Grow the memory `head` writes to by one (a copy of its last site) when
/// the head is on or past that last site.
fn grow_for_write(c: &mut crate::hardware_bcr::GeneCore, head: &GeneHead) {
    let pos = head.pos;
    let memory = c.mem_mut(head);
    if pos >= memory.size() - 1 {
        let size = memory.size();
        memory.resize(size + 1);
        if size >= 1 {
            memory.copy(size, size - 1);
        }
    }
}

/// `doSlipMutation(ctx, head.GetMemory(), head.Position())`.
fn slip_at_head<H: GeneHw>(hw: &mut H, w: W<'_>, head: &GeneHead) -> Result<(), String> {
    let is = hw.core().is.clone();
    let verbosity = w.verbosity();
    let cfg = w.config().clone();
    let pos = head.pos;
    let memory = hw.core_mut().mem_mut(head);
    do_slip_mutation(&is, &cfg, verbosity, w.rng(), memory, pos)
}

/// `Inst_HeadWrite`: write ?AX? (as an opcode, 0 when out of range) at
/// ?write head?, with insertion, deletion and slip copy mutations; advance.
fn inst_head_write<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let c = hw.core_mut();
    let head_id = c.fmh(H_WRITE);
    let src = c.fmr(R_AX);
    let mut active = c.head(head_id);
    grow_for_write(c, &active);
    c.h_adjust(&mut active);
    let mut value = c.reg(src);
    if value < 0 || value >= c.is.size() {
        value = 0;
    }
    c.h_set_inst(&active, Instruction::new(value));
    c.h_set_flag_copied(&active);
    *c.head_mut(head_id) = active;
    if org.mut_rates.test_copy_ins(w.rng()) {
        let inst = hw.core().is.random_inst(w.rng());
        hw.core_mut().h_insert_inst(&active, inst);
    }
    if org.mut_rates.test_copy_del(w.rng()) {
        hw.core_mut().h_remove_inst(&active);
    }
    if !hw.core().slip_read_head && org.mut_rates.test_copy_slip(w.rng()) {
        slip_at_head(hw, w, &active)?;
    }
    hw.core_mut().head_advance(head_id);
    Ok(true)
}

/// `Inst_HeadCopy`: copy from the read head to the write head (with copy
/// mutations), advancing both.
fn inst_head_copy<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let c = hw.core_mut();
    let write = c.head(H_WRITE);
    grow_for_write(c, &write);
    c.head_adjust(H_READ);
    c.head_adjust(H_WRITE);
    let read_head = c.head(H_READ);
    let write_head = c.head(H_WRITE);
    let mut read_inst = c.h_inst(&read_head);
    c.read_inst(read_inst);
    if org.mut_rates.test_copy_mut(w.rng()) {
        read_inst = hw.core().is.random_inst(w.rng());
        let c = hw.core_mut();
        c.h_set_flag_mutated(&write_head);
        c.h_set_flag_copy_mut(&write_head);
    }
    let c = hw.core_mut();
    c.h_set_inst(&write_head, read_inst);
    c.h_set_flag_copied(&write_head);
    if org.mut_rates.test_copy_ins(w.rng()) {
        let inst = hw.core().is.random_inst(w.rng());
        hw.core_mut().h_insert_inst(&write_head, inst);
    }
    if org.mut_rates.test_copy_del(w.rng()) {
        hw.core_mut().h_remove_inst(&write_head);
    }
    if org.mut_rates.test_copy_slip(w.rng()) {
        if hw.core().slip_read_head {
            let c = hw.core();
            let size = c.mem(&read_head).size();
            let p = w.rng().get_int(size);
            let c = hw.core_mut();
            let mut h = c.head(H_READ);
            c.h_set_position(&mut h, p);
            *c.head_mut(H_READ) = h;
        } else {
            slip_at_head(hw, w, &write_head)?;
        }
    }
    let c = hw.core_mut();
    c.head_advance(H_READ);
    c.head_advance(H_WRITE);
    Ok(true)
}

// ----------------------------------------------------------------------
// Searches
// ----------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Dir {
    Start,
    Forward,
    Backward,
    /// By the sign of BX: 0 start, < 0 backward, > 0 forward.
    ByBx,
}

/// The `Inst_Search_*` family: read the label that follows (complemented
/// for the nop-sequence searches), move the flow head to the match found
/// in its own memory (to the IP when there is none), then advance it.
fn search<H: GeneHw>(hw: &mut H, w: W<'_>, seq: bool, dir: Dir) -> R {
    let mles = w.config().max_label_exe_size;
    let c = hw.core_mut();
    c.read_label_ip(mles);
    if seq {
        let n = c.n as i32;
        c.label_mut().rotate(1, n);
    }
    let dir = match dir {
        Dir::ByBx => {
            let direction = c.reg(R_BX);
            if direction == 0 {
                Dir::Start
            } else if direction < 0 {
                Dir::Backward
            } else {
                Dir::Forward
            }
        }
        d => d,
    };
    let flow = c.head(H_FLOW);
    let ip = c.ip();
    let found = match (seq, dir) {
        (false, Dir::Start) => c.find_label_start(flow, ip, true, mles),
        (false, Dir::Forward) => c.find_label_forward(flow, ip, true, mles),
        (false, _) => c.find_label_backward(flow, ip),
        (true, Dir::Start) => c.find_nop_sequence_start(flow, ip, true, mles),
        (true, Dir::Forward) => c.find_nop_sequence_forward(flow, ip, true, mles),
        (true, _) => c.find_nop_sequence_backward(flow, ip),
    };
    *c.head_mut(H_FLOW) = found;
    c.head_advance(H_FLOW);
    Ok(true)
}

// ----------------------------------------------------------------------
// Waiting
// ----------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Wait {
    Equal,
    Less,
    Greater,
}

/// `Inst_WaitCondition_{Equal,Less,Greater}`: if another thread's ?LX?
/// (GP8: ?HX?, the last register) already meets the condition against
/// ?BX?, copy it into the destination register; otherwise sleep until a
/// thread writes such a value there, unless this is the last thread awake.
fn wait_cond<H: GeneHw>(hw: &mut H, w: W<'_>, cond: Wait) -> R {
    let c = hw.core_mut();
    let wait_value_reg = c.fmr_ex(R_BX as i32, H::IMM);
    let last = c.n - 1;
    let check_reg = c.fmr(last);
    let dst_default = if wait_value_reg < c.n as i32 {
        wait_value_reg as usize
    } else {
        R_BX
    };
    let wait_dst = c.fmr(dst_default);
    let wait_value = hw.core().reg_ctx(w, wait_value_reg);
    let c = hw.core_mut();
    let ct = c.ct();
    for i in 0..c.threads.len() {
        let v = c.threads[i].reg[check_reg];
        let met = match cond {
            Wait::Equal => v.value == wait_value,
            Wait::Less => v.value < wait_value,
            Wait::Greater => v.value > wait_value,
        };
        if i != ct && met {
            c.set_reg_src(wait_dst, v.value, &v);
            return Ok(true);
        }
    }
    if c.waiting_threads == c.running_threads {
        return Ok(false);
    }
    let mask = c.counter_mask;
    c.waiting_threads = c.waiting_threads.wrapping_add(1) & mask;
    let t = &mut c.threads[ct];
    t.active = false;
    t.wait_equal = matches!(cond, Wait::Equal);
    t.wait_less = matches!(cond, Wait::Less);
    t.wait_greater = matches!(cond, Wait::Greater);
    t.wait_reg = check_reg as i32;
    t.wait_value = wait_value;
    t.wait_dst = wait_dst as u32;
    Ok(true)
}

// ----------------------------------------------------------------------
// Reproduction
// ----------------------------------------------------------------------

/// `Inst_Divide` (BCR): divide off the memory space the write head is in,
/// up to the write head.
fn inst_divide(hw: &mut HardwareBcr, org: &mut OrgState, w: W<'_>) -> R {
    let c = hw.core_mut();
    let mut wh = c.head(H_WRITE);
    if wh.is_gene {
        return Ok(false);
    }
    c.h_adjust(&mut wh);
    *c.head_mut(H_WRITE) = wh;
    divide_main(hw, org, w, wh.ms as i32, wh.pos, 1.0)
}

/// The part of `Inst_Repro` after the hardware-specific checks: the
/// offspring is a copy of memory space 0 with copy and divide mutations.
pub(crate) fn repro_body<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let seq = hw.core().mem_array[0].to_sequence();
    org.offspring_genome = make_offspring(hw, seq);
    let memory = &hw.core().mem_array[0];
    org.phenotype.child_copied_size = memory.size();
    let lines_executed = (0..memory.size())
        .filter(|&i| memory.flag_executed(i))
        .count() as i32;
    org.phenotype.executed_size = lines_executed;

    let is = hw.base().inst_set.clone();
    if org.mut_rates.copy.mut_prob > 0.0 {
        let n = org.offspring_genome.representation().size();
        for i in 0..n {
            if org.mut_rates.test_copy_mut(w.rng()) {
                let r = is.random_inst(w.rng());
                org.offspring_genome.representation_mut().set(i, r);
            }
        }
    }
    divide_do_mutations(&is, org, w, &mut hw.core_mut().mem_array[0], 1.0, i32::MAX)?;

    let parent_size = org.initial_genome.representation().size();
    let child_size = org.offspring_genome.representation().size();
    if !check_viable(hw, org, w, parent_size, child_size, true) {
        return Ok(false);
    }
    divide_test_fitness_measures(org, w, false);
    hw.base_mut().reset_ft_costs();
    if w.config().divide_method == DIVIDE_METHOD_SPLIT {
        hw.core_mut().advance_ip = false;
    }
    let parent_alive = crate::hardware_bcr::activate_divide(hw, org, w)?;
    if parent_alive && w.config().divide_method == DIVIDE_METHOD_SPLIT {
        full_reset(hw, org, w);
    }
    Ok(true)
}

/// `cHardwareBCR::Inst_Repro`: needs `REQUIRED_BONUS`.
pub(crate) fn inst_repro(hw: &mut HardwareBcr, org: &mut OrgState, w: W<'_>) -> R {
    if org.phenotype.cur_bonus < w.config().required_bonus {
        return Ok(false);
    }
    repro_body(hw, org, w)
}
