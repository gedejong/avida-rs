//! Port of `cInstSet` (avida-core/source/cpu/cInstSet.{h,cc}): the mapping
//! from genome opcodes to instruction-library entries, with per-instruction
//! costs, the redundancy-weighted mutation index and the loader for
//! `INST name:arg=value:...` lines.

use crate::arg_schema::{ArgContainer, ArgSchema};
use crate::cstr::{edit_distance, pop, pop_word};
use crate::feedback::Feedback;
use crate::inst_lib::InstLib;
use crate::instruction::Instruction;
use crate::sequence::InstructionSequence;
use crate::weighted_index::OrderedWeightedIndex;
use avida_apto::AvidaRng;
use std::sync::Arc;

/// `MAX_INSTSET_SIZE`
pub const MAX_INSTSET_SIZE: i32 = 255;

/// `INST_CODE_ZEROS` (avida/core/Definitions.h `eINST_CODE_DEFAULT`).
pub const INST_CODE_ZEROS: i32 = 0;
/// `INST_CODE_INSTNUM`
pub const INST_CODE_INSTNUM: i32 = 1;

/// The two `cAvidaConfig` settings `cInstSet::LoadWithStringList` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstCodeConfig {
    /// `INST_CODE_LENGTH` (default 3)
    pub inst_code_length: i32,
    /// `INST_CODE_DEFAULT_TYPE` (default 0)
    pub inst_code_default_type: i32,
}

impl Default for InstCodeConfig {
    fn default() -> Self {
        InstCodeConfig {
            inst_code_length: 3,
            inst_code_default_type: INST_CODE_ZEROS,
        }
    }
}

/// `cInstSet::sInstEntry`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InstEntry {
    /// `lib_fun_id`
    pub lib_fun_id: i32,
    /// `redundancy` (an `int`; the loaded double is truncated)
    pub redundancy: i32,
    /// `cost`
    pub cost: i32,
    /// `ft_cost` (`initial_cost`)
    pub ft_cost: i32,
    /// `energy_cost`
    pub energy_cost: i32,
    /// `prob_fail`
    pub prob_fail: f64,
    /// `addl_time_cost`
    pub addl_time_cost: i32,
    /// `inst_code`
    pub inst_code: i32,
    /// `res_cost`
    pub res_cost: f64,
    /// `fem_res_cost`
    pub fem_res_cost: f64,
    /// `female_cost`
    pub female_cost: i32,
    /// `choosy_female_cost`
    pub choosy_female_cost: i32,
    /// `post_cost`
    pub post_cost: i32,
    /// `bonus_cost`
    pub bonus_cost: f64,
}

/// `cInstSet`.
///
/// Not `Clone`: the C++ copy constructor and assignment operator drop the
/// nop map and leave the stack size / uops uninitialised; only
/// `ANALYZE_REDUNDANCY_BY_INST_FAILURE` uses them, through
/// [`InstSet::cpp_copy`] and [`InstSet::cpp_assigned`].
#[derive(Debug)]
pub struct InstSet {
    name: String,
    hw_type: i32,
    inst_lib: Arc<InstLib>,
    lib_name_map: Vec<InstEntry>,
    lib_nopmod_map: Vec<i32>,
    mutation_index: OrderedWeightedIndex,
    has_costs: bool,
    has_ft_costs: bool,
    has_energy_costs: bool,
    has_res_costs: bool,
    has_fem_res_costs: bool,
    has_female_costs: bool,
    has_choosy_female_costs: bool,
    has_post_costs: bool,
    has_bonus_costs: bool,
    stack_size: i32,
    uops_per_cycle: i32,
}

