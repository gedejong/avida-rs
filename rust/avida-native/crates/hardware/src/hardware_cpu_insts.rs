//! The `cHardwareCPU::Inst_*` handlers (avida-core/source/cpu/cHardwareCPU.cc
//! at 47f13dadb) and the method-name -> handler table.
//!
//! Every handler has the [`InstFn`] shape: the hardware, the organism's
//! state and the world. A handler returns `Ok(success)` as the C++ returns
//! its `bool`; `Err` is reserved for an unported path reached at run time
//! and for C++ `exit()` calls.
//!
//! Handlers whose method is not matched in [`handler`] are registered but
//! unsupported: loading an instruction set that uses one fails loudly (see
//! [`crate::hardware_manager::check_inst_set_ported`]).

use crate::cpu_inst_lib::CPU_INST_DEFS;
use crate::hardware::{
    div_guard, divide_do_mutations, divide_test_fitness_measures, do_slip_mutation,
    do_uniform_copy_mutation, energy_usage_inst, in_no_mut_list, DivGuard, Hardware,
};
use crate::hardware_cpu::{HardwareCpu, NUM_NOPS, NUM_REGISTERS, REG_AX, REG_BX, REG_CX};
use crate::head_cpu::HeadCpu;
use crate::nhardware::*;
use crate::org_interface::{with_neighbor_state, Faced, OrgInterface};
use crate::organism::{OrgState, Organism};
use avida_genome::Instruction;
use std::sync::OnceLock;

/// A `cHardwareCPU` instruction handler.
pub type InstFn =
    fn(&mut HardwareCpu, &mut OrgState, &mut dyn OrgInterface) -> Result<bool, String>;

pub(crate) type R = Result<bool, String>;

/// `nGeometry::GRID`
pub(crate) const GEOMETRY_GRID: i32 = 1;
/// `nGeometry::TORUS`
pub(crate) const GEOMETRY_TORUS: i32 = 2;

/// `cHardwareBase::MASK_SIGNBIT`
pub(crate) const MASK_SIGNBIT: u32 = 0x7FFF_FFFF;
/// `cHardwareBase::MASK24`
pub(crate) const MASK24: u32 = 0x00FF_FFFF;
/// `CONSENSUS` (cHardwareCPU.cc): half the bits of an int.
pub(crate) const CONSENSUS: u32 = 16;
/// `CONSENSUS24`
pub(crate) const CONSENSUS24: u32 = 12;

/// `cHardwareCPU::BitCount(value)`.
#[inline]
pub(crate) fn bit_count(value: u32) -> u32 {
    let w = value.wrapping_sub((value >> 1) & 0x5555_5555);
    let x = (w & 0x3333_3333).wrapping_add((w >> 2) & 0x3333_3333);
    (x.wrapping_add(x >> 4) & 0x0F0F_0F0F).wrapping_mul(0x0101_0101) >> 24
}

/// `AvidaTools::Mod(value, base)`.
#[inline]
pub(crate) fn avida_mod(value: i32, base: i32) -> i32 {
    if base == 0 {
        return 0;
    }
    let v = value.wrapping_rem(base);
    if v < 0 {
        v + base
    } else {
        v
    }
}

// ----------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------

impl HardwareCpu {
    /// `ReadLabel()` with `MAX_LABEL_EXE_SIZE` from the world's config.
    #[inline]
    pub(crate) fn rl(&mut self, world: &dyn OrgInterface) {
        let m = world.config().max_label_exe_size;
        self.read_label(m);
    }

    /// `GetLabel().Rotate(1, NUM_NOPS)`.
    #[inline]
    pub(crate) fn rotate_label(&mut self) {
        self.label_mut().rotate(1, NUM_NOPS);
    }

    /// `if (cond) getIP().Advance();`
    #[inline]
    pub(crate) fn skip_if(&mut self, cond: bool) {
        if cond {
            self.ip_advance();
        }
    }

    /// Whether `inst` names an opcode of this set (opcode 255 does not).
    #[inline]
    pub(crate) fn valid_inst(&self, inst: Instruction) -> bool {
        inst.op() < self.base.inst_set.size()
    }

    /// `m_inst_set->GetName(inst) == name`; the error opcode matches nothing.
    pub(crate) fn inst_named(&self, inst: Instruction, name: &str) -> bool {
        self.valid_inst(inst) && self.base.inst_set.name_of(inst) == name
    }

    /// `Find(instr)`: the first `instr` after the IP, wrapping around.
    ///
    /// QUIRK: the search advances twice before its first test, so the
    /// instruction right after the IP is never considered, while the IP
    /// itself is tested last.
    pub(crate) fn find_inst(&self, name: &str) -> HeadCpu {
        let size = self.memory.size();
        let mut ptr = self.ip_head();
        let current = ptr.position();
        ptr.advance(size, 1);
        while ptr.position() != current {
            ptr.advance(size, 1);
            if self.inst_named(ptr.get_inst(&self.memory), name) {
                break;
            }
        }
        ptr
    }

    /// `Else_TopHalf()`.
    pub(crate) fn else_top_half(&mut self) {
        let else_head = self.find_inst("else");
        let endif_head = self.find_inst("endif");
        let ip_pos = self.ip_head().position();
        if else_head.position() != ip_pos {
            self.head_set_from(HEAD_IP, &else_head);
        } else if endif_head.position() != ip_pos {
            self.head_set_from(HEAD_IP, &endif_head);
        } else {
            self.ip_advance();
        }
    }

    /// `m_organism->TestCopyMut(ctx)`.
    #[inline]
    pub(crate) fn test_copy_mut(org: &OrgState, world: &mut dyn OrgInterface) -> bool {
        org.mut_rates.test_copy_mut(world.rng())
    }

    /// `cOrganism::DoOutput(ctx, value)` plus its bonus instructions.
    #[inline]
    pub(crate) fn output(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        value: i32,
    ) -> Result<(), String> {
        self.org_do_output(org, world, Some(value), false)
    }

    /// `cOrganism::GetNextInput()` followed by nothing else.
    #[inline]
    pub(crate) fn next_input(org: &mut OrgState, world: &mut dyn OrgInterface) -> i32 {
        org.get_next_input(world)
    }

    /// `Divide_DoTransposons(ctx)`.
    ///
    /// QUIRK: the C++ caches `transposon_in_use` and the transposon opcode
    /// in function statics, i.e. from the instruction set of the first
    /// organism that ever reproduces with `repro`. The port looks them up
    /// in this organism's set, which is the same for single-set runs.
    fn divide_do_transposons(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        if !self.base.inst_set.inst_in_set("transposon") {
            return Ok(());
        }
        let transposon_inst = self.base.inst_set.get_inst("transposon")?;
        let mut tr_count = 0;
        for i in 0..self.memory.size() {
            if self.memory.flag_executed(i) && self.memory[i] == transposon_inst {
                tr_count += 1;
            }
        }
        for _ in 0..tr_count {
            if world.rng().p(0.01) {
                let size = org.offspring_genome.representation().size();
                let mut_line = world.rng().get_uint((size + 1) as u32) as i32;
                let seq = org.offspring_genome.representation_mut();
                if mut_line >= 0 && mut_line <= seq.size() {
                    seq.insert(mut_line, transposon_inst);
                }
            }
        }
        Ok(())
    }

    /// `Inst_Repro(ctx)`: reproduce by copying the genome wholesale.
    pub(crate) fn inst_repro(&mut self, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
        if world.config().repro_method == 0 && world.is_neighbor_cell_occupied() {
            return Ok(false);
        }
        if org.phenotype.cur_bonus < world.config().required_bonus {
            return Ok(false);
        }

        org.offspring_genome = org.initial_genome.clone();
        self.divide_do_transposons(org, world)?;

        let is = self.base.inst_set.clone();
        if org.mut_rates.copy.mut_prob > 0.0 {
            let n = org.offspring_genome.representation().size();
            for i in 0..n {
                let inst = org.offspring_genome.representation().get(i);
                let in_list = in_no_mut_list(world.config(), inst);
                if Self::test_copy_mut(org, world) && !in_list {
                    let r = is.random_inst(world.rng());
                    org.offspring_genome.representation_mut().set(i, r);
                }
            }
        }

        divide_do_mutations(&is, org, world, &mut self.memory, 1.0, i32::MAX)?;

        let parent_size = org.initial_genome.representation().size();
        let child_size = org.offspring_genome.representation().size();
        if !self.check_viable(org, world, parent_size, child_size, true) {
            return Ok(false);
        }

        divide_test_fitness_measures(org, world, false);

        let divide_method = world.config().divide_method;
        if divide_method != DIVIDE_METHOD_OFFSPRING {
            self.base.reset_ft_costs();
        }
        if divide_method == DIVIDE_METHOD_SPLIT {
            self.advance_ip = false;
        }
        let parent_alive = self.activate_divide(org, world)?;
        if parent_alive {
            if world.config().divide_method == DIVIDE_METHOD_SPLIT {
                self.full_reset(org, world);
            }
            self.memory.clear_flags();
        }
        Ok(true)
    }

    /// `Inst_HeadDivideMut(ctx, mut_multiplier)`.
    fn head_divide_mut(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        mut_multiplier: f64,
    ) -> R {
        self.adjust_heads();
        let divide_pos = self.get_head(HEAD_READ).position();
        let mut child_end = self.get_head(HEAD_WRITE).position();
        if child_end == 0 {
            child_end = self.memory.size();
        }
        let extra_lines = self.memory.size() - child_end;
        let ret_val = self.divide_main(org, world, divide_pos, extra_lines, mut_multiplier)?;
        self.adjust_heads();
        if world.config().tolerance_window != 0 && ret_val {
            let v = i32::from(org.phenotype.born_parent_group);
            self.set_reg(REG_BX, v);
        }
        Ok(ret_val)
    }

    /// `Inst_HeadDivideRS` / `1RS` / `2RS`.
    fn head_divide_rs(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        variant: i32,
    ) -> R {
        self.adjust_heads();
        let divide_pos = self.get_head(HEAD_READ).position();
        let mut child_end = self.get_head(HEAD_WRITE).position();
        if child_end == 0 {
            child_end = self.memory.size();
        }
        let extra_lines = self.memory.size() - child_end;
        let ret_val = self.divide_main_rs(org, world, divide_pos, extra_lines, 1.0, variant)?;
        self.adjust_heads();
        Ok(ret_val)
    }

    /// `HeadCopy_ErrorCorrect(ctx, reduction)`.
    fn head_copy_error_correct(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        reduction: f64,
    ) -> R {
        let size = self.memory.size();
        self.head_ref(HEAD_READ).adjust(size, 1);
        self.head_ref(HEAD_WRITE).adjust(size, 1);
        let read_head = self.get_head(HEAD_READ);
        let write_head = self.get_head(HEAD_WRITE);

        let mut read_inst = read_head.get_inst(&self.memory);
        self.read_inst(read_inst.op());
        let mr = org.mut_rates.copy;
        if world.rng().p(mr.mut_prob / reduction)
            && !in_no_mut_list(world.config(), read_head.get_inst(&self.memory))
        {
            read_inst = self.base.inst_set.random_inst(world.rng());
            write_head.set_flag_mutated(&mut self.memory);
            write_head.set_flag_copy_mut(&mut self.memory);
        }
        write_head.set_inst(&mut self.memory, read_inst);
        write_head.set_flag_copied(&mut self.memory);

        if world.rng().p(mr.ins_prob / reduction) {
            let inst = self.base.inst_set.random_inst(world.rng());
            write_head.insert_inst(&mut self.memory, inst);
        }
        if world.rng().p(mr.del_prob / reduction) {
            write_head.remove_inst(&mut self.memory);
        }
        if world.rng().p(mr.uniform_prob / reduction) {
            self.uniform_copy_mutation(world, &write_head);
        }
        if world.rng().p(mr.slip_prob / reduction) {
            if self.slip_read_head {
                let pos = world.rng().get_int(self.memory.size());
                self.head_set_pos(HEAD_READ, pos);
            } else {
                self.slip_mutation(world, write_head.position())?;
            }
        }
        self.head_advance(HEAD_READ);
        self.head_advance(HEAD_WRITE);
        Ok(true)
    }

    /// `doUniformCopyMutation(ctx, head)` on this memory.
    fn uniform_copy_mutation(&mut self, world: &mut dyn OrgInterface, head: &HeadCpu) {
        let cfg = world.config().clone();
        let is = self.base.inst_set.clone();
        do_uniform_copy_mutation(&is, &cfg, world.rng(), head, &mut self.memory);
    }

    /// `doSlipMutation(ctx, m_memory, from)`.
    fn slip_mutation(&mut self, world: &mut dyn OrgInterface, from: i32) -> Result<(), String> {
        let cfg = world.config().clone();
        let verbosity = world.verbosity();
        let is = self.base.inst_set.clone();
        do_slip_mutation(&is, &cfg, verbosity, world.rng(), &mut self.memory, from)
    }

