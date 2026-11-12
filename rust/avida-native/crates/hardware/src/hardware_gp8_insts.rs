//! The `cHardwareGP8` instruction library (`cHardwareGP8::initInstLib`,
//! avida-core/source/cpu/cHardwareGP8.cc at 47f13dadb) and the handlers
//! that are the GP8's own: the queued actions (`move`, `rotate-x`, `eat`),
//! `rotate-home` and `repro`. Everything else is the code it shares with
//! the BCR ([`crate::hardware_bcr_insts`], [`crate::hardware_bcr_insts_env`])
//! run with [`GeneHw::IMM`] set: operands may be the immediate-value
//! instructions `zero`, `one`, `maxint` and `rand`.

use crate::hardware_bcr::{GeneHw, Imm, R_BX};
use crate::hardware_bcr_insts::{core_handler, repro_body, R, W};
use crate::hardware_bcr_insts_env::{env_handler, home_facing, rotate_until};
use crate::hardware_gp8::{HardwareGp8, A_EAT, A_MOVE, A_ROTATE};
use crate::organism::OrgState;
use avida_genome::inst_lib::{
    inst_flag as F, BehavClass, InstLib, InstLibEntry, InstructionClass as C,
};
use avida_genome::{InstSet, Instruction};
use std::sync::{Arc, OnceLock};

/// A `cHardwareGP8` instruction handler.
pub type Gp8InstFn =
    fn(&mut HardwareGp8, &mut OrgState, &mut dyn crate::org_interface::OrgInterface) -> R;

/// `uREAD`
pub const U_READ: u32 = 0x1;
/// `uWRITE`
pub const U_WRITE: u32 = 0x2;
/// `uATTACK`
pub const U_ATTACK: u32 = 0x4;

/// One `GP8Inst` (`INST` / `INSTI`).
#[derive(Debug, Clone, Copy)]
pub struct Gp8InstDef {
    /// Instruction name.
    pub name: &'static str,
    /// The `cHardwareGP8::Inst_*` handler.
    pub method: &'static str,
    /// `InstructionClass`
    pub class: C,
    /// `nInstFlag` bits
    pub flags: u32,
    /// The hardware units the instruction occupies for the rest of the
    /// cycle.
    pub units: u32,
    /// The `Val_*` method of an immediate-value instruction.
    pub imm: Option<Imm>,
}

const fn d(
    name: &'static str,
    method: &'static str,
    class: C,
    flags: u32,
    units: u32,
) -> Gp8InstDef {
    Gp8InstDef {
        name,
        method,
        class,
        flags,
        units,
        imm: None,
    }
}

const fn di(name: &'static str, method: &'static str, imm: Imm) -> Gp8InstDef {
    Gp8InstDef {
        name,
        method,
        class: C::ArithmeticLogic,
        flags: F::IMMEDIATE_VALUE,
        units: 0,
        imm: Some(imm),
    }
}

const STALL: u32 = F::STALL;

