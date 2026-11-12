//! Kaboom / lysis / quorum and promoter handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb: `Inst_SenseQuorum`
//! through `Inst_Kazi5` minus the nop-pre/post pair already in the core
//! table, and the promoter model: `Inst_Promoter` through
//! `Inst_SenseRegulate`, `Do_Numberate`, `NextPromoter`,
//! `IsActivePromoter` and `Numberate`).
//!
//! The promoter list (`m_promoters`) is built by `internalReset()` under
//! `PROMOTERS_ENABLED` (see [`HardwareCpu::internal_reset`]); without it
//! the list stays empty, `terminate` never finds an active promoter and
//! the regulation instructions only consume their nops.

use crate::hardware_cpu::{HardwareCpu, NUM_REGISTERS, REG_AX, REG_BX};
use crate::hardware_cpu_insts::{arm_rem, InstFn, R};
use crate::nhardware::HEAD_IP;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_SenseQuorum" => |h, o, w| sense_quorum(h, o, w, false),
        "Inst_NoisyQuorum" => |h, o, w| sense_quorum(h, o, w, true),
        "Inst_SmartExplode" => |h, o, w| {
            let reg = h.find_modified_register(REG_AX);
            if h.reg(reg) != 0 {
                o.phenotype.kaboom_executed = true;
                let percent_prob = w.config().kaboom_prob;
                let distance = w.config().kaboom_hamming;
                if w.rng().p(percent_prob) {
                    w.kaboom(o, distance);
                }
            } else {
                w.stats_inc_dont_explode();
            }
            Ok(true)
        },
        "Inst_Lyse" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_AX);
            let mut percent_prob = w.config().kaboom_prob;
            if percent_prob == -1.0 {
                percent_prob = f64::from(h.reg(reg_used).wrapping_rem(100)) / 100.0;
            }
            if w.rng().p(percent_prob) {
                o.phenotype.kaboom_executed = true;
                let cpu_cycles = o.phenotype.cpu_cycles_used;
                w.stats_lyse(percent_prob, cpu_cycles);
            } else {
                w.stats_inc_dont_explode();
            }
            Ok(true)
        },
        "Inst_DisplayLyse" => |_, o, _| {
            o.lyse_display = true;
            Ok(true)
        },
        "Inst_CheckLyse" => |h, o, w| {
            let cells = cells_around(w, 1, "check-lyse")?;
            let mut lyse_counter = 0;
            for c in cells {
                if cell_org(o, w, c, |s| s.lyse_display) == Some(true) {
                    lyse_counter += 1;
                }
            }
            let reg = h.find_modified_register(REG_AX);
            h.set_reg(reg, lyse_counter);
            Ok(true)
        },
        "Inst_SenseAI" => |_, o, w| {
            let cells = cells_around(w, 1, "sense-ai")?;
            let mut ai_counter = 0;
            for c in cells {
                if cell_org(o, w, c, |s| s.lyse_display) == Some(true) {
                    ai_counter += 1;
                }
            }
            if ai_counter >= 3 {
                o.phenotype.kaboom_executed = true;
                return Ok(true);
            }
            Ok(false)
        },
        "Inst_Aggressive_SA" => |h, o, w| {
            o.phenotype.kaboom_executed = true;
            h.org_do_output(o, w, Some(0), false)?;
            let kaboom_prob = w.config().kaboom_prob;
            if w.rng().p(kaboom_prob) {
                let distance = w.config().kaboom_hamming;
                let effect = 1.0 / f64::from(w.config().kaboom_effect);
                w.kaboom_effect(o, distance, effect);
            }
            Ok(true)
        },
        "Inst_Cooperative_SA" => |h, o, w| {
            o.phenotype.kaboom_executed = true;
            h.org_do_output(o, w, Some(0), false)?;
            let kaboom_prob = w.config().kaboom_prob;
            if w.rng().p(kaboom_prob) {
                let distance = w.config().kaboom_hamming;
                let effect = f64::from(w.config().kaboom_effect);
                w.kaboom_effect(o, distance, effect);
            }
            Ok(true)
        },
        "Inst_Kazi" => |h, o, w| {
            let hd = w.config().kaboom_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Kazi1" => |h, o, w| {
            let hd = w.config().kaboom1_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Kazi2" => |h, o, w| {
            let hd = w.config().kaboom2_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Kazi3" => |h, o, w| {
            let hd = w.config().kaboom3_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Kazi4" => |h, o, w| {
            let hd = w.config().kaboom4_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Kazi5" => |h, o, w| {
            let hd = w.config().kaboom5_hamming;
            kazi(h, o, w, hd)
        },
        "Inst_Promoter" => |_, _, _| Ok(true),
        "Inst_Terminate" => |h, o, w| {
            h.promoter_terminate(o, w)?;
            Ok(true)
        },
        "Inst_Regulate" => |h, _, _| {
            let reg_used = h.find_modified_register(REG_BX);
            let regulation_code = h.reg(reg_used);
            for p in h.promoters.iter_mut() {
                p.regulation = regulation_code;
            }
            Ok(true)
        },
        "Inst_RegulateSpecificPromoters" => |h, _, w| {
            let reg_used = h.find_modified_register(REG_BX);
            let regulation_code = h.reg(reg_used);
            let reg_promoter = h.find_modified_register((reg_used + 1) % NUM_REGISTERS);
            let regulation_promoter = h.reg(reg_promoter);
            let exe_length = w.config().promoter_exe_length;
            for p in h.promoters.iter_mut() {
                // Consensus bit matches over the window length (arithmetic
                // shifts of signed ints, as in C++).
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
        },
        "Inst_SenseRegulate" => |h, o, w| {
            h.sense_regulate(o, w)?;
            Ok(true)
        },
        "Inst_Numberate" => |h, _, w| do_numberate(h, w, 0),
        "Inst_Numberate24" => |h, _, w| do_numberate(h, w, 24),
        _ => return None,
    };
    Some(f)
}