    /// The label search of `Inst_RotateL` / `Inst_RotateR`: whether the
    /// faced organism's memory holds the (complemented) current label (this
    /// organism's own memory when it faces its own cell).
    pub(crate) fn neighbor_has_label(&self, world: &mut dyn OrgInterface) -> bool {
        let label = self.label().clone();
        let is = self.base.inst_set.clone();
        let search = |mem: &avida_genome::CpuMemory| {
            let mut search_head = HeadCpu::default();
            HardwareCpu::find_label_in_memory(&is, mem, &label, &mut search_head);
            search_head.in_memory(mem.size())
        };
        let mut found = false;
        match world.with_neighbor(&mut |n: &mut Organism| found = search(n.hardware.memory())) {
            Faced::SelfOrg => search(&self.memory),
            Faced::Other => found,
            Faced::Empty => false,
        }
    }

    /// `Inst_RotateL` (`dir = -1`) / `Inst_RotateR` (`dir = 1`).
    pub(crate) fn rotate_dir(&mut self, world: &mut dyn OrgInterface, dir: i32) -> R {
        let num_neighbors = world.get_num_neighbors();
        if num_neighbors == 0 {
            return Ok(false);
        }
        self.rl(world);
        world.rotate(dir);
        if self.label().get_size() == 0 {
            return Ok(true);
        }
        self.rotate_label();
        for _ in 1..num_neighbors {
            if self.neighbor_has_label(world) {
                return Ok(true);
            }
            world.rotate(-dir);
        }
        Ok(true)
    }

    /// `Inst_Throw` / `Inst_Goto` search for a marker instruction followed
    /// by the current label. `exact` selects the `goto` matching rules.
    fn throw_goto(&mut self, world: &mut dyn OrgInterface, marker: &str, exact: bool) -> R {
        let marker_inst = self.base.inst_set.get_inst(marker)?;
        self.rl(world);
        let size = self.memory.size();
        let is = self.base.inst_set.clone();
        let label = self.label().clone();
        let mut search_head = self.ip_head();
        let start_pos = search_head.position();
        search_head.advance(size, 1);

        while start_pos != search_head.position() {
            if search_head.get_inst(&self.memory) == marker_inst {
                let marker_pos = search_head.position();
                search_head.advance(size, 1);
                if exact {
                    let mut size_matched = 0;
                    while size_matched < label.get_size() {
                        let inst = search_head.get_inst(&self.memory);
                        if !is.is_nop(inst) || label.get(size_matched) != is.nop_mod(inst) {
                            break;
                        }
                        size_matched += 1;
                        search_head.advance(size, 1);
                    }
                    if size_matched == label.get_size()
                        && !is.is_nop(search_head.get_inst(&self.memory))
                    {
                        self.head_set_pos(HEAD_IP, marker_pos);
                        self.advance_ip = false;
                        return Ok(true);
                    }
                } else {
                    let mut matched = true;
                    let mut size_matched = 0;
                    while matched
                        && is.is_nop(search_head.get_inst(&self.memory))
                        && size_matched < label.get_size()
                    {
                        if label.get(size_matched) != is.nop_mod(search_head.get_inst(&self.memory))
                        {
                            matched = false;
                        }
                        search_head.advance(size, 1);
                        size_matched += 1;
                    }
                    if matched {
                        self.head_set_pos(HEAD_IP, marker_pos);
                        self.advance_ip = false;
                        return Ok(true);
                    }
                }
                if !is.is_nop(search_head.get_inst(&self.memory)) {
                    search_head.retreat(size, 1);
                }
            }
            search_head.advance(size, 1);
        }
        Ok(false)
    }

    /// `ChangeGroupMatingTypes` bookkeeping of the `set-mating-type-*`
    /// instructions.
    fn change_mating_group(
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        old_type: i32,
        new_type: i32,
    ) {
        if world.has_opinion(org) {
            let group = org.get_opinion().0;
            world.change_group_mating_types(org, group, old_type, new_type);
        }
    }
}

// ----------------------------------------------------------------------
// Instruction handlers
// ----------------------------------------------------------------------

/// `Inst_Nop`
fn inst_nop(_: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    Ok(true)
}

// ---- Conditionals ----

macro_rules! if_reg {
    ($(#[$m:meta])* $name:ident, $def:expr, |$v:ident| $skip:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let r = hw.find_modified_register($def);
            let $v = hw.reg(r);
            hw.skip_if($skip);
            Ok(true)
        }
    };
}

macro_rules! if_pair {
    ($(#[$m:meta])* $name:ident, |$a:ident, $b:ident| $skip:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let op1 = hw.find_modified_register(REG_BX);
            let op2 = HardwareCpu::find_next_register(op1);
            let $a = hw.reg(op1);
            let $b = hw.reg(op2);
            hw.skip_if($skip);
            Ok(true)
        }
    };
}

if_reg!(/// `Inst_If0`
    inst_if0, REG_BX, |v| v != 0);
if_reg!(/// `Inst_IfNot0`
    inst_if_not0, REG_BX, |v| v == 0);
if_reg!(/// `Inst_If0_defaultAX`
    inst_if0_default_ax, REG_AX, |v| v != 0);
if_reg!(/// `Inst_IfNot0_defaultAX`
    inst_if_not0_default_ax, REG_AX, |v| v == 0);
if_reg!(/// `Inst_IfGr0`
    inst_if_gr0, REG_BX, |v| v <= 0);
if_reg!(/// `Inst_IfGrEqu0`
    inst_if_gr_equ0, REG_BX, |v| v < 0);
if_reg!(/// `Inst_IfLess0`
    inst_if_less0, REG_BX, |v| v >= 0);
if_reg!(/// `Inst_IfLsEqu0`
    inst_if_ls_equ0, REG_BX, |v| v > 0);
if_reg!(/// `Inst_IfBit1`
    inst_if_bit1, REG_BX, |v| (v & 1) == 0);
if_reg!(/// `Inst_IfConsensus`
    inst_if_consensus, REG_BX, |v| bit_count(v as u32) < CONSENSUS);
if_reg!(/// `Inst_IfConsensus24`
    inst_if_consensus24, REG_BX, |v| bit_count(v as u32 & MASK24) < CONSENSUS24);

if_pair!(/// `Inst_IfEqu`
    inst_if_equ, |a, b| a != b);
if_pair!(/// `Inst_IfNEqu`
    inst_if_n_equ, |a, b| a == b);
if_pair!(/// `Inst_IfGr`
    inst_if_gr, |a, b| a <= b);
if_pair!(/// `Inst_IfGrEqu`
    inst_if_gr_equ, |a, b| a < b);
if_pair!(/// `Inst_IfLess`
    inst_if_less, |a, b| a >= b);
if_pair!(/// `Inst_IfLsEqu`
    inst_if_ls_equ, |a, b| a > b);