/// `s_f_array`, in order: the eight nops, `NULL` (index 8, the null
/// instruction), then the rest. QUIRK: `h-copy` declares the units
/// `uREAD & uWRITE`, which is 0, so it never stalls on `h-read` or
/// `h-write`.
#[rustfmt::skip]
pub static GP8_INST_DEFS: &[Gp8InstDef] = &[
    d("nop-A", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-B", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-C", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-D", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-E", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-F", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-G", "Inst_Nop", C::Nop, F::NOP, 0),
    d("nop-H", "Inst_Nop", C::Nop, F::NOP, 0),
    d("NULL", "Inst_Nop", C::Nop, 0, 0),
    d("nop-X", "Inst_Nop", C::Nop, 0, 0),
    d("promoter", "Inst_Nop", C::FlowControl, F::PROMOTER, 0),
    d("terminator", "Inst_Nop", C::FlowControl, F::TERMINATOR, 0),
    d("regulate-pause", "Inst_RegulatePause", C::Other, 0, 0),
    d("regulate-resume", "Inst_RegulateResume", C::Other, 0, 0),
    d("regulate-reset", "Inst_RegulateReset", C::Other, 0, 0),
    d("wait-cond-equ", "Inst_WaitCondition_Equal", C::Other, 0, 0),
    d("wait-cond-less", "Inst_WaitCondition_Less", C::Other, 0, 0),
    d("wait-cond-gtr", "Inst_WaitCondition_Greater", C::Other, 0, 0),
    d("yield", "Inst_Yield", C::Other, 0, 0),
    d("set-memory", "Inst_SetMemory", C::FlowControl, 0, 0),
    d("mov-head", "Inst_MoveHead", C::FlowControl, 0, 0),
    d("jmp-head", "Inst_JumpHead", C::FlowControl, 0, 0),
    d("get-head", "Inst_GetHead", C::FlowControl, 0, 0),
    d("label", "Inst_Label", C::FlowControl, F::LABEL, 0),
    d("search-lbl-s", "Inst_Search_Label_S", C::FlowControl, 0, 0),
    d("search-lbl-d", "Inst_Search_Label_D", C::FlowControl, 0, 0),
    d("search-seq-d", "Inst_Search_Seq_D", C::FlowControl, 0, 0),
    d("if-n-equ", "Inst_IfNEqu", C::Conditional, 0, 0),
    d("if-less", "Inst_IfLess", C::Conditional, 0, 0),
    d("if-not-0", "Inst_IfNotZero", C::Conditional, 0, 0),
    d("if-equ-0", "Inst_IfEqualZero", C::Conditional, 0, 0),
    d("if-gtr-0", "Inst_IfGreaterThanZero", C::Conditional, 0, 0),
    d("if-less-0", "Inst_IfLessThanZero", C::Conditional, 0, 0),
    d("shift-r", "Inst_ShiftR", C::ArithmeticLogic, 0, 0),
    d("shift-l", "Inst_ShiftL", C::ArithmeticLogic, 0, 0),
    d("inc", "Inst_Inc", C::ArithmeticLogic, 0, 0),
    d("dec", "Inst_Dec", C::ArithmeticLogic, 0, 0),
    d("add", "Inst_Add", C::ArithmeticLogic, 0, 0),
    d("sub", "Inst_Sub", C::ArithmeticLogic, 0, 0),
    d("nand", "Inst_Nand", C::ArithmeticLogic, 0, 0),
    d("mult", "Inst_Mult", C::ArithmeticLogic, 0, 0),
    d("div", "Inst_Div", C::ArithmeticLogic, 0, 0),
    d("mod", "Inst_Mod", C::ArithmeticLogic, 0, 0),
    di("zero", "Inst_Zero", Imm::Zero),
    di("one", "Inst_One", Imm::One),
    di("maxint", "Inst_MaxInt", Imm::MaxInt),
    di("rand", "Inst_Rand", Imm::Rand),
    d("pop", "Inst_Pop", C::Data, 0, 0),
    d("push", "Inst_Push", C::Data, 0, 0),
    d("pop-all", "Inst_PopAll", C::Data, 0, 0),
    d("push-all", "Inst_PushAll", C::Data, 0, 0),
    d("swap-stk", "Inst_SwitchStack", C::Data, 0, 0),
    d("swap", "Inst_Swap", C::Data, 0, 0),
    d("input", "Inst_TaskInput", C::Environment, STALL, 0),
    d("output", "Inst_TaskOutput", C::Environment, STALL, 0),
    d("h-read", "Inst_HeadRead", C::Lifecycle, 0, U_READ),
    d("h-write", "Inst_HeadWrite", C::Lifecycle, 0, U_WRITE),
    d("h-copy", "Inst_HeadCopy", C::Lifecycle, 0, U_READ & U_WRITE),
    d("divide-memory", "Inst_DivideMemory", C::Lifecycle, STALL, 0),
    d("did-copy-lbl", "Inst_DidCopyLabel", C::Other, 0, 0),
    d("repro", "Inst_Repro", C::Lifecycle, STALL, 0),
    d("die", "Inst_Die", C::Lifecycle, STALL, 0),
    d("sg-move", "Inst_SGMove", C::Environment, 0, 0),
    d("sg-rotate-l", "Inst_SGRotateL", C::Environment, 0, 0),
    d("sg-rotate-r", "Inst_SGRotateR", C::Environment, 0, 0),
    d("sg-sense", "Inst_SGSense", C::Environment, 0, 0),
    d("move", "Inst_Move", C::Environment, STALL, 0),
    d("get-north-offset", "Inst_GetNorthOffset", C::Environment, 0, 0),
    d("rotate-x", "Inst_RotateX", C::Environment, STALL, 0),
    d("rotate-home", "Inst_RotateHome", C::Environment, STALL, 0),
    d("rotate-org-id", "Inst_RotateOrgID", C::Environment, STALL, 0),
    d("rotate-away-org-id", "Inst_RotateAwayOrgID", C::Environment, STALL, 0),
    d("set-forage-target", "Inst_SetForageTarget", C::Environment, STALL, 0),
    d("set-ft-once", "Inst_SetForageTargetOnce", C::Environment, STALL, 0),
    d("set-rand-ft-once", "Inst_SetRandForageTargetOnce", C::Environment, STALL, 0),
    d("get-forage-target", "Inst_GetForageTarget", C::Environment, 0, 0),
    d("sense-resource-id", "Inst_SenseResourceID", C::Environment, STALL, 0),
    d("sense-nest", "Inst_SenseNest", C::Environment, STALL, 0),
    d("sense-faced-habitat", "Inst_SenseFacedHabitat", C::Environment, STALL, 0),
    d("look-ahead-ex", "Inst_LookAheadEX", C::Environment, STALL, 0),
    d("look-around-ex", "Inst_LookAroundEX", C::Environment, STALL, 0),
    d("look-again-ex", "Inst_LookAgainEX", C::Environment, STALL, 0),
    d("eat", "Inst_Eat", C::Environment, STALL, 0),
    d("collect-specific", "Inst_CollectSpecific", C::Environment, STALL, 0),
    d("get-res-stored", "Inst_GetResStored", C::Environment, STALL, 0),
    d("get-faced-org-id", "Inst_GetFacedOrgID", C::Environment, STALL, 0),
    d("teach-offspring", "Inst_TeachOffspring", C::Environment, 0, 0),
    d("learn-parent", "Inst_LearnParent", C::Environment, STALL, 0),
    d("attack-prey", "Inst_AttackPrey", C::Environment, STALL, U_ATTACK),
    d("scramble-registers", "Inst_ScrambleReg", C::Data, 0, 0),
];