/// `Inst_Kazi` / `Inst_Kazi<N>` with `hamming` the matching
/// `KABOOM[N]_HAMMING`: explode (`Kaboom`) with `KABOOM_PROB`, register
/// ?AX? supplying whichever of probability and distance is -1 (a
/// `MAX_GENOME_SIZE` of 0 takes the register unreduced, as `x % 0` comes
/// out on the arm64 reference build).
fn kazi(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface, hamming: i32) -> R {
    let reg_used = hw.find_modified_register(REG_AX);
    org.phenotype.kaboom_executed = true;
    hw.org_do_output(org, world, Some(0), false)?;
    let mut percent_prob = 1.0;
    let mut distance = -1;
    let kaboom_prob = world.config().kaboom_prob;
    let prob_set = kaboom_prob as i32 != -1;
    if prob_set && hamming == -1 {
        let get_reg_value = hw.reg(reg_used);
        let genome_size = world.config().max_genome_size;
        percent_prob = kaboom_prob;
        distance = arm_rem(get_reg_value, genome_size);
    } else if prob_set && hamming != -1 {
        percent_prob = kaboom_prob;
        distance = hamming;
    } else if !prob_set && hamming != -1 {
        percent_prob = f64::from(hw.reg(reg_used).wrapping_rem(100)) / 100.0;
        distance = hamming;
    }
    if world.rng().p(percent_prob) {
        world.kaboom(org, distance);
    }
    Ok(true)
}

/// The cells within `radius` of the organism's cell (row by row of x,
/// then y), wrapped once around the `WORLD_X` x `WORLD_Y` torus as the C++
/// does. The test CPU (cell -1) and radii past one wrap read outside the
/// population in C++ and are the `Err`.
fn cells_around(world: &dyn OrgInterface, radius: i32, inst: &str) -> Result<Vec<i32>, String> {
    let cell_id = world.get_cell_id();
    let world_x = world.config().world_x;
    let world_y = world.config().world_y;
    let err = || {
        format!("unsupported: {inst} outside the population grid (C++: cHardwareCPU.cc:cHardwareCPU::Inst_{inst} reads GetCell out of range)")
    };
    if cell_id < 0 || world_x <= 0 || world_y <= 0 {
        return Err(err());
    }
    let cell_x = cell_id % world_x;
    let cell_y = (cell_id - cell_x) / world_x;
    let mut out = Vec::new();
    for i in (cell_x - radius)..=(cell_x + radius) {
        for j in (cell_y - radius)..=(cell_y + radius) {
            let x = if i < 0 {
                world_x + i
            } else if i >= world_x {
                i - world_x
            } else {
                i
            };
            let y = if j < 0 {
                world_y + j
            } else if j >= world_y {
                j - world_y
            } else {
                j
            };
            if x < 0 || x >= world_x || y < 0 || y >= world_y {
                return Err(err());
            }
            out.push(y * world_x + x);
        }
    }
    Ok(out)
}

/// `f` of the organism in `cell` (`None` when the cell is empty); the
/// running organism answers for its own cell.
fn cell_org<T>(
    org: &OrgState,
    world: &mut dyn OrgInterface,
    cell: i32,
    f: impl Fn(&OrgState) -> T,
) -> Option<T> {
    if world.cell_organism_id(cell) < 0 {
        return None;
    }
    // By cell: the two offspring of a sexual birth share an id.
    if cell == world.get_cell_id() {
        return Some(f(org));
    }
    let mut out = None;
    world.with_organism_in_cell(cell, &mut |o| out = Some(f(&o.state)));
    out
}