/// `Inst_IfANotEqB`
fn inst_if_a_not_eq_b(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let c = hw.reg(REG_AX) == hw.reg(REG_BX);
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfBNotEqC`
fn inst_if_b_not_eq_c(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let c = hw.reg(REG_BX) == hw.reg(REG_CX);
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfANotEqC`
fn inst_if_a_not_eq_c(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let c = hw.reg(REG_AX) == hw.reg(REG_CX);
    hw.skip_if(c);
    Ok(true)
}

/// The nop-selected comparison value of `Inst_IfGrX` / `Inst_IfEquX`.
fn compare_x_value(hw: &mut HardwareCpu) -> i32 {
    let mut value = 1;
    let ip = hw.ip_head();
    let is = hw.base.inst_set.clone();
    if is.is_nop(ip.get_next_inst(&hw.memory, &is)) {
        hw.ip_advance();
        let ip = hw.ip_head();
        value = match is.nop_mod(ip.get_inst(&hw.memory)) {
            0 => -1,
            1 => 2,
            2 => 4,
            _ => 1,
        };
        hw.ip_set_flag_executed();
    }
    value
}

/// `Inst_IfGrX`
fn inst_if_gr_x(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let x = compare_x_value(hw);
    let c = hw.reg(REG_BX) <= x;
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfEquX`
fn inst_if_equ_x(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let x = compare_x_value(hw);
    let c = hw.reg(REG_BX) != x;
    hw.skip_if(c);
    Ok(true)
}

macro_rules! if_prob {
    ($(#[$m:meta])* $name:ident, $p:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
            let c = world.rng().p($p);
            hw.skip_if(c);
            Ok(true)
        }
    };
}

if_prob!(/// `Inst_IfP0p125`
    inst_if_p0p125, 0.875);
if_prob!(/// `Inst_IfP0p25`
    inst_if_p0p25, 0.75);
if_prob!(/// `Inst_IfP0p50`
    inst_if_p0p50, 0.5);
if_prob!(/// `Inst_IfP0p75`
    inst_if_p0p75, 0.25);

/// `Inst_IfGerm`
fn inst_if_germ(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.skip_if(!org.phenotype.is_germ_cell);
    Ok(true)
}

/// `Inst_IfSoma`
fn inst_if_soma(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.skip_if(org.phenotype.is_germ_cell);
    Ok(true)
}

macro_rules! if_end {
    ($(#[$m:meta])* $name:ident, |$a:ident, $b:ident| $fail:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let x = hw.find_modified_register(REG_BX);
            let y = HardwareCpu::find_next_register(x);
            let $a = hw.reg(x);
            let $b = hw.reg(y);
            if $fail {
                hw.else_top_half();
            }
            Ok(true)
        }
    };
}

if_end!(/// `Inst_IfLessEnd`
    inst_if_less_end, |a, b| a >= b);
if_end!(/// `Inst_IfNotEqualEnd`
    inst_if_not_equal_end, |a, b| a == b);
if_end!(/// `Inst_IfGrtEquEnd`
    inst_if_grt_equ_end, |a, b| a < b);

/// `Inst_Else`
fn inst_else(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let endif_head = hw.find_inst("endif");
    if endif_head.position() != hw.ip_head().position() {
        hw.head_set_from(HEAD_IP, &endif_head);
    } else {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_IfLessConsensus`
fn inst_if_less_consensus(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let op1 = hw.find_modified_register(REG_BX);
    let op2 = hw.find_modified_next_register(op1);
    let c = bit_count(hw.reg(op1) as u32) >= bit_count(hw.reg(op2) as u32);
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfLessConsensus24`
fn inst_if_less_consensus24(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let op1 = hw.find_modified_register(REG_BX);
    let op2 = hw.find_modified_next_register(op1);
    let c = bit_count(hw.reg(op1) as u32 & MASK24) >= bit_count(hw.reg(op2) as u32 & MASK24);
    hw.skip_if(c);
    Ok(true)
}

// ---- Flow control ----

/// `Inst_JumpF` (`dir = 1`) / `Inst_JumpB` (`dir = -1`).
fn jump_dir(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface, dir: i32) -> R {
    hw.rl(world);
    hw.rotate_label();
    let active = hw.active_head_id();
    if hw.label().get_size() == 0 {
        let bx = hw.reg(REG_BX);
        hw.head_jump(active, bx);
        return Ok(true);
    }
    let jump_location = hw.find_label(dir);
    if jump_location.position() != -1 {
        hw.head_set_from(active, &jump_location);
        return Ok(true);
    }
    let msg = if dir > 0 {
        "jump-f: No complement label"
    } else {
        "jump-b: No complement label"
    };
    org.fault(FAULT_LOC_JUMP, FAULT_TYPE_ERROR, msg);
    Ok(false)
}

/// `Inst_JumpF`
fn inst_jump_f(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    jump_dir(hw, org, world, 1)
}

/// `Inst_JumpB`
fn inst_jump_b(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    jump_dir(hw, org, world, -1)
}

/// `Inst_Call`
fn inst_call(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let location = hw.ip_head().position();
    hw.stack_push(location);
    hw.rl(world);
    hw.rotate_label();
    if hw.label().get_size() == 0 {
        let bx = hw.reg(REG_BX);
        hw.head_jump(HEAD_IP, bx);
        return Ok(true);
    }
    let jump_location = hw.find_label(1);
    if jump_location.position() != -1 {
        hw.head_set_from(HEAD_IP, &jump_location);
        return Ok(true);
    }
    org.fault(
        FAULT_LOC_JUMP,
        FAULT_TYPE_ERROR,
        "call: no complement label",
    );
    Ok(false)
}

/// `Inst_Return`
fn inst_return(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let v = hw.stack_pop();
    hw.head_set_pos(HEAD_IP, v);
    Ok(true)
}

/// `Inst_Throw`.
///
/// QUIRK: the C++ looks up the `catch` opcode once per process (a function
/// static); an instruction set without `catch` makes `GetInst` exit(1).
fn inst_throw(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.throw_goto(world, "catch", false)
}

/// `Inst_ThrowIfNot0`
fn inst_throw_if_not0(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if hw.reg(REG_BX) == 0 {
        return Ok(false);
    }
    inst_throw(hw, org, world)
}

/// `Inst_ThrowIf0`
fn inst_throw_if0(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if hw.reg(REG_BX) != 0 {
        return Ok(false);
    }
    inst_throw(hw, org, world)
}

/// `Inst_Catch` / `Inst_Label` / `Inst_Transposon`: read the label only.
fn inst_read_label_only(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    Ok(true)
}

/// `Inst_Goto` (QUIRK as `Inst_Throw`, with the `label` opcode).
fn inst_goto(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.throw_goto(world, "label", true)
}

/// `Inst_GotoIfNot0`
fn inst_goto_if_not0(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if hw.reg(REG_BX) == 0 {
        return Ok(false);
    }
    inst_goto(hw, org, world)
}

/// `Inst_GotoIf0`
fn inst_goto_if0(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if hw.reg(REG_BX) != 0 {
        return Ok(false);
    }
    inst_goto(hw, org, world)
}

/// `Inst_Skip`
fn inst_skip(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.ip_advance();
    Ok(true)
}

// ---- Stack ----

/// `Inst_Pop`
fn inst_pop(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = hw.stack_pop();
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_Push`
fn inst_push(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = hw.reg(r);
    hw.stack_push(v);
    Ok(true)
}

/// `Inst_HeadPop`
fn inst_head_pop(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let v = hw.stack_pop();
    hw.head_set_pos(h, v);
    Ok(true)
}

/// `Inst_HeadPush`
fn inst_head_push(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let pos = hw.get_head(h).position();
    hw.stack_push(pos);
    if h == HEAD_IP {
        let flow = hw.get_head(HEAD_FLOW);
        hw.head_set_from(h, &flow);
        hw.advance_ip = false;
    }
    Ok(true)
}

macro_rules! pop_push_fixed {
    ($pop:ident, $push:ident, $reg:expr, $pdoc:literal, $udoc:literal) => {
        #[doc = $pdoc]
        fn $pop(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let v = hw.stack_pop();
            hw.set_reg($reg, v);
            Ok(true)
        }
        #[doc = $udoc]
        fn $push(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let v = hw.reg($reg);
            hw.stack_push(v);
            Ok(true)
        }
    };
}

pop_push_fixed!(
    inst_pop_a,
    inst_push_a,
    REG_AX,
    "`Inst_PopA`",
    "`Inst_PushA`"
);
pop_push_fixed!(
    inst_pop_b,
    inst_push_b,
    REG_BX,
    "`Inst_PopB`",
    "`Inst_PushB`"
);
pop_push_fixed!(
    inst_pop_c,
    inst_push_c,
    REG_CX,
    "`Inst_PopC`",
    "`Inst_PushC`"
);

/// `Inst_SwitchStack`
fn inst_switch_stack(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.switch_stack();
    Ok(true)
}

/// `Inst_FlipStack`
fn inst_flip_stack(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.stack_flip();
    Ok(true)
}

// ---- Registers ----

/// `Inst_Swap`
fn inst_swap(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let op1 = hw.find_modified_register(REG_BX);
    let op2 = HardwareCpu::find_next_register(op1);
    let (a, b) = (hw.reg(op1), hw.reg(op2));
    hw.set_reg(op1, b);
    hw.set_reg(op2, a);
    Ok(true)
}

macro_rules! swap_fixed {
    ($name:ident, $a:expr, $b:expr, $doc:literal) => {
        #[doc = $doc]
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let (x, y) = (hw.reg($a), hw.reg($b));
            hw.set_reg($a, y);
            hw.set_reg($b, x);
            Ok(true)
        }
    };
}

swap_fixed!(inst_swap_ab, REG_AX, REG_BX, "`Inst_SwapAB`");
swap_fixed!(inst_swap_bc, REG_BX, REG_CX, "`Inst_SwapBC`");
swap_fixed!(inst_swap_ac, REG_AX, REG_CX, "`Inst_SwapAC`");

/// `Inst_CopyReg`
fn inst_copy_reg(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let src = hw.find_modified_register(REG_BX);
    let dst = HardwareCpu::find_next_register(src);
    let v = hw.reg(src);
    hw.set_reg(dst, v);
    Ok(true)
}

macro_rules! copy_fixed {
    ($name:ident, $dst:expr, $src:expr, $doc:literal) => {
        #[doc = $doc]
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let v = hw.reg($src);
            hw.set_reg($dst, v);
            Ok(true)
        }
    };
}

copy_fixed!(inst_copy_reg_ab, REG_AX, REG_BX, "`Inst_CopyRegAB`");
copy_fixed!(inst_copy_reg_ac, REG_AX, REG_CX, "`Inst_CopyRegAC`");
copy_fixed!(inst_copy_reg_ba, REG_BX, REG_AX, "`Inst_CopyRegBA`");
copy_fixed!(inst_copy_reg_bc, REG_BX, REG_CX, "`Inst_CopyRegBC`");
copy_fixed!(inst_copy_reg_ca, REG_CX, REG_AX, "`Inst_CopyRegCA`");
copy_fixed!(inst_copy_reg_cb, REG_CX, REG_BX, "`Inst_CopyRegCB`");

/// `Inst_Reset`
fn inst_reset(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    for r in 0..NUM_REGISTERS {
        hw.set_reg(r, 0);
    }
    hw.stack_clear();
    hw.last_cell_data = (false, 0);
    Ok(true)
}

macro_rules! unary {
    ($(#[$m:meta])* $name:ident, $def:expr, |$v:ident| $e:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let r = hw.find_modified_register($def);
            let $v = hw.reg(r);
            hw.set_reg(r, $e);
            Ok(true)
        }
    };
}

unary!(/// `Inst_ShiftR`
    inst_shift_r, REG_BX, |v| v >> 1);
unary!(/// `Inst_ShiftL`
    inst_shift_l, REG_BX, |v| v.wrapping_shl(1));
unary!(/// `Inst_Bit1`
    inst_bit1, REG_BX, |v| v | 1);
unary!(/// `Inst_Inc`
    inst_inc, REG_BX, |v| v.wrapping_add(1));
unary!(/// `Inst_Dec`
    inst_dec, REG_BX, |v| v.wrapping_sub(1));
unary!(/// `Inst_Zero`
    inst_zero, REG_BX, |_v| 0);
unary!(/// `Inst_One`
    inst_one, REG_BX, |_v| 1);
unary!(/// `Inst_All1s`
    inst_all1s, REG_BX, |_v| -1);
unary!(/// `Inst_Neg`
    inst_neg, REG_BX, |v| v.wrapping_neg());
unary!(/// `Inst_Square`
    inst_square, REG_BX, |v| v.wrapping_mul(v));
unary!(/// `Inst_Not`
    inst_not, REG_BX, |v| !v);
unary!(/// `Inst_MaskSignBit`
    inst_mask_sign_bit, REG_BX, |v| (v as u32 & MASK_SIGNBIT) as i32);

macro_rules! mask_off {
    ($bx:ident, $ax:ident, $mask:expr, $bdoc:literal, $adoc:literal) => {
        unary!(
            #[doc = $bdoc]
            $bx,
            REG_BX,
            |v| (v as u32 & $mask) as i32
        );
        unary!(
            #[doc = $adoc]
            $ax,
            REG_AX,
            |v| (v as u32 & $mask) as i32
        );
    };
}

mask_off!(
    inst_mask_off16,
    inst_mask_off16_ax,
    0xFFFF_0000,
    "`Inst_MaskOffLower16Bits`",
    "`Inst_MaskOffLower16Bits_defaultAX`"
);
mask_off!(
    inst_mask_off15,
    inst_mask_off15_ax,
    0xFFFF_8000,
    "`Inst_MaskOffLower15Bits`",
    "`Inst_MaskOffLower15Bits_defaultAX`"
);
mask_off!(
    inst_mask_off14,
    inst_mask_off14_ax,
    0xFFFF_C000,
    "`Inst_MaskOffLower14Bits`",
    "`Inst_MaskOffLower14Bits_defaultAX`"
);
mask_off!(
    inst_mask_off13,
    inst_mask_off13_ax,
    0xFFFF_E000,
    "`Inst_MaskOffLower13Bits`",
    "`Inst_MaskOffLower13Bits_defaultAX`"
);
mask_off!(
    inst_mask_off12,
    inst_mask_off12_ax,
    0xFFFF_F000,
    "`Inst_MaskOffLower12Bits`",
    "`Inst_MaskOffLower12Bits_defaultAX`"
);
mask_off!(
    inst_mask_off8,
    inst_mask_off8_ax,
    0xFFFF_FF00,
    "`Inst_MaskOffLower8Bits`",
    "`Inst_MaskOffLower8Bits_defaultAX`"
);
mask_off!(
    inst_mask_off4,
    inst_mask_off4_ax,
    0xFFFF_FFF0,
    "`Inst_MaskOffLower4Bits`",
    "`Inst_MaskOffLower4Bits_defaultAX`"
);

/// `Inst_Sqrt` / `Inst_Log` / `Inst_Log10` (`which` 0, 1, 2).
fn math_fn(hw: &mut HardwareCpu, org: &mut OrgState, which: i32) -> R {
    let r = hw.find_modified_register(REG_BX);
    let value = hw.reg(r);
    let (threshold_ok, name) = match which {
        0 => (value > 1, "sqrt"),
        1 => (value >= 1, "log"),
        _ => (value >= 1, "log10"),
    };
    if threshold_ok {
        let d = f64::from(value);
        let out = match which {
            0 => d.sqrt(),
            1 => d.ln(),
            _ => d.log10(),
        };
        hw.set_reg(r, out as i32);
    } else if value < 0 {
        org.fault(
            FAULT_LOC_MATH,
            FAULT_TYPE_ERROR,
            &format!("{}: value is negative", name),
        );
        return Ok(false);
    }
    Ok(true)
}

/// `Inst_Sqrt`
fn inst_sqrt(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    math_fn(hw, org, 0)
}

/// `Inst_Log`
fn inst_log(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    math_fn(hw, org, 1)
}

/// `Inst_Log10`
fn inst_log10(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    math_fn(hw, org, 2)
}

macro_rules! binop {
    ($(#[$m:meta])* $name:ident, |$a:ident, $b:ident| $e:expr) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            let dst = hw.find_modified_register(REG_BX);
            let $a = hw.reg(REG_BX);
            let $b = hw.reg(REG_CX);
            hw.set_reg(dst, $e);
            Ok(true)
        }
    };
}

binop!(/// `Inst_Add`
    inst_add, |a, b| a.wrapping_add(b));
binop!(/// `Inst_Sub`
    inst_sub, |a, b| a.wrapping_sub(b));
binop!(/// `Inst_Mult`
    inst_mult, |a, b| a.wrapping_mul(b));
binop!(/// `Inst_Nand`
    inst_nand, |a, b| !(a & b));
binop!(/// `Inst_Nor`
    inst_nor, |a, b| !(a | b));
binop!(/// `Inst_Or`
    inst_or, |a, b| a | b);
binop!(/// `Inst_And`
    inst_and, |a, b| a & b);
binop!(/// `Inst_Xor`
    inst_xor, |a, b| a ^ b);

/// `a / b` as the arm64 reference build computes it: `sdiv` yields 0 for
/// a zero divisor (undefined in C++, a trap on x86) and wraps
/// `INT_MIN / -1`.
pub(crate) fn arm_div(a: i32, b: i32) -> i32 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}

/// `a % b` as the arm64 reference build computes it (`a - (a / b) * b`
/// with [`arm_div`]): `a` for a zero divisor.
pub(crate) fn arm_rem(a: i32, b: i32) -> i32 {
    if b == 0 {
        a
    } else {
        a.wrapping_rem(b)
    }
}