impl InstSet {
    /// `cInstSet(world, name, hw_type, inst_lib, stack_size, uops_per_cycle)`.
    pub fn new(
        name: &str,
        hw_type: i32,
        inst_lib: Arc<InstLib>,
        stack_size: i32,
        uops_per_cycle: i32,
    ) -> Self {
        InstSet {
            name: name.to_string(),
            hw_type,
            inst_lib,
            lib_name_map: Vec::new(),
            lib_nopmod_map: Vec::new(),
            mutation_index: OrderedWeightedIndex::new(),
            has_costs: false,
            has_ft_costs: false,
            has_energy_costs: false,
            has_res_costs: false,
            has_fem_res_costs: false,
            has_female_costs: false,
            has_choosy_female_costs: false,
            has_post_costs: false,
            has_bonus_costs: false,
            stack_size,
            uops_per_cycle,
        }
    }

    /// `cInstSet(const cInstSet&)`, the copy constructor. QUIRK: it copies
    /// the name, hardware type, library, entries (costs, failure
    /// probabilities, ...), mutation index and cost flags, but not the nop
    /// map, so the copy has no nops (`IsNop` is false for every opcode and
    /// `GetNumNops` is 0); and it leaves `m_stack_size` and
    /// `m_uops_per_cycle` uninitialised. Those two are copied here; callers
    /// must keep the copy away from hardware that reads them (the
    /// experimental, GP8 and BCR hardware).
    pub fn cpp_copy(&self) -> InstSet {
        InstSet {
            lib_nopmod_map: Vec::new(),
            stack_size: self.stack_size,
            uops_per_cycle: self.uops_per_cycle,
            ..self.copied_members()
        }
    }

    /// `*this = source` (`cInstSet::operator=`): the members the copy
    /// constructor copies come from `source`; the nop map, stack size and
    /// uops stay those of `self`.
    pub fn cpp_assigned(&self, source: &InstSet) -> InstSet {
        InstSet {
            lib_nopmod_map: self.lib_nopmod_map.clone(),
            stack_size: self.stack_size,
            uops_per_cycle: self.uops_per_cycle,
            ..source.copied_members()
        }
    }

    /// The members the copy constructor and `operator=` copy (the rest
    /// left for the caller to fill).
    fn copied_members(&self) -> InstSet {
        InstSet {
            name: self.name.clone(),
            hw_type: self.hw_type,
            inst_lib: Arc::clone(&self.inst_lib),
            lib_name_map: self.lib_name_map.clone(),
            lib_nopmod_map: Vec::new(),
            mutation_index: self.mutation_index.clone(),
            has_costs: self.has_costs,
            has_ft_costs: self.has_ft_costs,
            has_energy_costs: self.has_energy_costs,
            has_res_costs: self.has_res_costs,
            has_fem_res_costs: self.has_fem_res_costs,
            has_female_costs: self.has_female_costs,
            has_choosy_female_costs: self.has_choosy_female_costs,
            has_post_costs: self.has_post_costs,
            has_bonus_costs: self.has_bonus_costs,
            stack_size: 0,
            uops_per_cycle: 0,
        }
    }

    /// `GetInstSetName()`.
    pub fn inst_set_name(&self) -> &str {
        &self.name
    }

    /// `GetHardwareType()`.
    pub fn hardware_type(&self) -> i32 {
        self.hw_type
    }

    /// `GetInstLib()`.
    pub fn inst_lib(&self) -> &Arc<InstLib> {
        &self.inst_lib
    }

    /// The entry of opcode `id` (`m_lib_name_map[id]`).
    pub fn entry(&self, id: i32) -> &InstEntry {
        &self.lib_name_map[id as usize]
    }

    fn e(&self, inst: Instruction) -> &InstEntry {
        &self.lib_name_map[inst.op() as usize]
    }

    /// `GetName(int id)`.
    pub fn name(&self, id: i32) -> &str {
        self.inst_lib
            .name(self.lib_name_map[id as usize].lib_fun_id)
    }

    /// `GetName(const Instruction&)`.
    pub fn name_of(&self, inst: Instruction) -> &str {
        self.name(inst.op())
    }