/// `Inst_SenseQuorum` / `Inst_NoisyQuorum` (`noisy`): AX = whether the
/// share of near kin (genomes within `KABOOM_HAMMING` symbol mismatches)
/// in the `KABOOM_RADIUS` square, in percent (scaled by |N(1, sd)| when
/// noisy, in `float`), is at most ?BX? mod 100.
///
/// A neighbour genome longer than this one compares its extra symbols
/// against the end of the C++ string (undefined past the terminator);
/// they count as mismatches.
fn sense_quorum(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    noisy: bool,
) -> R {
    let ref_genome = org.initial_genome.representation().as_string().into_bytes();
    let radius = world.config().kaboom_radius;
    let distance = world.config().kaboom_hamming;
    let cells = cells_around(
        world,
        radius,
        if noisy { "NoisyQuorum" } else { "SenseQuorum" },
    )?;
    let mut kincounter = 0;
    for c in cells {
        let diff = cell_org(org, world, c, |s| {
            let g = s.initial_genome.representation().as_string().into_bytes();
            g.iter()
                .enumerate()
                .filter(|(i, b)| ref_genome.get(*i) != Some(*b))
                .count() as i32
        });
        if let Some(diff) = diff {
            if diff <= distance {
                kincounter += 1;
            }
        }
    }
    let side = 2 * radius + 1;
    let ratio = kincounter as f32 / (side * side - 1) as f32;
    let reg = hw.find_modified_register(REG_BX);
    let org_ratio = hw.reg(reg).wrapping_rem(100);
    let scaled = if noisy {
        let sd = world.config().noisy_qs_sd as f32;
        let noise = world.rng().get_rand_normal_params(1.0, f64::from(sd)).abs() as f32;
        ratio * 100.0 * noise
    } else {
        ratio * 100.0
    };
    world.stats_inc_quorum(org_ratio);
    let out = hw.find_modified_register(REG_AX);
    hw.set_reg(out, i32::from(scaled as i32 <= org_ratio));
    Ok(true)
}

impl HardwareCpu {
    /// `Inst_Terminate(ctx)` (also run by `SingleProcess` before the first
    /// cycle and when processivity or `PROMOTER_INST_MAX` ends a run):
    /// optionally reset the thread and the inputs, then move the IP just
    /// past the next active promoter (BX = its bit code under
    /// `PROMOTER_TO_REGISTER`). Without one, `NO_ACTIVE_PROMOTER_EFFECT` 0
    /// and 2 restart at the top of the genome with BX = 0, 1 kills the
    /// organism, and other values print a warning to stdout and change
    /// nothing else, as the C++ does.
    ///
    /// A negative `PROMOTER_EXE_LENGTH` makes the C++ search run the
    /// offset down through signed overflow before it stops; that is the
    /// `Err` here.
    pub(crate) fn promoter_terminate(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        let ct = self.ct();
        if world.config().termination_resets != 0 {
            let id = self.threads[ct].id();
            self.threads[ct].reset(id);
            world.reset_inputs();
            org.input_buf.clear();
        }
        self.threads[ct].promoter_inst_executed = 0;
        self.advance_ip = false;
        let reg_used = REG_BX;

        let start_offset = self.promoter_offset;
        let mut start_index = self.promoter_index;
        let mut no_promoter_found = true;
        if !self.promoters.is_empty() {
            if world.config().promoter_exe_length < 0 {
                return Err(format!(
                    "unsupported: PROMOTER_EXE_LENGTH={} (C++: cHardwareCPU.cc:cHardwareCPU::Inst_Terminate searches through signed overflow)",
                    world.config().promoter_exe_length
                ));
            }
            loop {
                self.next_promoter(world);
                if self.is_active_promoter(world) {
                    no_promoter_found = false;
                    break;
                }
                // Back at the promoter we started on: none is active.
                if start_offset == self.promoter_offset && start_index == self.promoter_index {
                    break;
                }
                // Not started on a promoter: stop after the first one at
                // offset zero.
                if start_index == -1 {
                    start_index = 0;
                }
            }
        }

        if no_promoter_found {
            match world.config().no_active_promoter_effect {
                0 | 2 => {
                    self.promoter_index = -1;
                    self.head_set_pos(HEAD_IP, 0);
                    self.set_reg(reg_used, 0);
                }
                1 => org.die(world),
                other => world.cout(&format!(
                    "Unrecognized NO_ACTIVE_PROMOTER_EFFECT setting: {other}\n"
                )),
            }
        } else {
            // cHeadCPU::Set takes the position mod the genome size.
            let p = self.promoters[self.promoter_index as usize];
            self.head_set_pos(HEAD_IP, p.pos + 1);
            if world.config().promoter_to_register != 0 {
                self.set_reg(reg_used, p.bit_code);
            }
        }
        Ok(())
    }