/// `cHardwareGP8::GetInstLib()`: default instruction 0, null instruction 8.
pub fn gp8_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let entries = GP8_INST_DEFS
            .iter()
            .map(|d| InstLibEntry::new(d.name, d.class, d.flags, "", BehavClass::None))
            .collect();
        let nops = (0..8)
            .map(|i| (format!("nop-{}", (b'A' + i as u8) as char), i))
            .collect();
        Arc::new(InstLib::new(entries, nops, 0, 8))
    })
    .clone()
}

/// `m_imm_methods`, by library index.
pub fn gp8_imm_table() -> &'static [Option<Imm>] {
    static TABLE: OnceLock<Vec<Option<Imm>>> = OnceLock::new();
    TABLE.get_or_init(|| GP8_INST_DEFS.iter().map(|d| d.imm).collect())
}

/// `m_hw_units`, by library index.
pub fn gp8_unit_table() -> &'static [u32] {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    TABLE.get_or_init(|| GP8_INST_DEFS.iter().map(|d| d.units).collect())
}

/// The handler table, indexed like [`GP8_INST_DEFS`].
pub fn gp8_handler_table() -> &'static [Option<Gp8InstFn>] {
    static TABLE: OnceLock<Vec<Option<Gp8InstFn>>> = OnceLock::new();
    TABLE.get_or_init(|| {
        GP8_INST_DEFS
            .iter()
            .map(|d| gp8_handler(d.method))
            .collect()
    })
}

fn gp8_handler(method: &str) -> Option<Gp8InstFn> {
    let f: Gp8InstFn = match method {
        "Inst_Repro" => inst_repro,
        "Inst_Move" => inst_move,
        "Inst_Eat" => inst_eat,
        "Inst_RotateX" => inst_rotate_x,
        "Inst_RotateHome" => inst_rotate_home,
        _ => {
            return core_handler::<HardwareGp8>(method)
                .or_else(|| env_handler::<HardwareGp8>(method))
        }
    };
    Some(f)
}