/// `Inst_Div`, guarded as the reference binary guards it (see [`div_guard`]):
/// `-INT_MAX / -1` faults, and `INT_MIN / -1` (undefined in C++) yields
/// `INT_MIN`, as `sdiv` does on arm64.
fn inst_div(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let dst = hw.find_modified_register(REG_BX);
    let a = hw.reg(REG_BX);
    let b = hw.reg(REG_CX);
    match div_guard(a, b) {
        DivGuard::Ok => hw.set_reg(dst, a.wrapping_div(b)),
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

/// `Inst_Mod`, guarded as the reference binary guards it (see
/// [`div_guard`]): `-INT_MAX % -1` faults as a modulo by 0, and
/// `INT_MIN % -1` (undefined in C++) yields 0, as the arm64 reference build
/// computes it.
fn inst_mod(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let dst = hw.find_modified_register(REG_BX);
    let a = hw.reg(REG_BX);
    let b = hw.reg(REG_CX);
    if div_guard(a, b) == DivGuard::Ok {
        hw.set_reg(dst, a.wrapping_rem(b));
    } else {
        org.fault(FAULT_LOC_MATH, FAULT_TYPE_ERROR, "mod: modding by 0");
        return Ok(false);
    }
    Ok(true)
}

/// `Inst_Order`
fn inst_order(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let (a, b) = (hw.reg(REG_BX), hw.reg(REG_CX));
    if a > b {
        hw.set_reg(REG_BX, b);
        hw.set_reg(REG_CX, a);
    }
    Ok(true)
}

/// `Inst_Setbit`
fn inst_setbit(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let to_set = hw.find_modified_register(REG_BX);
    let bit_reg = HardwareCpu::find_next_register(to_set);
    let bit = (hw.reg(bit_reg).max(0) % 32) as u32;
    let v = hw.reg(to_set) | 1i32.wrapping_shl(bit);
    hw.set_reg(to_set, v);
    Ok(true)
}

/// `Inst_Clearbit`
fn inst_clearbit(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let to_clear = hw.find_modified_register(REG_BX);
    let bit_reg = HardwareCpu::find_next_register(to_clear);
    let bit = (hw.reg(bit_reg).max(0) % 32) as u32;
    let v = hw.reg(to_clear) & !1i32.wrapping_shl(bit);
    hw.set_reg(to_clear, v);
    Ok(true)
}

/// `Inst_BitConsensus`
fn inst_bit_consensus(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let op1 = hw.find_modified_next_register(r);
    let v = i32::from(bit_count(hw.reg(op1) as u32) >= CONSENSUS);
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_BitConsensus24`
fn inst_bit_consensus24(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let op1 = hw.find_modified_next_register(r);
    let v = i32::from(bit_count(hw.reg(op1) as u32 & MASK24) >= CONSENSUS24);
    hw.set_reg(r, v);
    Ok(true)
}

macro_rules! set_num {
    ($(#[$m:meta])* $name:ident, $method:ident) => {
        $(#[$m])*
        fn $name(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
            hw.rl(world);
            let v = hw.label().$method(NUM_NOPS);
            hw.set_reg(REG_BX, v);
            Ok(true)
        }
    };
}

set_num!(/// `Inst_SetNum`
    inst_set_num, as_int);
set_num!(/// `Inst_ValGrey`
    inst_val_grey, as_int_grey_code);
set_num!(/// `Inst_ValDir`
    inst_val_dir, as_int_direct);
set_num!(/// `Inst_ValAddP`
    inst_val_add_p, as_int_additive_polynomial);
set_num!(/// `Inst_ValFib`
    inst_val_fib, as_int_fib);
set_num!(/// `Inst_ValPolyC`
    inst_val_poly_c, as_int_polynomial_coefficent);

// ---- Memory: single-instruction copies ----

/// `Inst_Copy`
fn inst_copy(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let from = hw.make_head(hw.reg(REG_BX));
    let to = hw.make_head(hw.reg(REG_AX).wrapping_add(hw.reg(REG_BX)));
    if HardwareCpu::test_copy_mut(org, world)
        && !in_no_mut_list(world.config(), from.get_inst(&hw.memory))
    {
        let inst = hw.base.inst_set.random_inst(world.rng());
        to.set_inst(&mut hw.memory, inst);
        to.set_flag_mutated(&mut hw.memory);
        to.set_flag_copy_mut(&mut hw.memory);
    } else {
        let inst = from.get_inst(&hw.memory);
        to.set_inst(&mut hw.memory, inst);
        to.clear_flag_mutated(&mut hw.memory);
        to.clear_flag_copy_mut(&mut hw.memory);
    }
    to.set_flag_copied(&mut hw.memory);
    Ok(true)
}

/// `Inst_ReadInst`
fn inst_read_inst(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let dst = hw.find_modified_register(REG_CX);
    let from = hw.make_head(hw.reg(REG_BX));
    let v = from.get_inst(&hw.memory).op();
    hw.set_reg(dst, v);
    Ok(true)
}

/// The mutate-or-write tail of `Inst_WriteInst` / `Inst_StackWriteInst`.
fn write_value(
    hw: &mut HardwareCpu,
    org: &OrgState,
    world: &mut dyn OrgInterface,
    to: HeadCpu,
    value: i32,
) {
    if HardwareCpu::test_copy_mut(org, world)
        && !in_no_mut_list(world.config(), to.get_inst(&hw.memory))
    {
        let inst = hw.base.inst_set.random_inst(world.rng());
        to.set_inst(&mut hw.memory, inst);
        to.set_flag_mutated(&mut hw.memory);
        to.set_flag_copy_mut(&mut hw.memory);
    } else {
        to.set_inst(&mut hw.memory, Instruction::new(value));
        to.clear_flag_mutated(&mut hw.memory);
        to.clear_flag_copy_mut(&mut hw.memory);
    }
    to.set_flag_copied(&mut hw.memory);
}

/// `Inst_WriteInst`
fn inst_write_inst(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let src = hw.find_modified_register(REG_CX);
    let to = hw.make_head(hw.reg(REG_AX).wrapping_add(hw.reg(REG_BX)));
    let value = avida_mod(hw.reg(src), hw.base.inst_set.size());
    write_value(hw, org, world, to, value);
    Ok(true)
}

/// `Inst_StackReadInst`
fn inst_stack_read_inst(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_CX);
    let from = hw.make_head(hw.reg(r));
    let v = from.get_inst(&hw.memory).op();
    hw.stack_push(v);
    Ok(true)
}

/// `Inst_StackWriteInst`
fn inst_stack_write_inst(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let dst = hw.find_modified_register(REG_BX);
    let to = hw.make_head(hw.reg(REG_AX).wrapping_add(hw.reg(dst)));
    let popped = hw.stack_pop();
    let value = avida_mod(popped, hw.base.inst_set.size());
    write_value(hw, org, world, to, value);
    Ok(true)
}

/// `Inst_Compare`
fn inst_compare(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let dst = hw.find_modified_register(REG_CX);
    let from = hw.make_head(hw.reg(REG_BX));
    let to = hw.make_head(hw.reg(REG_AX).wrapping_add(hw.reg(REG_BX)));
    if HardwareCpu::test_copy_mut(org, world)
        && !in_no_mut_list(world.config(), from.get_inst(&hw.memory))
    {
        let inst = hw.base.inst_set.random_inst(world.rng());
        to.set_inst(&mut hw.memory, inst);
        to.set_flag_mutated(&mut hw.memory);
        to.set_flag_copy_mut(&mut hw.memory);
    }
    let v = from.get_inst(&hw.memory).op() - to.get_inst(&hw.memory).op();
    hw.set_reg(dst, v);
    Ok(true)
}

/// `Inst_IfNCpy`
fn inst_if_n_cpy(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let from = hw.make_head(hw.reg(REG_BX));
    let to = hw.make_head(hw.reg(REG_AX).wrapping_add(hw.reg(REG_BX)));
    let same = from.get_inst(&hw.memory) == to.get_inst(&hw.memory);
    if HardwareCpu::test_copy_mut(org, world) {
        hw.skip_if(!same);
    } else {
        hw.skip_if(same);
    }
    Ok(true)
}

// ---- Allocation, divide, reproduction ----

/// `Inst_Allocate`
fn inst_allocate(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let size = hw.memory.size();
    let amount = hw.reg(REG_BX);
    if hw.allocate_main(org, world, amount) {
        hw.set_reg(REG_AX, size);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// `Inst_Divide`
fn inst_divide(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let p = hw.reg(REG_AX);
    hw.divide_main(org, world, p, 0, 1.0)
}

/// `Inst_DivideRS`
fn inst_divide_rs(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let p = hw.reg(REG_AX);
    hw.divide_main_rs(org, world, p, 0, 1.0, 0)
}

/// `Inst_CDivide`
fn inst_c_divide(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let p = hw.memory.size() / 2;
    hw.divide_main(org, world, p, 0, 1.0)
}

/// `Inst_CAlloc`
fn inst_c_alloc(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let size = hw.memory.size();
    Ok(hw.allocate_main(org, world, size))
}

/// `Inst_MaxAlloc` (`move_write_head = false`) and
/// `Inst_MaxAllocMoveWriteHead` (`true`).
fn max_alloc(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    move_write_head: bool,
) -> R {
    let cur_size = hw.memory.size();
    let range = world.config().offspring_size_range;
    let alloc_size = ((range * f64::from(cur_size)) as i32).min(MAX_GENOME_LENGTH - cur_size);
    if hw.allocate_main(org, world, alloc_size) {
        hw.set_reg(REG_AX, cur_size);
        if move_write_head {
            hw.head_set_pos(HEAD_WRITE, cur_size);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

/// `Inst_MaxAlloc`
fn inst_max_alloc(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    max_alloc(hw, org, world, false)
}

/// `Inst_MaxAllocMoveWriteHead`
fn inst_max_alloc_move_write_head(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    max_alloc(hw, org, world, true)
}

/// `Inst_Repro`
fn inst_repro(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.inst_repro(org, world)
}

/// `Inst_ReproSex`
fn inst_repro_sex(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    org.phenotype.divide_sex = true;
    org.phenotype.cross_num = 1;
    hw.inst_repro(org, world)
}

/// `Inst_ReproGermFlag`
fn inst_repro_germ_flag(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let res = hw.inst_repro(org, world)?;
    if res {
        org.phenotype.is_germ_cell = true;
    }
    Ok(res)
}

/// `Inst_TaskPutRepro`
fn inst_task_put_repro(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let value = hw.reg(r);
    hw.output(org, world, value)?;
    hw.inst_repro(org, world)
}

/// `Inst_TaskPutResetInputsRepro`
fn inst_task_put_reset_inputs_repro(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let return_value = inst_task_put_reset_inputs(hw, org, world)?;
    hw.inst_repro(org, world)?;
    Ok(return_value)
}

/// `Inst_ConditionalRepro`
fn inst_conditional_repro(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if org.phenotype.last_task_id == 0 {
        return hw.inst_repro(org, world);
    }
    Ok(false)
}

/// `Inst_Sterilize`
fn inst_sterilize(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.phenotype.is_fertile = false;
    Ok(true)
}

/// `Inst_Die`
fn inst_die(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    org.die(world);
    Ok(true)
}

/// `Inst_Prob_Die`
fn inst_prob_die(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_AX);
    let mut percent_prob = world.config().kaboom_prob;
    if percent_prob == -1.0 {
        percent_prob = f64::from(hw.reg(r).wrapping_rem(100)) / 100.0;
    }
    if world.rng().p(percent_prob) {
        org.die(world);
    }
    Ok(true)
}

/// `Inst_Poison`
fn inst_poison(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let poison_multiplier = 1.0 - world.config().poison_penalty;
    org.phenotype.cur_bonus *= poison_multiplier;
    Ok(true)
}

/// `Inst_NopPre`
fn inst_nop_pre(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    Ok(org.phenotype.num_divides == 0)
}

/// `Inst_NopPost`
fn inst_nop_post(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    Ok(org.phenotype.num_divides > 0)
}

// ---- Input/output ----

/// `Inst_TaskGet`
fn inst_task_get(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_CX);
    let value = HardwareCpu::next_input(org, world);
    hw.set_reg(r, value);
    org.do_input(value);
    Ok(true)
}

/// `Inst_TaskGet2`
fn inst_task_get2(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    world.reset_inputs();
    org.input_buf.clear();
    let r1 = hw.find_modified_register(REG_BX);
    let r2 = HardwareCpu::find_next_register(r1);
    let v1 = HardwareCpu::next_input(org, world);
    hw.set_reg(r1, v1);
    org.do_input(v1);
    let v2 = HardwareCpu::next_input(org, world);
    hw.set_reg(r2, v2);
    org.do_input(v2);
    Ok(true)
}

/// `Inst_TaskStackGet`
fn inst_task_stack_get(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let value = HardwareCpu::next_input(org, world);
    hw.stack_push(value);
    org.do_input(value);
    Ok(true)
}

/// `Inst_TaskStackLoad`
fn inst_task_stack_load(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    for _ in 0..3 {
        let v = HardwareCpu::next_input(org, world);
        hw.stack_push(v);
    }
    Ok(true)
}

/// `Inst_TaskPut`
fn inst_task_put(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let value = hw.reg(r);
    hw.set_reg(r, 0);
    hw.output(org, world, value)?;
    Ok(true)
}

/// `Inst_TaskPutResetInputs`
fn inst_task_put_reset_inputs(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let return_value = inst_task_put(hw, org, world)?;
    world.reset_inputs();
    org.input_buf.clear();
    Ok(return_value)
}

/// `Inst_TaskIO`
fn inst_task_io(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let value_out = hw.reg(r);
    hw.output(org, world, value_out)?;
    let value_in = HardwareCpu::next_input(org, world);
    hw.set_reg(r, value_in);
    org.do_input(value_in);
    Ok(true)
}

/// `Inst_TaskIO_BonusCost_0_001` (`Inst_TaskIO_BonusCost(ctx, 0.001)`).
fn inst_task_io_bonus_cost_0_001(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let mut new_bonus = org.phenotype.cur_bonus * (1.0 - 0.001);
    if new_bonus < 0.0 {
        new_bonus = 0.0;
    }
    org.phenotype.cur_bonus = new_bonus;
    inst_task_io(hw, org, world)
}

/// `Inst_TaskIO_Feedback`
fn inst_task_io_feedback(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let pre = org.phenotype.cur_bonus;
    let value_out = hw.reg(r);
    hw.output(org, world, value_out)?;
    let post = org.phenotype.cur_bonus;
    if pre > post {
        hw.stack_push(-1);
    } else if pre == post {
        hw.stack_push(0);
    } else if pre < post {
        hw.stack_push(1);
    }
    let value_in = HardwareCpu::next_input(org, world);
    hw.set_reg(r, value_in);
    org.do_input(value_in);
    Ok(true)
}

/// `Inst_MatchStrings`
fn inst_match_strings(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if hw.executedmatchstrings {
        return Ok(false);
    }
    hw.output(org, world, 357_913_941)?;
    hw.executedmatchstrings = true;
    Ok(true)
}

/// `Inst_Send`
fn inst_send(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    org.sent_active = true;
    org.sent_value = hw.reg(r);
    hw.set_reg(r, 0);
    Ok(true)
}

/// `Inst_Receive`
fn inst_receive(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world.receive_value(org);
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_IOBufAdd1`
fn inst_io_buf_add1(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.output_buf.add(1);
    Ok(true)
}

/// `Inst_IOBufAdd0`
fn inst_io_buf_add0(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.output_buf.add(0);
    Ok(true)
}

// ---- Search ----

/// `Inst_SearchF`
fn inst_search_f(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    hw.rotate_label();
    let search_size = hw.find_label(1).position() - hw.ip_head().position();
    hw.set_reg(REG_BX, search_size);
    let l = hw.label().get_size();
    hw.set_reg(REG_CX, l);
    Ok(true)
}

/// `Inst_SearchB`
fn inst_search_b(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    hw.rotate_label();
    let search_size = hw.ip_head().position() - hw.find_label(-1).position();
    hw.set_reg(REG_BX, search_size);
    let l = hw.label().get_size();
    hw.set_reg(REG_CX, l);
    Ok(true)
}

/// `Inst_MemSize`
fn inst_mem_size(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let s = hw.memory.size();
    hw.set_reg(r, s);
    Ok(true)
}

// ---- Rotation and position ----

/// `Inst_RotateL`
fn inst_rotate_l(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rotate_dir(world, -1)
}

/// `Inst_RotateR`
fn inst_rotate_r(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rotate_dir(world, 1)
}

/// `Inst_RotateLeftOne`
fn inst_rotate_left_one(_: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    world.rotate(1);
    Ok(true)
}

/// `Inst_RotateRightOne`
fn inst_rotate_right_one(_: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    world.rotate(-1);
    Ok(true)
}

/// `Inst_RotateLabel`. A geometry other than grid or torus makes the C++
/// `exit(-1)`.
fn inst_rotate_label(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let actual = world.get_num_neighbors();
    hw.rl(world);
    let geometry = world.config().world_geometry;
    if geometry != GEOMETRY_TORUS && geometry != GEOMETRY_GRID {
        return Err(format!(
            "rotate-label with WORLD_GEOMETRY={} (C++ exits with status -1)",
            geometry
        ));
    }
    let new_facing = hw.label().as_int_grey_code(NUM_NOPS) % 8;
    for _ in 0..actual {
        if new_facing == world.get_facing() {
            break;
        }
        world.rotate(1);
    }
    Ok(true)
}

/// `Inst_RotateUnoccupiedCell` (`occupied = false`) /
/// `Inst_RotateOccupiedCell` (`true`).
fn rotate_to_cell(hw: &mut HardwareCpu, world: &mut dyn OrgInterface, occupied: bool) -> R {
    let r = hw.find_modified_register(REG_BX);
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.is_neighbor_cell_occupied() == occupied {
            hw.set_reg(r, 1);
            return Ok(true);
        }
        world.rotate(1);
        i += 1;
    }
    hw.set_reg(r, 0);
    Ok(true)
}

/// `Inst_RotateUnoccupiedCell`
fn inst_rotate_unoccupied_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    rotate_to_cell(hw, world, false)
}

/// `Inst_RotateOccupiedCell`
fn inst_rotate_occupied_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    rotate_to_cell(hw, world, true)
}