    /// `NextPromoter()`: the next promoter, moving the window offset on
    /// (back to 0 when the window would wrap) after the last one.
    fn next_promoter(&mut self, world: &dyn OrgInterface) {
        let exe_length = world.config().promoter_exe_length;
        let code_size = world.config().promoter_code_size;
        self.promoter_index += 1;
        if self.promoter_index == self.promoters.len() as i32 {
            self.promoter_index = 0;
            self.promoter_offset = self.promoter_offset.wrapping_add(exe_length);
            if self.promoter_offset.wrapping_add(exe_length) > code_size {
                self.promoter_offset = 0;
            }
        }
    }

    /// `IsActivePromoter()`: at least `PROMOTER_EXE_THRESHOLD` bits set in
    /// the current window of the regulated bit code. A
    /// `PROMOTER_CODE_SIZE` of 0 leaves the offset unreduced (`x % 0` on
    /// the arm64 reference build) and shifts take their amount mod 32.
    fn is_active_promoter(&self, world: &dyn OrgInterface) -> bool {
        let cfg = world.config();
        let code = self.promoters[self.promoter_index as usize].regulated_bit_code() as u32;
        let mut count = 0;
        for i in 0..cfg.promoter_exe_length {
            let offset = arm_rem(self.promoter_offset.wrapping_add(i), cfg.promoter_code_size);
            let state = code.wrapping_shr(offset as u32) as i32;
            count += state & 1;
        }
        count >= cfg.promoter_exe_threshold
    }

    /// `Inst_SenseRegulate(ctx)` (also the constitutive regulation step of
    /// `SingleProcess`): the regulation of every promoter becomes one bit
    /// per `PROMOTER_CODE_SIZE` position, set where the resource
    /// `i % num_resources` is non-zero. Without resources the C++ takes
    /// `i % 0` and indexes an empty array; that is the `Err`.
    pub(crate) fn sense_regulate(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
    ) -> Result<(), String> {
        let res_count = world.get_resources(org);
        let code_size = world.config().promoter_code_size;
        if res_count.is_empty() && code_size > 0 {
            return Err("sense-regulate without resources (C++: cHardwareCPU.cc:cHardwareCPU::Inst_SenseRegulate takes i % 0)".to_string());
        }
        let mut bits: u32 = 0;
        for i in 0..code_size {
            let b = (i as usize) % res_count.len();
            bits <<= 1;
            bits = bits.wrapping_add(u32::from(res_count[b] != 0.0));
        }
        for p in self.promoters.iter_mut() {
            p.regulation = bits as i32;
        }
        Ok(())
    }
}

/// `Do_Numberate(ctx, num_bits)`: ?BX? = the bits encoded by the
/// instructions after this one (`Numberate(pos, +1, num_bits)`; 0 bits
/// means 32), and execution resumes right after the IP.
fn do_numberate(hw: &mut HardwareCpu, world: &mut dyn OrgInterface, num_bits: i32) -> R {
    let reg_used = hw.find_modified_register(REG_BX);
    hw.ip_advance();
    hw.advance_ip = false;
    let pos = hw.ip_head().position();
    let num = numberate(hw, world.config().inst_code_length, pos, 1, num_bits)?;
    hw.set_reg(reg_used, num);
    Ok(true)
}

/// `Numberate(pos, dir, num_bits)`: shift in `INST_CODE_LENGTH` bits of
/// each instruction code from `pos` on in direction `dir`. A code length
/// below 1 never fills the bits (the C++ loops forever) and is the `Err`.
pub(crate) fn numberate(
    hw: &HardwareCpu,
    code_len: i32,
    pos: i32,
    dir: i32,
    mut num_bits: i32,
) -> Result<i32, String> {
    if num_bits == 0 {
        num_bits = 32;
    }
    if code_len < 1 && num_bits > 0 {
        return Err("numberate with INST_CODE_LENGTH < 1 (C++: cHardwareCPU.cc:cHardwareCPU::Numberate never terminates)".to_string());
    }
    let size = hw.memory.size();
    let mut code_size = 0;
    let mut code: u32 = 0;
    let mut j = pos.wrapping_add(size) % size;
    while code_size < num_bits {
        let mut inst_code = hw.base.inst_set.instruction_code(hw.memory[j]) as u32;
        let mut code_on = 0;
        while code_size < num_bits && code_on < code_len {
            if dir < 0 {
                code >>= 1;
                code = code.wrapping_add(
                    1u32.wrapping_shl((num_bits - 1) as u32)
                        .wrapping_mul(inst_code & 1),
                );
                inst_code >>= 1;
            } else {
                code <<= 1;
                code = code.wrapping_add(inst_code.wrapping_shr((code_len - 1) as u32) & 1);
                inst_code <<= 1;
            }
            code_size += 1;
            code_on += 1;
        }
        j = (j + size + dir) % size;
    }
    Ok(code as i32)
}