/// Error if `is` (a GP8 set) uses an instruction whose handler is not
/// ported.
pub fn check_gp8_inst_set_ported(is: &InstSet) -> Result<(), String> {
    let table = gp8_handler_table();
    for op in 0..is.size() {
        let idx = is.lib_function_index(Instruction::new(op)).max(0) as usize;
        if let Some(def) = GP8_INST_DEFS.get(idx) {
            if table.get(idx).copied().flatten().is_none() {
                return Err(format!(
                    "unsupported: instruction '{}' in instruction set '{}' (C++: cHardwareGP8.cc:cHardwareGP8::{})",
                    def.name,
                    is.inst_set_name(),
                    def.method
                ));
            }
        }
    }
    Ok(())
}

/// Queue `action` for the end of the cycle unless it already is.
fn queue(hw: &mut HardwareGp8, action: u8) {
    let flag = match action {
        A_EAT => &mut hw.hw_queue_eat,
        A_MOVE => &mut hw.hw_queue_move,
        _ => &mut hw.hw_queue_rotate,
    };
    if !*flag {
        *flag = true;
        if let Some(slot) = hw.hw_queue.get_mut(hw.hw_queued) {
            *slot = action;
        }
        hw.hw_queued += 1;
    }
}

/// `Inst_Move`: queue a move (fails in the test CPU and, with
/// `JUV_PERIOD`, for juveniles).
fn inst_move(hw: &mut HardwareGp8, org: &mut OrgState, w: W<'_>) -> R {
    if w.get_cell_id() == -1 {
        return Ok(false);
    }
    if hw.juv_enabled && org.phenotype.time_used < w.config().juv_period {
        return Ok(false);
    }
    queue(hw, A_MOVE);
    Ok(true)
}

/// `Inst_Eat`: queue the eat action (an output of 0 at the end of the
/// cycle) and put this thread to sleep until then; the bonus instructions
/// the output triggers run on every thread that ate.
fn inst_eat(hw: &mut HardwareGp8, _org: &mut OrgState, _w: W<'_>) -> R {
    queue(hw, A_EAT);
    let cur = hw.core.cur_thread;
    hw.hw_queue_eat_threads.push(cur);
    let ct = hw.core.ct();
    hw.core.threads[ct].active = false;
    hw.core.threads[ct].wait_reg = -1;
    Ok(true)
}

/// `Inst_RotateX`: queue a turn of ?BX? steps (sign = direction, modulo
/// 8). Fails without neighbours.
fn inst_rotate_x(hw: &mut HardwareGp8, _org: &mut OrgState, w: W<'_>) -> R {
    let num_neighbors = if hw.core.use_avatar != 0 {
        w.get_av_num_neighbors(0)
    } else {
        w.get_num_neighbors()
    };
    if num_neighbors == 0 {
        return Ok(false);
    }
    let reg_used = hw.core.fmr_ex(R_BX as i32, true);
    let mut rot_num = hw.core.reg_ctx(w, reg_used);
    hw.hw_queue_rotate_reverse = rot_num < 0;
    rot_num = rot_num.wrapping_abs();
    if rot_num > 7 {
        rot_num %= 8;
    }
    // `m_hw_queue_rotate_num:3`
    hw.hw_queue_rotate_num = (rot_num as u32) & 7;
    queue(hw, A_ROTATE);
    Ok(true)
}

/// `Inst_RotateHome` (GP8): face the birth cell; nothing for the zero
/// vector.
fn inst_rotate_home(hw: &mut HardwareGp8, org: &mut OrgState, w: W<'_>) -> R {
    let correct_facing = home_facing(org.northerly, org.easterly);
    if correct_facing < 0 {
        return Ok(true);
    }
    let rotates = if hw.core.use_avatar != 0 {
        w.get_av_num_neighbors(0)
    } else {
        w.get_num_neighbors()
    };
    rotate_until(hw, w, rotates, correct_facing);
    Ok(true)
}

/// `cHardwareGP8::Inst_Repro`: the organism-level viability test first (a
/// failure resets the hardware under `DIVIDE_FAILURE_RESETS`), then
/// `MIN_CYCLES`.
pub(crate) fn inst_repro(hw: &mut HardwareGp8, org: &mut OrgState, w: W<'_>) -> R {
    if !org.divide_check_viable(w) {
        if w.config().divide_failure_resets != 0 {
            let cfg = w.config().clone();
            GeneHw::internal_reset(hw, org, &cfg);
        }
        return Ok(false);
    }
    if org.phenotype.time_used < w.config().min_cycles {
        return Ok(false);
    }
    repro_body(hw, org, w)
}