/// `Inst_RotateNextOccupiedCell`
fn inst_rotate_next_occupied_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    world.rotate(1);
    rotate_to_cell(hw, world, true)
}

/// `Inst_RotateNextUnoccupiedCell`
fn inst_rotate_next_unoccupied_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    world.rotate(1);
    rotate_to_cell(hw, world, false)
}

/// `Inst_RotateEventCell`
fn inst_rotate_event_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.get_cell_data() > 0 {
            hw.set_reg(r, 1);
            return Ok(true);
        }
        world.rotate(1);
        i += 1;
    }
    hw.set_reg(r, 0);
    Ok(true)
}

/// `Inst_RotateUphill`. Out-of-range resource indices (undefined in C++)
/// read as 0.
fn inst_rotate_uphill(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let actual = world.get_num_neighbors();
    let mut opinion = 0;
    if world.has_opinion(org) {
        opinion = org.get_opinion().0;
    }
    let at = |v: &[f64], i: i32| -> f64 {
        if i < 0 {
            0.0
        } else {
            v.get(i as usize).copied().unwrap_or(0.0)
        }
    };
    let current_res = world.get_resources(org);
    let mut max_res = 0.0;
    for _ in 0..actual {
        world.rotate(1);
        let faced = world.get_faced_cell_resources(org);
        if at(&faced, opinion) > max_res {
            max_res = at(&faced, opinion);
        }
    }
    let cur = at(&current_res, opinion);
    if max_res > cur {
        for _ in 0..actual {
            let faced = world.get_faced_cell_resources(org);
            if at(&faced, opinion) != max_res {
                world.rotate(1);
            }
        }
    }
    let res_diff = if cur == 0.0 {
        max_res as i32
    } else {
        ((max_res - cur) / cur).mul_add(100.0, 0.5) as i32
    };
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, res_diff);
    Ok(true)
}

/// `Inst_RotateHome`
fn inst_rotate_home(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let easterly = org.easterly;
    let northerly = org.northerly;
    let mut correct_facing = 0;
    if northerly > 0 && easterly == 0 {
        correct_facing = 0;
    } else if northerly > 0 && easterly < 0 {
        correct_facing = 1;
    } else if northerly == 0 && easterly < 0 {
        correct_facing = 2;
    } else if northerly < 0 && easterly < 0 {
        correct_facing = 3;
    } else if northerly < 0 && easterly == 0 {
        correct_facing = 4;
    } else if northerly < 0 && easterly > 0 {
        correct_facing = 5;
    } else if northerly == 0 && easterly > 0 {
        correct_facing = 6;
    } else if northerly > 0 && easterly > 0 {
        correct_facing = 7;
    }
    let mut i = 0;
    while i < world.get_num_neighbors() {
        world.rotate(1);
        if world.get_faced_dir() == correct_facing {
            break;
        }
        i += 1;
    }
    Ok(true)
}

/// `Inst_SetCopyMut`
fn inst_set_copy_mut(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let new_mut_rate = hw.reg(r).max(1);
    org.mut_rates.copy.mut_prob = f64::from(new_mut_rate) / 10000.0;
    Ok(true)
}

/// `Inst_ModCopyMut`
fn inst_mod_copy_mut(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let new_mut_rate = org.mut_rates.copy.mut_prob + f64::from(hw.reg(r)) / 10000.0;
    if new_mut_rate > 0.0 {
        org.mut_rates.copy.mut_prob = new_mut_rate;
    }
    Ok(true)
}

/// `Inst_GetCellPosition`
fn inst_get_cell_position(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let x = world.get_cell_x_position();
    let y = world.get_cell_y_position();
    if x == -1 || y == -1 {
        return Ok(false);
    }
    let xreg = hw.find_modified_register(REG_BX);
    let yreg = HardwareCpu::find_next_register(xreg);
    hw.set_reg(xreg, x);
    hw.set_reg(yreg, y);
    Ok(true)
}

/// `Inst_GetCellPositionX`
fn inst_get_cell_position_x(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let x = world.get_cell_x_position();
    if x == -1 {
        return Ok(false);
    }
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, x);
    Ok(true)
}

/// `Inst_GetCellPositionY`
fn inst_get_cell_position_y(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let y = world.get_cell_y_position();
    if y == -1 {
        return Ok(false);
    }
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, y);
    Ok(true)
}