    /// `GetRedundancy(inst)`
    pub fn redundancy(&self, inst: Instruction) -> i32 {
        self.e(inst).redundancy
    }
    /// `GetCost(inst)`
    pub fn cost(&self, inst: Instruction) -> i32 {
        self.e(inst).cost
    }
    /// `GetFTCost(inst)`
    pub fn ft_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).ft_cost
    }
    /// `GetEnergyCost(inst)`
    pub fn energy_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).energy_cost
    }
    /// `GetProbFail(inst)`
    pub fn prob_fail(&self, inst: Instruction) -> f64 {
        self.e(inst).prob_fail
    }
    /// `GetAddlTimeCost(inst)`
    pub fn addl_time_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).addl_time_cost
    }
    /// `GetInstructionCode(inst)`
    pub fn instruction_code(&self, inst: Instruction) -> i32 {
        self.e(inst).inst_code
    }
    /// `GetResCost(inst)`
    pub fn res_cost(&self, inst: Instruction) -> f64 {
        self.e(inst).res_cost
    }
    /// `GetFemResCost(inst)`
    pub fn fem_res_cost(&self, inst: Instruction) -> f64 {
        self.e(inst).fem_res_cost
    }
    /// `GetFemaleCost(inst)`
    pub fn female_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).female_cost
    }
    /// `GetChoosyFemaleCost(inst)`
    pub fn choosy_female_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).choosy_female_cost
    }
    /// `GetPostCost(inst)`
    pub fn post_cost(&self, inst: Instruction) -> i32 {
        self.e(inst).post_cost
    }
    /// `GetBonusCost(inst)`
    pub fn bonus_cost(&self, inst: Instruction) -> f64 {
        self.e(inst).bonus_cost
    }

    /// `GetLibFunctionIndex(inst)`: index into the hardware's function table.
    pub fn lib_function_index(&self, inst: Instruction) -> i32 {
        self.e(inst).lib_fun_id
    }

    /// `GetNopMod(inst)`: register modifier of a nop. The opcode must be a
    /// nop (`is_nop`), as in C++.
    pub fn nop_mod(&self, inst: Instruction) -> i32 {
        let nopmod = self.lib_nopmod_map[inst.op() as usize];
        self.inst_lib.nop_mod(nopmod as u32)
    }

    /// `GetRandomInst(ctx)`: one `GetDouble(total_weight)` draw, then the
    /// redundancy-weighted lookup. QUIRK: a failed lookup (-1) wraps to
    /// opcode 255.
    pub fn random_inst(&self, rng: &mut AvidaRng) -> Instruction {
        let weight = rng.get_double_max(self.mutation_index.total_weight());
        let inst_ndx = self.mutation_index.find_position(weight);
        Instruction::new(inst_ndx)
    }

    /// `GetRandFunctionIndex(ctx)`.
    pub fn rand_function_index(&self, rng: &mut AvidaRng) -> i32 {
        let inst = self.random_inst(rng);
        self.e(inst).lib_fun_id
    }

    /// `GetSize()`.
    pub fn size(&self) -> i32 {
        self.lib_name_map.len() as i32
    }

    /// `GetNumNops()`.
    pub fn num_nops(&self) -> i32 {
        self.lib_nopmod_map.len() as i32
    }

    /// `HasCosts()` (some `cost > 1`)
    pub fn has_costs(&self) -> bool {
        self.has_costs
    }
    /// `HasFTCosts()`
    pub fn has_ft_costs(&self) -> bool {
        self.has_ft_costs
    }
    /// `HasEnergyCosts()`
    pub fn has_energy_costs(&self) -> bool {
        self.has_energy_costs
    }
    /// `HasResCosts()`
    pub fn has_res_costs(&self) -> bool {
        self.has_res_costs
    }
    /// `HasFemResCosts()`
    pub fn has_fem_res_costs(&self) -> bool {
        self.has_fem_res_costs
    }
    /// `HasFemaleCosts()`
    pub fn has_female_costs(&self) -> bool {
        self.has_female_costs
    }
    /// `HasChoosyFemaleCosts()`
    pub fn has_choosy_female_costs(&self) -> bool {
        self.has_choosy_female_costs
    }
    /// `HasPostCosts()` (some `post_cost > 1`)
    pub fn has_post_costs(&self) -> bool {
        self.has_post_costs
    }
    /// `HasBonusCosts()`
    pub fn has_bonus_costs(&self) -> bool {
        self.has_bonus_costs
    }

    /// `GetStackSize()`.
    pub fn stack_size(&self) -> i32 {
        self.stack_size
    }

    /// `GetUOpsPerCycle()`.
    pub fn uops_per_cycle(&self) -> i32 {
        self.uops_per_cycle
    }

    /// `IsNop(inst)`: opcode below the number of nops.
    pub fn is_nop(&self, inst: Instruction) -> bool {
        inst.op() < self.lib_nopmod_map.len() as i32
    }

    fn lib_entry(&self, inst: Instruction) -> &crate::inst_lib::InstLibEntry {
        self.inst_lib.get(self.lib_function_index(inst))
    }

    /// `IsLabel(inst)`
    pub fn is_label(&self, inst: Instruction) -> bool {
        self.lib_entry(inst).is_label()
    }
    /// `IsPromoter(inst)`
    pub fn is_promoter(&self, inst: Instruction) -> bool {
        self.lib_entry(inst).is_promoter()
    }
    /// `IsTerminator(inst)`
    pub fn is_terminator(&self, inst: Instruction) -> bool {
        self.lib_entry(inst).is_terminator()
    }
    /// `ShouldStall(inst)`
    pub fn should_stall(&self, inst: Instruction) -> bool {
        self.lib_entry(inst).should_stall()
    }
    /// `ShouldSleep(inst)`
    pub fn should_sleep(&self, inst: Instruction) -> bool {
        self.lib_entry(inst).should_sleep()
    }
    /// `IsImmediateValue(inst)` (false for the error instruction).
    pub fn is_immediate_value(&self, inst: Instruction) -> bool {
        inst != self.inst_error() && self.lib_entry(inst).is_immediate_value()
    }
    /// `GetFlags(inst)`
    pub fn flags(&self, inst: Instruction) -> u32 {
        self.lib_entry(inst).flags()
    }

    /// `ActivateNullInst()`: append the library's NULL instruction (once)
    /// with zero redundancy and costs; it is not added to the mutation index.
    pub fn activate_null_inst(&mut self) -> Instruction {
        let inst_id = self.lib_name_map.len() as i32;
        let null_fun_id = self.inst_lib.inst_null();

        for i in 0..inst_id {
            if self.lib_name_map[i as usize].lib_fun_id == null_fun_id {
                return Instruction::new(i);
            }
        }

        self.lib_name_map.push(InstEntry {
            lib_fun_id: null_fun_id,
            ..InstEntry::default()
        });
        Instruction::new(inst_id)
    }

    /// `SetProbFail(inst, p)`.
    pub fn set_prob_fail(&mut self, inst: Instruction, prob_fail: f64) {
        self.lib_name_map[inst.op() as usize].prob_fail = prob_fail;
    }

    /// `SetRedundancy(inst, r)`. QUIRK: adds a further mutation-index item
    /// for the opcode instead of replacing its weight.
    pub fn set_redundancy(&mut self, inst: Instruction, redundancy: i32) {
        self.lib_name_map[inst.op() as usize].redundancy = redundancy;
        self.mutation_index.set_weight(inst.op(), redundancy as f64);
    }

    /// The redundancy-weighted index used by `random_inst`.
    pub fn mutation_index(&self) -> &OrderedWeightedIndex {
        &self.mutation_index
    }

    /// `GetInst(name)`: the opcode whose library name is `name`.
    ///
    /// The C++ prints `Error: Unknown instruction '<name>'.  Exiting...` to
    /// stderr and calls `exit(1)` for an unknown name; that line is the
    /// `Err` value, and the caller must print it and exit with status 1.
    pub fn get_inst(&self, in_name: &str) -> Result<Instruction, String> {
        for (i, e) in self.lib_name_map.iter().enumerate() {
            if self.inst_lib.name(e.lib_fun_id) == in_name {
                return Ok(Instruction::new(i as i32));
            }
        }
        Err(format!(
            "Error: Unknown instruction '{}'.  Exiting...",
            in_name
        ))
    }

    /// `FindBestMatch(name)`: closest instruction name by
    /// `cStringUtil::EditDistance` (first best, stops at an exact match).
    pub fn find_best_match(&self, in_name: &str) -> String {
        let mut best_dist = 1024;
        let mut best_name = String::new();
        for e in &self.lib_name_map {
            let cur_name = self.inst_lib.name(e.lib_fun_id);
            let cur_dist = edit_distance(cur_name, in_name);
            if cur_dist < best_dist {
                best_dist = cur_dist;
                best_name = cur_name.to_string();
            }
            if cur_dist == 0 {
                break;
            }
        }
        best_name
    }

    /// `InstInSet(name)`.
    pub fn inst_in_set(&self, in_name: &str) -> bool {
        self.lib_name_map
            .iter()
            .any(|e| self.inst_lib.name(e.lib_fun_id) == in_name)
    }

    /// `GetInstDefault()`: QUIRK: the *library* default index wrapped as an
    /// opcode.
    pub fn inst_default(&self) -> Instruction {
        Instruction::new(self.inst_lib.inst_default())
    }

    /// `GetInstError()`: opcode 255.
    pub fn inst_error(&self) -> Instruction {
        Instruction::ERROR
    }

    /// `LoadWithStringList(sl, feedback)`: process every `INST` line (other
    /// lines are ignored), then build the mutation index from the integer
    /// redundancies (zero-redundancy opcodes are left out).
    pub fn load_with_string_list(
        &mut self,
        sl: &[String],
        cfg: InstCodeConfig,
        feedback: &mut Feedback,
    ) -> bool {
        let mut schema = ArgSchema::with_entry_separator(':');

        // Integer
        schema.add_int("cost", 0, 0);
        schema.add_int("initial_cost", 1, 0);
        schema.add_int("energy_cost", 2, 0);
        schema.add_int("addl_time_cost", 3, 0);
        schema.add_int("female_cost", 4, 0);
        schema.add_int("choosy_female_cost", 5, 0);
        schema.add_int("post_cost", 6, 0);

        // Double
        schema.add_double("prob_fail", 0, 0.0);
        schema.add_double("res_cost", 1, 0.0);
        schema.add_double("redundancy", 2, 1.0);
        schema.add_double("fem_res_cost", 3, 0.0);
        schema.add_double("bonus_cost", 4, 0.0);

        // String
        schema.add_string("inst_code", 0, "");

        // Ensure that the instruction code length is in the range of bits
        // supported by the int type
        let mut inst_code_len = cfg.inst_code_length;
        if (inst_code_len as u32) > 32 {
            inst_code_len = 32;
        } else if inst_code_len <= 0 {
            inst_code_len = 1;
        }

        let mut success = true;
        for line in sl {
            let mut cur_line = line.clone();

            // Look for the INST keyword at the beginning of each line.
            let inst_name = pop_word(&mut cur_line);
            if inst_name != "INST" {
                continue;
            }

            // Lookup the instruction name in the library
            let inst_name = pop(&mut cur_line, ':');
            let fun_id = self.inst_lib.get_index(&inst_name);
            if fun_id == -1 {
                feedback.error(format!(
                    "unknown instruction '{}' (Best match = '{}')",
                    inst_name,
                    self.inst_lib.near_match(&inst_name)
                ));
                success = false;
                continue;
            }

            // Load the arguments for this instruction
            let Some(args) = ArgContainer::load(&cur_line, &schema, feedback) else {
                success = false;
                continue;
            };

            // Check to make sure we are not inserting the special NULL instruction
            if fun_id == self.inst_lib.inst_null() {
                feedback.error("invalid use of NULL instruction");
                success = false;
                continue;
            }

            let redundancy = args.get_double(2);
            if redundancy < 0.0 {
                feedback.warning(format!(
                    "instruction '{}' has negative redundancy, ignoring...",
                    inst_name
                ));
                continue;
            }

            let inst_id = self.lib_name_map.len() as i32;

            let mut entry = InstEntry {
                lib_fun_id: fun_id,
                // double -> int conversion truncates toward zero
                redundancy: redundancy as i32,
                cost: args.get_int(0),
                ft_cost: args.get_int(1),
                energy_cost: args.get_int(2),
                prob_fail: args.get_double(0),
                addl_time_cost: args.get_int(3),
                inst_code: 0,
                res_cost: args.get_double(1),
                fem_res_cost: args.get_double(3),
                female_cost: args.get_int(4),
                choosy_female_cost: args.get_int(5),
                post_cost: args.get_int(6),
                bonus_cost: args.get_double(4),
            };

            if entry.cost > 1 {
                self.has_costs = true;
            }
            if entry.ft_cost != 0 {
                self.has_ft_costs = true;
            }
            if entry.energy_cost != 0 {
                self.has_energy_costs = true;
            }
            if entry.res_cost != 0.0 {
                self.has_res_costs = true;
            }
            if entry.fem_res_cost != 0.0 {
                self.has_fem_res_costs = true;
            }
            if entry.female_cost != 0 {
                self.has_female_costs = true;
            }
            if entry.choosy_female_cost != 0 {
                self.has_choosy_female_costs = true;
            }
            if entry.post_cost > 1 {
                self.has_post_costs = true;
            }
            if entry.bonus_cost != 0.0 {
                self.has_bonus_costs = true;
            }

            // Parse the instruction code
            let inst_code = args.get_string(0);
            if inst_code.is_empty() {
                match cfg.inst_code_default_type {
                    INST_CODE_ZEROS => entry.inst_code = 0,
                    // QUIRK: `((~0) >> (32 - len)) & inst_id` shifts a
                    // negative int arithmetically, so the mask is all ones
                    // and the code is the plain instruction id.
                    INST_CODE_INSTNUM => {
                        entry.inst_code = (!0i32 >> (32 - inst_code_len)) & inst_id
                    }
                    _ => {
                        feedback.error("invalid default instruction code type");
                        success = false;
                    }
                }
            } else {
                let mut inst_code_val: i32 = 0;
                for &c in inst_code.as_bytes().iter().take(inst_code_len as usize) {
                    inst_code_val = inst_code_val.wrapping_shl(1);
                    if c == b'1' {
                        inst_code_val |= 1;
                    } else if c != b'0' {
                        feedback.error("invalid character in instruction code, must be 0 or 1");
                        success = false;
                        break;
                    }
                }
                entry.inst_code = inst_code_val;
            }

            self.lib_name_map.push(entry);

            // If this is a nop, add it to the proper mappings
            if self.inst_lib.get(fun_id).is_nop() {
                // Assert nops are at the _beginning_ of an inst_set.
                if self.lib_name_map.len() != self.lib_nopmod_map.len() + 1 {
                    feedback.error("invalid NOP placement, all NOPs must be listed first");
                    success = false;
                }
                self.lib_nopmod_map.resize(inst_id as usize + 1, 0);
                self.lib_nopmod_map[inst_id as usize] = fun_id;
            }
        }

        // Setup mutation indexing based on redundancies
        self.mutation_index = OrderedWeightedIndex::new();
        for (id, e) in self.lib_name_map.iter().enumerate() {
            let red = e.redundancy as f64;
            if red == 0.0 {
                continue;
            }
            self.mutation_index.set_weight(id as i32, red);
        }
        success
    }

    /// `SaveInstructionSequence(of, seq)`: one instruction name per line.
    pub fn save_instruction_sequence(&self, seq: &InstructionSequence) -> String {
        let mut out = String::new();
        for inst in seq.iter() {
            out.push_str(self.name_of(*inst));
            out.push('\n');
        }
        out
    }
}