/// `Inst_GetDirectionOffNorth`
fn inst_get_direction_off_north(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world.get_faced_dir();
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_GetNortherly`
fn inst_get_northerly(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, org.northerly);
    Ok(true)
}

/// `Inst_GetEasterly`
fn inst_get_easterly(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, org.easterly);
    Ok(true)
}

/// `Inst_ZeroEasterly`
fn inst_zero_easterly(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.easterly = 0;
    Ok(true)
}

/// `Inst_ZeroNortherly`
fn inst_zero_northerly(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.northerly = 0;
    Ok(true)
}

// ---- Threads ----

/// `Inst_ForkThread`
fn inst_fork_thread(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.ip_advance();
    let max = world.config().max_cpu_threads;
    if !hw.fork_thread(max) {
        org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_ERROR, "");
    }
    Ok(true)
}

/// `Inst_ForkThreadLabel`
fn inst_fork_thread_label(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.rl(world);
    hw.rotate_label();
    if hw.label().get_size() == 0 {
        return inst_fork_thread(hw, org, world);
    }
    let search_head = hw.find_label(1);
    if search_head.position() != hw.ip_head().position() {
        let save_pos = hw.ip_head().position();
        hw.head_set_pos(HEAD_IP, search_head.position() + 1);
        let max = world.config().max_cpu_threads;
        if !hw.fork_thread(max) {
            org.fault(FAULT_LOC_THREAD_FORK, FAULT_TYPE_ERROR, "");
        }
        hw.head_set_pos(HEAD_IP, save_pos);
    }
    Ok(true)
}

/// `Inst_ForkThreadLabelIfNot0`
fn inst_fork_thread_label_if_not0(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if hw.reg(REG_BX) == 0 {
        hw.rl(world);
        return Ok(false);
    }
    inst_fork_thread_label(hw, org, world)
}

/// `Inst_ForkThreadLabelIf0`
fn inst_fork_thread_label_if0(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if hw.reg(REG_BX) != 0 {
        hw.rl(world);
        return Ok(false);
    }
    inst_fork_thread_label(hw, org, world)
}

/// `Inst_KillThread`
fn inst_kill_thread(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    if !hw.kill_thread() {
        org.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_ERROR, "");
    } else {
        hw.advance_ip = false;
    }
    Ok(true)
}

/// `Inst_ThreadID`
fn inst_thread_id(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let id = hw.threads[hw.ct()].id();
    hw.set_reg(r, id);
    Ok(true)
}

// ---- Heads ----

/// `Inst_SetHead`
fn inst_set_head(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let ct = hw.ct();
    hw.threads[ct].cur_head = h as u8;
    Ok(true)
}

/// `Inst_AdvanceHead`
fn inst_advance_head(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let h = hw.find_modified_head(HEAD_WRITE);
    hw.head_advance(h);
    Ok(true)
}

/// `Inst_MoveHead`
pub(crate) fn inst_move_head(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let target = hw.get_head(HEAD_FLOW);
    hw.head_set_from(h, &target);
    if h == HEAD_IP {
        hw.advance_ip = false;
    }
    Ok(true)
}

/// `Inst_JumpHead`
pub(crate) fn inst_jump_head(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let cx = hw.reg(REG_CX);
    hw.head_jump(h, cx);
    Ok(true)
}

/// `Inst_GetHead`
fn inst_get_head(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let h = hw.find_modified_head(HEAD_IP);
    let p = hw.get_head(h).position();
    hw.set_reg(REG_CX, p);
    Ok(true)
}

/// `Inst_IfLabel`
fn inst_if_label(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    hw.rotate_label();
    let c = hw.label() != hw.read_label_ref();
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfLabelDirect`
fn inst_if_label_direct(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    let c = hw.label() != hw.read_label_ref();
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfLabel2`
fn inst_if_label2(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.rl(world);
    hw.rotate_label();
    if hw.label() != hw.read_label_ref() {
        hw.ip_advance();
        let is = hw.base.inst_set.clone();
        if is.is_nop(hw.ip_head().get_next_inst(&hw.memory, &is)) {
            hw.ip_advance();
        }
    }
    Ok(true)
}

/// `Inst_HeadDivide`
fn inst_head_divide(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.head_divide_mut(org, world, 1.0)
}

macro_rules! head_divide_n {
    ($($name:ident = $mult:expr, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            fn $name(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
                hw.head_divide_mut(org, world, $mult)
            }
        )*
    };
}

head_divide_n! {
    inst_head_divide2 = 2.0, "`Inst_HeadDivide2`";
    inst_head_divide3 = 3.0, "`Inst_HeadDivide3`";
    inst_head_divide4 = 4.0, "`Inst_HeadDivide4`";
    inst_head_divide5 = 5.0, "`Inst_HeadDivide5`";
    inst_head_divide6 = 6.0, "`Inst_HeadDivide6`";
    inst_head_divide7 = 7.0, "`Inst_HeadDivide7`";
    inst_head_divide8 = 8.0, "`Inst_HeadDivide8`";
    inst_head_divide9 = 9.0, "`Inst_HeadDivide9`";
    inst_head_divide10 = 10.0, "`Inst_HeadDivide10`";
    inst_head_divide16 = 16.0, "`Inst_HeadDivide16`";
    inst_head_divide32 = 32.0, "`Inst_HeadDivide32`";
    inst_head_divide50 = 50.0, "`Inst_HeadDivide50`";
    inst_head_divide100 = 100.0, "`Inst_HeadDivide100`";
    inst_head_divide500 = 500.0, "`Inst_HeadDivide500`";
    inst_head_divide1000 = 1000.0, "`Inst_HeadDivide1000`";
    inst_head_divide5000 = 5000.0, "`Inst_HeadDivide5000`";
    inst_head_divide10000 = 10000.0, "`Inst_HeadDivide10000`";
    inst_head_divide50000 = 50000.0, "`Inst_HeadDivide50000`";
    inst_head_divide0_5 = 0.5, "`Inst_HeadDivide0_5`";
    inst_head_divide0_1 = 0.1, "`Inst_HeadDivide0_1`";
    inst_head_divide0_05 = 0.05, "`Inst_HeadDivide0_05`";
    inst_head_divide0_01 = 0.01, "`Inst_HeadDivide0_01`";
    inst_head_divide0_001 = 0.001, "`Inst_HeadDivide0_001`";
}

/// `Inst_HeadDivideRS`
fn inst_head_divide_rs(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.head_divide_rs(org, world, 0)
}

/// `Inst_HeadDivide1RS`
fn inst_head_divide1_rs(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.head_divide_rs(org, world, 1)
}

/// `Inst_HeadDivide2RS`
fn inst_head_divide2_rs(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.head_divide_rs(org, world, 2)
}

/// `Inst_HeadDivideSex`
fn inst_head_divide_sex(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    org.phenotype.divide_sex = true;
    org.phenotype.cross_num = 1;
    inst_head_divide(hw, org, world)
}

/// `Inst_HeadDivideAsex`
fn inst_head_divide_asex(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    org.phenotype.divide_sex = false;
    org.phenotype.cross_num = 0;
    inst_head_divide(hw, org, world)
}

/// `Inst_HeadDivideAsexWait`
fn inst_head_divide_asex_wait(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    org.phenotype.divide_sex = true;
    org.phenotype.cross_num = 0;
    inst_head_divide(hw, org, world)
}

/// `Inst_HeadDivideMateSelect`
fn inst_head_divide_mate_select(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.rl(world);
    org.phenotype.mate_select_id = hw.label().as_int(NUM_NOPS);
    org.phenotype.divide_sex = true;
    org.phenotype.cross_num = 1;
    inst_head_divide(hw, org, world)
}

/// `Inst_HeadRead`
fn inst_head_read(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let head_id = hw.find_modified_head(HEAD_READ);
    hw.head_adjust(head_id);
    let read_inst = if HardwareCpu::test_copy_mut(org, world) {
        hw.base.inst_set.random_inst(world.rng()).op()
    } else {
        hw.get_head(head_id).get_inst(&hw.memory).op()
    };
    hw.set_reg(REG_BX, read_inst);
    hw.read_inst(read_inst);
    if hw.slip_read_head && org.mut_rates.test_copy_slip(world.rng()) {
        let pos = world.rng().get_int(hw.memory.size());
        hw.head_set_pos(head_id, pos);
    }
    hw.head_advance(head_id);
    Ok(true)
}

/// `Inst_HeadWrite`
fn inst_head_write(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let head_id = hw.find_modified_head(HEAD_WRITE);
    hw.head_adjust(head_id);
    let active_head = hw.get_head(head_id);
    let mut value = hw.reg(REG_BX);
    if value < 0 || value >= hw.base.inst_set.size() {
        value = 0;
    }
    active_head.set_inst(&mut hw.memory, Instruction::new(value));
    active_head.set_flag_copied(&mut hw.memory);

    if org.mut_rates.test_copy_ins(world.rng()) {
        let inst = hw.base.inst_set.random_inst(world.rng());
        active_head.insert_inst(&mut hw.memory, inst);
    }
    if org.mut_rates.test_copy_del(world.rng()) {
        active_head.remove_inst(&mut hw.memory);
    }
    if org.mut_rates.test_copy_uniform(world.rng()) {
        hw.uniform_copy_mutation(world, &active_head);
    }
    if !hw.slip_read_head && org.mut_rates.test_copy_slip(world.rng()) {
        hw.slip_mutation(world, active_head.position())?;
    }
    hw.head_advance(head_id);
    Ok(true)
}

/// `Inst_HeadCopy`
fn inst_head_copy(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let size = hw.memory.size();
    hw.head_ref(HEAD_READ).adjust(size, 1);
    hw.head_ref(HEAD_WRITE).adjust(size, 1);
    let read_head = hw.get_head(HEAD_READ);
    let write_head = hw.get_head(HEAD_WRITE);

    let mut read_inst = read_head.get_inst(&hw.memory);
    hw.read_inst(read_inst.op());
    if HardwareCpu::test_copy_mut(org, world)
        && !in_no_mut_list(world.config(), read_head.get_inst(&hw.memory))
    {
        read_inst = hw.base.inst_set.random_inst(world.rng());
        write_head.set_flag_mutated(&mut hw.memory);
        write_head.set_flag_copy_mut(&mut hw.memory);
    }
    write_head.set_inst(&mut hw.memory, read_inst);
    write_head.set_flag_copied(&mut hw.memory);

    if org.mut_rates.test_copy_ins(world.rng()) {
        let inst = hw.base.inst_set.random_inst(world.rng());
        write_head.insert_inst(&mut hw.memory, inst);
    }
    if org.mut_rates.test_copy_del(world.rng()) {
        write_head.remove_inst(&mut hw.memory);
    }
    if org.mut_rates.test_copy_uniform(world.rng()) {
        hw.uniform_copy_mutation(world, &write_head);
    }
    if org.mut_rates.test_copy_slip(world.rng()) {
        if hw.slip_read_head {
            let pos = world.rng().get_int(hw.memory.size());
            hw.head_set_pos(HEAD_READ, pos);
        } else {
            hw.slip_mutation(world, write_head.position())?;
        }
    }
    hw.head_advance(HEAD_READ);
    hw.head_advance(HEAD_WRITE);
    Ok(true)
}

/// `Inst_HeadCopy_ifResource`
fn inst_head_copy_if_resource(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.head_adjust(HEAD_READ);
    let resource = hw.get_head(HEAD_READ).get_inst(&hw.memory).op();
    if resource < 52 && org.rbin(resource) >= 1.0 {
        org.add_to_rbin(resource, -1.0);
        return inst_head_copy(hw, org, world);
    }
    Ok(true)
}

macro_rules! head_copy_n {
    ($($name:ident = $red:expr, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            fn $name(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
                hw.head_copy_error_correct(org, world, $red)
            }
        )*
    };
}

head_copy_n! {
    inst_head_copy2 = 2.0, "`Inst_HeadCopy2`";
    inst_head_copy3 = 3.0, "`Inst_HeadCopy3`";
    inst_head_copy4 = 4.0, "`Inst_HeadCopy4`";
    inst_head_copy5 = 5.0, "`Inst_HeadCopy5`";
    inst_head_copy6 = 6.0, "`Inst_HeadCopy6`";
    inst_head_copy7 = 7.0, "`Inst_HeadCopy7`";
    inst_head_copy8 = 8.0, "`Inst_HeadCopy8`";
    inst_head_copy9 = 9.0, "`Inst_HeadCopy9`";
    inst_head_copy10 = 10.0, "`Inst_HeadCopy10`";
}

/// `Inst_HeadSearch` (`direct = false`) / `Inst_HeadSearchDirect` (`true`).
fn head_search(hw: &mut HardwareCpu, world: &mut dyn OrgInterface, direct: bool) -> R {
    hw.rl(world);
    let found_pos = if direct {
        hw.find_label(1)
    } else {
        hw.rotate_label();
        hw.find_label(0)
    };
    let search_size = found_pos.position() - hw.ip_head().position();
    hw.set_reg(REG_BX, search_size);
    let l = hw.label().get_size();
    hw.set_reg(REG_CX, l);
    hw.head_set_from(HEAD_FLOW, &found_pos);
    hw.head_advance(HEAD_FLOW);
    Ok(true)
}

/// `Inst_HeadSearch`
fn inst_head_search(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    head_search(hw, world, false)
}

/// `Inst_HeadSearchDirect`
fn inst_head_search_direct(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    head_search(hw, world, true)
}

/// `Inst_SetFlow`
fn inst_set_flow(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_CX);
    let v = hw.reg(r);
    hw.head_set_pos(HEAD_FLOW, v);
    Ok(true)
}

// ---- Organism state ----

/// `Inst_Sleep`: executed at the end of a sleep cycle; with
/// `APPLY_ENERGY_METHOD=2` the stored energy is refreshed into the merit.
fn inst_sleep(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    org.is_sleeping = false;
    world.end_sleep();
    if world.config().apply_energy_method == 2 {
        crate::organism::apply_stored_energy_merit(org, world, "sleep")?;
    }
    Ok(true)
}

/// `Inst_GetUpdate`
fn inst_get_update(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let u = world.update();
    hw.set_reg(r, u);
    Ok(true)
}

/// `Inst_SetOpinion`
fn inst_set_opinion(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = hw.reg(r);
    world.set_opinion(v, org);
    Ok(true)
}

/// `Inst_GetOpinion`
fn inst_get_opinion(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if world.has_opinion(org) {
        let opinion_reg = hw.find_modified_register(REG_BX);
        let age_reg = HardwareCpu::find_next_register(opinion_reg);
        let (op, upd) = org.get_opinion();
        hw.set_reg(opinion_reg, op);
        hw.set_reg(age_reg, world.update().wrapping_sub(upd));
    }
    Ok(true)
}

/// `Inst_GetOpinionOnly_ZeroIfNone`
fn inst_get_opinion_only_zero_if_none(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = if world.has_opinion(org) {
        org.get_opinion().0
    } else {
        0
    };
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_ClearOpinion`
fn inst_clear_opinion(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    world.clear_opinion(org);
    Ok(true)
}

/// `Inst_IfOpinionSet`
fn inst_if_opinion_set(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let c = !world.has_opinion(org);
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_IfOpinionNotSet`
fn inst_if_opinion_not_set(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let c = world.has_opinion(org);
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_CollectCellData`
fn inst_collect_cell_data(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world.get_cell_data();
    hw.set_reg(r, v);
    hw.last_cell_data = (true, v);
    Ok(true)
}

/// `Inst_IfCellDataChanged`
fn inst_if_cell_data_changed(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let c = !hw.last_cell_data.0 || hw.last_cell_data.1 == world.get_cell_data();
    hw.skip_if(c);
    Ok(true)
}

/// `Inst_ReadCellData`
fn inst_read_cell_data(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world.get_cell_data();
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_ReadFacedCellDataOrgID`
fn inst_read_faced_cell_data_org_id(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world.get_faced_cell_data_org_id();
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_ReadFacedCellDataFreshness`
fn inst_read_faced_cell_data_freshness(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = world
        .update()
        .wrapping_sub(world.get_faced_cell_data_update());
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_MarkCellWithID`
fn inst_mark_cell_with_id(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let id = org.id;
    world.set_cell_data(org, id);
    Ok(true)
}

/// `Inst_GetResStored`
fn inst_get_res_stored(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    let resource = world.config().collect_specific_resource;
    // `avd_cpu_inst_get_res_stored` in the reference binary: Rust, so the
    // multiply and the subtraction are rounded separately.
    let res_stored = (org.rbin(resource) * 100.0 - 0.5) as i32;
    hw.set_reg(r, res_stored);
    Ok(true)
}

/// `Inst_GetID`
fn inst_get_id(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, org.id);
    Ok(true)
}

/// `Inst_GetFacedOrgID`
fn inst_get_faced_org_id(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if !world.is_neighbor_cell_occupied() {
        return Ok(false);
    }
    let Some((id, dead)) = with_neighbor_state(world, org, |n| (n.id, n.is_dead)) else {
        return Ok(false);
    };
    if dead {
        return Ok(false);
    }
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, id);
    Ok(true)
}

/// `Inst_HardReset`
fn inst_hard_reset(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    hw.full_reset(org, world);
    hw.advance_ip = false;
    Ok(true)
}

/// `Inst_GetCycles`
fn inst_get_cycles(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    let v = hw.cycle_counter as i32;
    hw.set_reg(r, v);
    Ok(true)
}

/// `Inst_GetTimeUsed`
fn inst_get_time_used(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, org.phenotype.time_used);
    Ok(true)
}

/// `Inst_ApplyPointMutations`
fn inst_apply_point_mutations(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.config().point_mut_repair_start == 0 {
        let p = world.config().inst_point_mut_prob;
        let n = hw.point_mutate(org, world, p);
        org.num_point_mut = org.num_point_mut.wrapping_add(n);
    } else {
        let cost = world.config().inst_point_repair_cost;
        hw.base.task_switching_cost = hw.base.task_switching_cost.wrapping_add(cost);
    }
    Ok(true)
}

/// `Inst_ApplyVaryingPointMutations`
fn inst_apply_varying_point_mutations(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let last_task = org.phenotype.last_task_id;
    if last_task != -1 {
        let cfg = world.config();
        let p = cfg.inst_point_mut_prob * cfg.inst_point_mut_slope * f64::from(last_task);
        let n = hw.point_mutate(org, world, p);
        org.num_point_mut = org.num_point_mut.wrapping_add(n);
    }
    Ok(true)
}

/// `Inst_JoinGermline`
fn inst_join_germline(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.phenotype.is_germ_cell = true;
    Ok(true)
}

/// `Inst_ExitGermline`
fn inst_exit_germline(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    org.phenotype.is_germ_cell = false;
    Ok(true)
}

/// `Inst_RepairPointMutOn`
fn inst_repair_point_mut_on(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    org.repair = 1;
    Ok(true)
}

/// `Inst_RepairPointMutOff`
fn inst_repair_point_mut_off(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    org.repair = 0;
    Ok(true)
}

/// `Inst_DoubleEnergyUsage`
fn inst_double_energy_usage(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    Ok(energy_usage_inst(org, world, 0))
}

/// `Inst_HalveEnergyUsage`
fn inst_halve_energy_usage(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    Ok(energy_usage_inst(org, world, 1))
}

/// `Inst_DefaultEnergyUsage`
fn inst_default_energy_usage(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    Ok(energy_usage_inst(org, world, 2))
}

// ---- Mating types ----

/// `Inst_SetMatingTypeMale`
fn inst_set_mating_type_male(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if org.phenotype.mating_type == MATING_TYPE_FEMALE {
        return Ok(false);
    }
    let old_type = if org.phenotype.mating_type == MATING_TYPE_JUVENILE {
        2
    } else {
        1
    };
    HardwareCpu::change_mating_group(org, world, old_type, 1);
    org.phenotype.mating_type = MATING_TYPE_MALE;
    Ok(true)
}

/// `Inst_SetMatingTypeFemale`
fn inst_set_mating_type_female(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if org.phenotype.mating_type == MATING_TYPE_MALE {
        return Ok(false);
    }
    let old_type = if org.phenotype.mating_type == MATING_TYPE_JUVENILE {
        2
    } else {
        0
    };
    HardwareCpu::change_mating_group(org, world, old_type, 0);
    org.phenotype.mating_type = MATING_TYPE_FEMALE;
    Ok(true)
}

/// `Inst_SetMatingTypeJuvenile`
fn inst_set_mating_type_juvenile(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let old_type = if org.phenotype.mating_type == MATING_TYPE_MALE {
        1
    } else if org.phenotype.mating_type == MATING_TYPE_JUVENILE {
        2
    } else {
        0
    };
    HardwareCpu::change_mating_group(org, world, old_type, 2);
    org.phenotype.mating_type = MATING_TYPE_JUVENILE;
    Ok(true)
}

/// `Inst_DivideSexMatingType`
fn inst_divide_sex_mating_type(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if org.phenotype.mating_type == MATING_TYPE_JUVENILE {
        return Ok(false);
    }
    inst_head_divide_sex(hw, org, world)
}

macro_rules! if_mating {
    ($name:ident, $t:expr, $doc:literal) => {
        #[doc = $doc]
        fn $name(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
            hw.skip_if(org.phenotype.mating_type != $t);
            Ok(true)
        }
    };
}

if_mating!(
    inst_if_mating_type_male,
    MATING_TYPE_MALE,
    "`Inst_IfMatingTypeMale`"
);
if_mating!(
    inst_if_mating_type_female,
    MATING_TYPE_FEMALE,
    "`Inst_IfMatingTypeFemale`"
);
if_mating!(
    inst_if_mating_type_juvenile,
    MATING_TYPE_JUVENILE,
    "`Inst_IfMatingTypeJuvenile`"
);

/// `Inst_IncrementMatingDisplayA`
fn inst_increment_mating_display_a(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    org.phenotype.cur_mating_display_a = org.phenotype.cur_mating_display_a.wrapping_add(1);
    Ok(true)
}

/// `Inst_IncrementMatingDisplayB`
fn inst_increment_mating_display_b(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    org.phenotype.cur_mating_display_b = org.phenotype.cur_mating_display_b.wrapping_add(1);
    Ok(true)
}

/// `Inst_SetMatingDisplayA`
fn inst_set_mating_display_a(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    org.phenotype.cur_mating_display_a = hw.reg(r);
    Ok(true)
}

/// `Inst_SetMatingDisplayB`
fn inst_set_mating_display_b(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    _: &mut dyn OrgInterface,
) -> R {
    let r = hw.find_modified_register(REG_BX);
    org.phenotype.cur_mating_display_b = hw.reg(r);
    Ok(true)
}

macro_rules! mate_pref {
    ($($name:ident = $p:expr, $doc:literal;)*) => {
        $(
            #[doc = $doc]
            fn $name(_: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
                org.phenotype.mate_preference = $p;
                Ok(true)
            }
        )*
    };
}

mate_pref! {
    inst_set_mate_preference_random = MATE_PREFERENCE_RANDOM, "`Inst_SetMatePreferenceRandom`";
    inst_set_mate_preference_highest_display_a = MATE_PREFERENCE_HIGHEST_DISPLAY_A, "`Inst_SetMatePreferenceHighestDisplayA`";
    inst_set_mate_preference_highest_display_b = MATE_PREFERENCE_HIGHEST_DISPLAY_B, "`Inst_SetMatePreferenceHighestDisplayB`";
    inst_set_mate_preference_highest_merit = MATE_PREFERENCE_HIGHEST_MERIT, "`Inst_SetMatePreferenceHighestMerit`";
    inst_set_mate_preference_lowest_display_a = MATE_PREFERENCE_LOWEST_DISPLAY_A, "`Inst_SetMatePreferenceLowestDisplayA`";
    inst_set_mate_preference_lowest_display_b = MATE_PREFERENCE_LOWEST_DISPLAY_B, "`Inst_SetMatePreferenceLowestDisplayB`";
    inst_set_mate_preference_lowest_merit = MATE_PREFERENCE_LOWEST_MERIT, "`Inst_SetMatePreferenceLowestMerit`";
}

// ----------------------------------------------------------------------
// Dispatch table
// ----------------------------------------------------------------------

/// The ported handler of `cHardwareCPU::<method>`, or `None` when the
/// method is registered but not ported.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    core_handler(method)
        .or_else(|| crate::hardware_cpu_insts_res::handler(method))
        .or_else(|| crate::hardware_cpu_insts_move::handler(method))
        .or_else(|| crate::hardware_cpu_insts_donate::handler(method))
        .or_else(|| crate::hardware_cpu_insts_msg::handler(method))
        .or_else(|| crate::hardware_cpu_insts_group::handler(method))
        .or_else(|| crate::hardware_cpu_insts_energy::handler(method))
        .or_else(|| crate::hardware_cpu_insts_misc::handler(method))
}

/// The handlers defined in this module.
fn core_handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_Nop" | "Inst_EndIf" | "Inst_NandTreatable" => inst_nop,
        "Inst_If0" => inst_if0,
        "Inst_IfNot0" => inst_if_not0,
        "Inst_If0_defaultAX" => inst_if0_default_ax,
        "Inst_IfNot0_defaultAX" => inst_if_not0_default_ax,
        "Inst_IfEqu" => inst_if_equ,
        "Inst_IfNEqu" => inst_if_n_equ,
        "Inst_IfGr0" => inst_if_gr0,
        "Inst_IfGr" => inst_if_gr,
        "Inst_IfGrEqu0" => inst_if_gr_equ0,
        "Inst_IfGrEqu" => inst_if_gr_equ,
        "Inst_IfLess0" => inst_if_less0,
        "Inst_IfLess" => inst_if_less,
        "Inst_IfLsEqu0" => inst_if_ls_equ0,
        "Inst_IfLsEqu" => inst_if_ls_equ,
        "Inst_IfANotEqB" => inst_if_a_not_eq_b,
        "Inst_IfBNotEqC" => inst_if_b_not_eq_c,
        "Inst_IfANotEqC" => inst_if_a_not_eq_c,
        "Inst_IfBit1" => inst_if_bit1,
        "Inst_IfGrX" => inst_if_gr_x,
        "Inst_IfEquX" => inst_if_equ_x,
        "Inst_IfGerm" => inst_if_germ,
        "Inst_IfSoma" => inst_if_soma,
        "Inst_IfP0p125" => inst_if_p0p125,
        "Inst_IfP0p25" => inst_if_p0p25,
        "Inst_IfP0p50" => inst_if_p0p50,
        "Inst_IfP0p75" => inst_if_p0p75,
        "Inst_IfLessEnd" => inst_if_less_end,
        "Inst_IfNotEqualEnd" => inst_if_not_equal_end,
        "Inst_IfGrtEquEnd" => inst_if_grt_equ_end,
        "Inst_Else" => inst_else,
        "Inst_JumpF" => inst_jump_f,
        "Inst_JumpB" => inst_jump_b,
        "Inst_Call" => inst_call,
        "Inst_Return" => inst_return,
        "Inst_Throw" => inst_throw,
        "Inst_ThrowIf0" => inst_throw_if0,
        "Inst_ThrowIfNot0" => inst_throw_if_not0,
        "Inst_Catch" | "Inst_Label" | "Inst_Transposon" => inst_read_label_only,
        "Inst_Goto" => inst_goto,
        "Inst_GotoIf0" => inst_goto_if0,
        "Inst_GotoIfNot0" => inst_goto_if_not0,
        "Inst_Pop" => inst_pop,
        "Inst_Push" => inst_push,
        "Inst_SwitchStack" => inst_switch_stack,
        "Inst_FlipStack" => inst_flip_stack,
        "Inst_Swap" => inst_swap,
        "Inst_SwapAB" => inst_swap_ab,
        "Inst_SwapBC" => inst_swap_bc,
        "Inst_SwapAC" => inst_swap_ac,
        "Inst_CopyReg" => inst_copy_reg,
        "Inst_CopyRegAB" => inst_copy_reg_ab,
        "Inst_CopyRegAC" => inst_copy_reg_ac,
        "Inst_CopyRegBA" => inst_copy_reg_ba,
        "Inst_CopyRegBC" => inst_copy_reg_bc,
        "Inst_CopyRegCA" => inst_copy_reg_ca,
        "Inst_CopyRegCB" => inst_copy_reg_cb,
        "Inst_Reset" => inst_reset,
        "Inst_PopA" => inst_pop_a,
        "Inst_PopB" => inst_pop_b,
        "Inst_PopC" => inst_pop_c,
        "Inst_PushA" => inst_push_a,
        "Inst_PushB" => inst_push_b,
        "Inst_PushC" => inst_push_c,
        "Inst_ShiftR" => inst_shift_r,
        "Inst_ShiftL" => inst_shift_l,
        "Inst_Bit1" => inst_bit1,
        "Inst_SetNum" => inst_set_num,
        "Inst_ValGrey" => inst_val_grey,
        "Inst_ValDir" => inst_val_dir,
        "Inst_ValAddP" => inst_val_add_p,
        "Inst_ValFib" => inst_val_fib,
        "Inst_ValPolyC" => inst_val_poly_c,
        "Inst_Inc" => inst_inc,
        "Inst_Dec" => inst_dec,
        "Inst_Zero" => inst_zero,
        "Inst_One" => inst_one,
        "Inst_All1s" => inst_all1s,
        "Inst_Neg" => inst_neg,
        "Inst_Square" => inst_square,
        "Inst_Sqrt" => inst_sqrt,
        "Inst_Log" => inst_log,
        "Inst_Log10" => inst_log10,
        "Inst_Not" => inst_not,
        "Inst_Add" => inst_add,
        "Inst_Sub" => inst_sub,
        "Inst_Mult" => inst_mult,
        "Inst_Div" => inst_div,
        "Inst_Mod" => inst_mod,
        "Inst_Nand" => inst_nand,
        "Inst_Or" => inst_or,
        "Inst_Nor" => inst_nor,
        "Inst_And" => inst_and,
        "Inst_Order" => inst_order,
        "Inst_Xor" => inst_xor,
        "Inst_Setbit" => inst_setbit,
        "Inst_Clearbit" => inst_clearbit,
        "Inst_Copy" => inst_copy,
        "Inst_ReadInst" => inst_read_inst,
        "Inst_WriteInst" => inst_write_inst,
        "Inst_StackReadInst" => inst_stack_read_inst,
        "Inst_StackWriteInst" => inst_stack_write_inst,
        "Inst_Compare" => inst_compare,
        "Inst_IfNCpy" => inst_if_n_cpy,
        "Inst_Allocate" => inst_allocate,
        "Inst_Divide" => inst_divide,
        "Inst_DivideRS" => inst_divide_rs,
        "Inst_CAlloc" => inst_c_alloc,
        "Inst_CDivide" => inst_c_divide,
        "Inst_SearchF" => inst_search_f,
        "Inst_SearchB" => inst_search_b,
        "Inst_MemSize" => inst_mem_size,
        "Inst_TaskGet" => inst_task_get,
        "Inst_TaskGet2" => inst_task_get2,
        "Inst_TaskStackGet" => inst_task_stack_get,
        "Inst_TaskStackLoad" => inst_task_stack_load,
        "Inst_TaskPut" => inst_task_put,
        "Inst_TaskPutResetInputs" => inst_task_put_reset_inputs,
        "Inst_TaskIO" => inst_task_io,
        "Inst_TaskIO_Feedback" => inst_task_io_feedback,
        "Inst_TaskIO_BonusCost_0_001" => inst_task_io_bonus_cost_0_001,
        "Inst_MatchStrings" => inst_match_strings,
        "Inst_Send" => inst_send,
        "Inst_Receive" => inst_receive,
        "Inst_IOBufAdd1" => inst_io_buf_add1,
        "Inst_IOBufAdd0" => inst_io_buf_add0,
        "Inst_RotateL" => inst_rotate_l,
        "Inst_RotateR" => inst_rotate_r,
        "Inst_RotateLeftOne" => inst_rotate_left_one,
        "Inst_RotateRightOne" => inst_rotate_right_one,
        "Inst_RotateLabel" => inst_rotate_label,
        "Inst_RotateUnoccupiedCell" => inst_rotate_unoccupied_cell,
        "Inst_RotateNextUnoccupiedCell" => inst_rotate_next_unoccupied_cell,
        "Inst_RotateOccupiedCell" => inst_rotate_occupied_cell,
        "Inst_RotateNextOccupiedCell" => inst_rotate_next_occupied_cell,
        "Inst_RotateEventCell" => inst_rotate_event_cell,
        "Inst_RotateUphill" => inst_rotate_uphill,
        "Inst_RotateHome" => inst_rotate_home,
        "Inst_SetCopyMut" => inst_set_copy_mut,
        "Inst_ModCopyMut" => inst_mod_copy_mut,
        "Inst_GetCellPosition" => inst_get_cell_position,
        "Inst_GetCellPositionX" => inst_get_cell_position_x,
        "Inst_GetCellPositionY" => inst_get_cell_position_y,
        "Inst_GetDirectionOffNorth" => inst_get_direction_off_north,
        "Inst_GetNortherly" => inst_get_northerly,
        "Inst_GetEasterly" => inst_get_easterly,
        "Inst_ZeroEasterly" => inst_zero_easterly,
        "Inst_ZeroNortherly" => inst_zero_northerly,
        "Inst_ForkThread" => inst_fork_thread,
        "Inst_ForkThreadLabel" => inst_fork_thread_label,
        "Inst_ForkThreadLabelIfNot0" => inst_fork_thread_label_if_not0,
        "Inst_ForkThreadLabelIf0" => inst_fork_thread_label_if0,
        "Inst_KillThread" => inst_kill_thread,
        "Inst_ThreadID" => inst_thread_id,
        "Inst_MaxAlloc" => inst_max_alloc,
        "Inst_MaxAllocMoveWriteHead" => inst_max_alloc_move_write_head,
        "Inst_HeadDivide" | "Inst_HeadDivide1" => inst_head_divide,
        "Inst_HeadDivideRS" => inst_head_divide_rs,
        "Inst_HeadDivide1RS" => inst_head_divide1_rs,
        "Inst_HeadDivide2RS" => inst_head_divide2_rs,
        "Inst_HeadRead" => inst_head_read,
        "Inst_HeadWrite" => inst_head_write,
        "Inst_HeadCopy" => inst_head_copy,
        "Inst_HeadSearch" => inst_head_search,
        "Inst_HeadSearchDirect" => inst_head_search_direct,
        "Inst_HeadPush" => inst_head_push,
        "Inst_HeadPop" => inst_head_pop,
        "Inst_SetHead" => inst_set_head,
        "Inst_AdvanceHead" => inst_advance_head,
        "Inst_MoveHead" => inst_move_head,
        "Inst_JumpHead" => inst_jump_head,
        "Inst_GetHead" => inst_get_head,
        "Inst_IfLabel" => inst_if_label,
        "Inst_IfLabelDirect" => inst_if_label_direct,
        "Inst_IfLabel2" => inst_if_label2,
        "Inst_SetFlow" => inst_set_flow,
        "Inst_HeadCopy_ifResource" => inst_head_copy_if_resource,
        "Inst_HeadCopy2" => inst_head_copy2,
        "Inst_HeadCopy3" => inst_head_copy3,
        "Inst_HeadCopy4" => inst_head_copy4,
        "Inst_HeadCopy5" => inst_head_copy5,
        "Inst_HeadCopy6" => inst_head_copy6,
        "Inst_HeadCopy7" => inst_head_copy7,
        "Inst_HeadCopy8" => inst_head_copy8,
        "Inst_HeadCopy9" => inst_head_copy9,
        "Inst_HeadCopy10" => inst_head_copy10,
        "Inst_HeadDivideSex" => inst_head_divide_sex,
        "Inst_HeadDivideAsex" => inst_head_divide_asex,
        "Inst_HeadDivideAsexWait" => inst_head_divide_asex_wait,
        "Inst_HeadDivideMateSelect" => inst_head_divide_mate_select,
        "Inst_HeadDivide2" => inst_head_divide2,
        "Inst_HeadDivide3" => inst_head_divide3,
        "Inst_HeadDivide4" => inst_head_divide4,
        "Inst_HeadDivide5" => inst_head_divide5,
        "Inst_HeadDivide6" => inst_head_divide6,
        "Inst_HeadDivide7" => inst_head_divide7,
        "Inst_HeadDivide8" => inst_head_divide8,
        "Inst_HeadDivide9" => inst_head_divide9,
        "Inst_HeadDivide10" => inst_head_divide10,
        "Inst_HeadDivide16" => inst_head_divide16,
        "Inst_HeadDivide32" => inst_head_divide32,
        "Inst_HeadDivide50" => inst_head_divide50,
        "Inst_HeadDivide100" => inst_head_divide100,
        "Inst_HeadDivide500" => inst_head_divide500,
        "Inst_HeadDivide1000" => inst_head_divide1000,
        "Inst_HeadDivide5000" => inst_head_divide5000,
        "Inst_HeadDivide10000" => inst_head_divide10000,
        "Inst_HeadDivide50000" => inst_head_divide50000,
        "Inst_HeadDivide0_5" => inst_head_divide0_5,
        "Inst_HeadDivide0_1" => inst_head_divide0_1,
        "Inst_HeadDivide0_05" => inst_head_divide0_05,
        "Inst_HeadDivide0_01" => inst_head_divide0_01,
        "Inst_HeadDivide0_001" => inst_head_divide0_001,
        "Inst_SetMatingTypeMale" => inst_set_mating_type_male,
        "Inst_SetMatingTypeFemale" => inst_set_mating_type_female,
        "Inst_SetMatingTypeJuvenile" => inst_set_mating_type_juvenile,
        "Inst_DivideSexMatingType" => inst_divide_sex_mating_type,
        "Inst_IfMatingTypeMale" => inst_if_mating_type_male,
        "Inst_IfMatingTypeFemale" => inst_if_mating_type_female,
        "Inst_IfMatingTypeJuvenile" => inst_if_mating_type_juvenile,
        "Inst_IncrementMatingDisplayA" => inst_increment_mating_display_a,
        "Inst_IncrementMatingDisplayB" => inst_increment_mating_display_b,
        "Inst_SetMatingDisplayA" => inst_set_mating_display_a,
        "Inst_SetMatingDisplayB" => inst_set_mating_display_b,
        "Inst_SetMatePreferenceRandom" => inst_set_mate_preference_random,
        "Inst_SetMatePreferenceHighestDisplayA" => inst_set_mate_preference_highest_display_a,
        "Inst_SetMatePreferenceHighestDisplayB" => inst_set_mate_preference_highest_display_b,
        "Inst_SetMatePreferenceHighestMerit" => inst_set_mate_preference_highest_merit,
        "Inst_SetMatePreferenceLowestDisplayA" => inst_set_mate_preference_lowest_display_a,
        "Inst_SetMatePreferenceLowestDisplayB" => inst_set_mate_preference_lowest_display_b,
        "Inst_SetMatePreferenceLowestMerit" => inst_set_mate_preference_lowest_merit,
        "Inst_Repro" => inst_repro,
        "Inst_ReproSex" => inst_repro_sex,
        "Inst_ReproGermFlag" => inst_repro_germ_flag,
        "Inst_ConditionalRepro" => inst_conditional_repro,
        "Inst_TaskPutRepro" => inst_task_put_repro,
        "Inst_TaskPutResetInputsRepro" => inst_task_put_reset_inputs_repro,
        "Inst_Sterilize" => inst_sterilize,
        "Inst_NopPre" => inst_nop_pre,
        "Inst_NopPost" => inst_nop_post,
        "Inst_Die" => inst_die,
        "Inst_Prob_Die" => inst_prob_die,
        "Inst_Poison" => inst_poison,
        "Inst_Sleep" => inst_sleep,
        "Inst_GetUpdate" => inst_get_update,
        "Inst_BitConsensus" => inst_bit_consensus,
        "Inst_BitConsensus24" => inst_bit_consensus24,
        "Inst_IfConsensus" => inst_if_consensus,
        "Inst_IfConsensus24" => inst_if_consensus24,
        "Inst_IfLessConsensus" => inst_if_less_consensus,
        "Inst_IfLessConsensus24" => inst_if_less_consensus24,
        "Inst_MaskSignBit" => inst_mask_sign_bit,
        "Inst_MaskOffLower16Bits" => inst_mask_off16,
        "Inst_MaskOffLower16Bits_defaultAX" => inst_mask_off16_ax,
        "Inst_MaskOffLower15Bits" => inst_mask_off15,
        "Inst_MaskOffLower15Bits_defaultAX" => inst_mask_off15_ax,
        "Inst_MaskOffLower14Bits" => inst_mask_off14,
        "Inst_MaskOffLower14Bits_defaultAX" => inst_mask_off14_ax,
        "Inst_MaskOffLower13Bits" => inst_mask_off13,
        "Inst_MaskOffLower13Bits_defaultAX" => inst_mask_off13_ax,
        "Inst_MaskOffLower12Bits" => inst_mask_off12,
        "Inst_MaskOffLower12Bits_defaultAX" => inst_mask_off12_ax,
        "Inst_MaskOffLower8Bits" => inst_mask_off8,
        "Inst_MaskOffLower8Bits_defaultAX" => inst_mask_off8_ax,
        "Inst_MaskOffLower4Bits" => inst_mask_off4,
        "Inst_MaskOffLower4Bits_defaultAX" => inst_mask_off4_ax,
        "Inst_DoubleEnergyUsage" => inst_double_energy_usage,
        "Inst_HalveEnergyUsage" => inst_halve_energy_usage,
        "Inst_DefaultEnergyUsage" => inst_default_energy_usage,
        "Inst_Skip" => inst_skip,
        "Inst_SetOpinion" => inst_set_opinion,
        "Inst_GetOpinion" => inst_get_opinion,
        "Inst_GetOpinionOnly_ZeroIfNone" => inst_get_opinion_only_zero_if_none,
        "Inst_ClearOpinion" => inst_clear_opinion,
        "Inst_IfOpinionSet" => inst_if_opinion_set,
        "Inst_IfOpinionNotSet" => inst_if_opinion_not_set,
        "Inst_IfCellDataChanged" => inst_if_cell_data_changed,
        "Inst_CollectCellData" => inst_collect_cell_data,
        "Inst_ReadCellData" => inst_read_cell_data,
        "Inst_ReadFacedCellDataOrgID" => inst_read_faced_cell_data_org_id,
        "Inst_ReadFacedCellDataFreshness" => inst_read_faced_cell_data_freshness,
        "Inst_MarkCellWithID" => inst_mark_cell_with_id,
        "Inst_GetResStored" => inst_get_res_stored,
        "Inst_GetID" => inst_get_id,
        "Inst_GetFacedOrgID" => inst_get_faced_org_id,
        "Inst_HardReset" => inst_hard_reset,
        "Inst_GetCycles" => inst_get_cycles,
        "Inst_GetTimeUsed" => inst_get_time_used,
        "Inst_ApplyPointMutations" => inst_apply_point_mutations,
        "Inst_ApplyVaryingPointMutations" => inst_apply_varying_point_mutations,
        "Inst_JoinGermline" => inst_join_germline,
        "Inst_ExitGermline" => inst_exit_germline,
        "Inst_RepairPointMutOn" => inst_repair_point_mut_on,
        "Inst_RepairPointMutOff" => inst_repair_point_mut_off,
        _ => return None,
    };
    Some(f)
}

/// Whether the handler of `cHardwareCPU::<method>` is ported.
pub fn handler_ported(method: &str) -> bool {
    handler(method).is_some()
}

/// `m_functions`: the handler of each library entry, by library index.
pub(crate) fn handler_table() -> &'static [Option<InstFn>] {
    static TABLE: OnceLock<Vec<Option<InstFn>>> = OnceLock::new();
    TABLE.get_or_init(|| CPU_INST_DEFS.iter().map(|d| handler(d.method)).collect())
}

/// The library methods that are registered but not ported, in library
/// order, without duplicates.
pub fn unported_methods() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for d in CPU_INST_DEFS.iter() {
        if handler(d.method).is_none() && !out.contains(&d.method) {
            out.push(d.method);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_count_matches_popcount() {
        for v in [0u32, 1, 0xFFFF_FFFF, 0x8000_0001, 0x1234_5678, 0x00FF_FF00] {
            assert_eq!(bit_count(v), v.count_ones());
        }
    }

    #[test]
    fn avida_mod_is_non_negative() {
        assert_eq!(avida_mod(-1, 26), 25);
        assert_eq!(avida_mod(27, 26), 1);
        assert_eq!(avida_mod(i32::MIN, 26), avida_mod(i32::MIN % 26, 26));
    }

    #[test]
    fn every_ported_method_exists_in_the_library() {
        let methods: Vec<&str> = CPU_INST_DEFS.iter().map(|d| d.method).collect();
        assert!(methods.contains(&"Inst_HeadCopy"));
        assert!(handler_ported("Inst_HeadCopy"));
        assert!(handler_ported("Inst_Collect"));
        assert_eq!(handler_table().len(), CPU_INST_DEFS.len());
        assert_eq!(unported_methods(), Vec::<&str>::new());
    }
}
