//! Group, tolerance, network and deme handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb: `Inst_JoinGroup`
//! through `Inst_GetGroupTolerance`, `Inst_CreateLinkByFacing` through
//! `Inst_NetworkSelect`, `Inst_DonateResToDeme`,
//! `Inst_ApplyPointMutationsGroup*`, `Inst_ReproDeme`, `Inst_SpawnDeme`,
//! `Inst_Suicide` and `Inst_RelinquishEnergyTo*`).
//!
//! A NULL `GetDeme()` (the test CPU) is a negative deme id here.

use crate::hardware_cpu::{HardwareCpu, REG_AX, REG_BX, REG_CX};
use crate::hardware_cpu_insts::{InstFn, R};
use crate::hardware_cpu_insts_msg::new_message;
use crate::nhardware::{MATING_TYPE_FEMALE, MATING_TYPE_JUVENILE, MATING_TYPE_MALE};
use crate::org_interface::{with_neighbor_state, OrgInterface, TestamentTarget};
use crate::organism::OrgState;

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_JoinGroup" => inst_join_group,
        "Inst_JoinMTGroup" => |h, o, w| {
            if o.phenotype.mating_type == MATING_TYPE_JUVENILE {
                return Ok(false);
            }
            inst_join_group(h, o, w)
        },
        "Inst_JoinNextGroup" => inst_join_next_group,
        "Inst_NumberOrgsInMyGroup" => |h, o, w| {
            let mut num_orgs = 0;
            if w.has_opinion(o) {
                let opinion = o.get_opinion().0;
                num_orgs = w.number_of_organisms_in_group(opinion);
            }
            let num_org_reg = h.find_modified_register(REG_BX);
            h.set_reg(num_org_reg, num_orgs);
            Ok(true)
        },
        "Inst_NumberMTInMyGroup" => |h, o, w| {
            if o.phenotype.mating_type == MATING_TYPE_JUVENILE {
                return Ok(false);
            }
            let (mut num_fem, mut num_male, mut num_juv) = (0, 0, 0);
            if w.has_opinion(o) {
                let opinion = o.get_opinion().0;
                num_fem = w.number_group_females(opinion);
                num_male = w.number_group_males(opinion);
                num_juv = w.number_group_juvs(opinion);
            }
            set_three(h, num_fem, num_male, num_juv);
            Ok(true)
        },
        // QUIRK: the group id is the index of the nop-selected register,
        // not its contents (also in Inst_NumberMTInGroup).
        "Inst_NumberOrgsInGroup" => |h, _, w| {
            let group_id = h.find_modified_register(REG_BX) as i32;
            let num_org_reg = h.find_modified_register(REG_CX);
            let num_orgs = w.number_of_organisms_in_group(group_id);
            h.set_reg(num_org_reg, num_orgs);
            Ok(true)
        },
        "Inst_NumberMTInGroup" => |h, o, w| {
            if o.phenotype.mating_type == MATING_TYPE_JUVENILE {
                return Ok(false);
            }
            let group_id = h.find_modified_register(REG_BX) as i32;
            let num_fem = w.number_group_females(group_id);
            let num_male = w.number_group_males(group_id);
            let num_juv = w.number_group_juvs(group_id);
            set_three(h, num_fem, num_male, num_juv);
            Ok(true)
        },
        "Inst_NumberNextGroup" => |h, o, w| number_next_group(h, o, w, false),
        "Inst_NumberMTNextGroup" => |h, o, w| {
            if o.phenotype.mating_type == MATING_TYPE_JUVENILE {
                return Ok(false);
            }
            number_next_group(h, o, w, true)
        },
        "Inst_KillGroupMember" => |_, o, w| {
            if w.has_opinion(o) {
                let opinion = o.get_opinion().0;
                w.kill_group_member(opinion, o);
            }
            Ok(true)
        },
        "Inst_IncTolerance" => |h, o, w| change_tolerance(h, o, w, true),
        "Inst_DecTolerance" => |h, o, w| change_tolerance(h, o, w, false),
        "Inst_GetTolerance" => inst_get_tolerance,
        "Inst_GetGroupTolerance" => inst_get_group_tolerance,
        "Inst_CreateLinkByFacing" => |h, _, w| {
            let wreg = h.find_modified_register(REG_BX);
            w.create_link_by_facing(f64::from(h.reg(wreg)));
            Ok(true)
        },
        "Inst_CreateLinkByXY" => |h, _, w| {
            let xreg = h.find_modified_register(REG_BX);
            let yreg = HardwareCpu::find_next_register(xreg);
            let wreg = HardwareCpu::find_next_register(yreg);
            w.create_link_by_xy(h.reg(xreg), h.reg(yreg), f64::from(h.reg(wreg)));
            Ok(true)
        },
        "Inst_CreateLinkByIndex" => |h, _, w| {
            let idxreg = h.find_modified_register(REG_BX);
            let wreg = HardwareCpu::find_next_register(idxreg);
            w.create_link_by_index(h.reg(idxreg), f64::from(h.reg(wreg)));
            Ok(true)
        },
        "Inst_NetworkBroadcast1" => |h, o, w| network_message(h, o, w, true),
        "Inst_NetworkUnicast" => |h, o, w| network_message(h, o, w, false),
        "Inst_NetworkRotate" => |h, _, w| {
            let reg = h.find_modified_register(REG_BX);
            Ok(w.network_rotate(h.reg(reg)))
        },
        "Inst_NetworkSelect" => |h, _, w| {
            let reg = h.find_modified_register(REG_BX);
            Ok(w.network_select(h.reg(reg)))
        },
        "Inst_DonateResToDeme" => |_, o, w| {
            // cOrganism::DonateResConsumedToDeme
            let deme_id = w.get_deme_id();
            if deme_id >= 0 {
                let consumed = o.phenotype.take_resources_consumed();
                w.deme_add_resources_consumed(deme_id, consumed);
            }
            Ok(true)
        },
        "Inst_ApplyPointMutationsGroupRandom" => |h, o, w| {
            let point_mut_prob = w.config().inst_point_mut_prob;
            // Check for test CPU
            let deme_id = w.get_deme_id();
            if deme_id < 0 {
                return Ok(false);
            }
            w.point_mutate_deme_member(o, h, deme_id, point_mut_prob, None);
            Ok(true)
        },
        "Inst_ApplyPointMutationsGroupGS" => |h, o, w| {
            let point_mut_prob = w.config().inst_point_mut_prob;
            let deme_id = w.get_deme_id();
            if deme_id < 0 {
                return Ok(false);
            }
            let gs = o.phenotype.is_germ_cell;
            w.point_mutate_deme_member(o, h, deme_id, point_mut_prob, Some(gs));
            Ok(true)
        },
        "Inst_ReproDeme" => |_, _, w| {
            let deme_id = w.get_deme_id();
            if deme_id < 0 {
                return Ok(false);
            }
            w.deme_replicate(deme_id);
            Ok(true)
        },
        "Inst_SpawnDeme" => |_, o, w| {
            w.spawn_deme(o);
            Ok(true)
        },
        "Inst_Suicide" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_AX);
            let percent_prob = f64::from(h.reg(reg_used).wrapping_rem(100)) / 100.0;
            let deme_id = w.get_deme_id();
            if deme_id < 0 {
                return Ok(false);
            }
            if w.rng().p(percent_prob) {
                let points = w.config().demes_protection_points;
                w.deme_record_suicide(deme_id, points);
                o.die(w);
            }
            Ok(true)
        },
        "Inst_RelinquishEnergyToFutureDeme" => |_, o, w| {
            let stored_energy = o.phenotype.energy_store * w.config().frac_energy_relinquish;
            let deme_id = w.get_deme_id();
            if deme_id < 0 {
                return Ok(false);
            }
            w.deme_increase_total_energy_testament(deme_id, stored_energy);
            w.stats_add_energy_testament(TestamentTarget::FutureDeme, stored_energy);
            o.die(w);
            Ok(true)
        },
        "Inst_RelinquishEnergyToNeighborOrganisms" => inst_relinquish_energy_to_neighbor_organisms,
        "Inst_RelinquishEnergyToOrganismsInDeme" => |_, o, w| {
            let stored_energy = o.phenotype.energy_store * w.config().frac_energy_relinquish;
            w.divide_org_testament_among_deme(stored_energy);
            w.stats_add_energy_testament(TestamentTarget::DemeOrganisms, stored_energy);
            o.die(w);
            Ok(true)
        },
        _ => return None,
    };
    Some(f)
}

/// ?BX? and the next two nop-selected registers (`FindModifiedNextRegister`).
fn set_three(hw: &mut HardwareCpu, a: i32, b: i32, c: i32) {
    let reg1 = hw.find_modified_register(REG_BX);
    let reg2 = hw.find_modified_next_register(reg1);
    let reg3 = hw.find_modified_next_register(reg2);
    hw.set_reg(reg1, a);
    hw.set_reg(reg2, b);
    hw.set_reg(reg3, c);
}

/// Whether the instruction after the IP is a nop
/// (`m_inst_set->IsNop(getIP().GetNextInst())`).
fn next_is_nop(hw: &HardwareCpu) -> bool {
    let next = hw.ip_head().get_next_inst(&hw.memory, &hw.base.inst_set);
    hw.base.inst_set.is_nop(next)
}

/// `JOIN_GROUP_FAILURE`: `Some(())` when the join attempt ends here (the
/// handler returns true), after killing the organism for a negative
/// percentage.
fn join_group_failure(org: &mut OrgState, world: &mut dyn OrgInterface) -> Option<()> {
    let percent_failure = world.config().join_group_failure;
    if percent_failure == 0 {
        return None;
    }
    let prob_failure = (f64::from(percent_failure) / 100.0).abs();
    let rand = world.rng().get_double();
    if rand <= prob_failure && percent_failure > 0 {
        return Some(());
    } else if rand <= prob_failure && percent_failure < 0 {
        org.die(world);
        return Some(());
    }
    None
}

/// `Inst_JoinGroup`: join the group in ?BX? (leaving the current one).
fn inst_join_group(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let reg = hw.find_modified_register(REG_BX);
    let prop_group_id = hw.reg(reg);
    if world.config().use_form_groups == 2 && !world.environment().is_group_id(prop_group_id) {
        return Ok(false);
    }
    if world.has_opinion(org) {
        let opinion = org.get_opinion().0;
        if opinion == prop_group_id {
            return Ok(false);
        }
        if join_group_failure(org, world).is_some() {
            return Ok(true);
        }
        if world.config().tolerance_window > 0 {
            world.attempt_immigrate_group(prop_group_id, org);
            return Ok(true);
        }
        world.leave_group(org, opinion);
    }
    world.set_opinion(prop_group_id, org);
    if world.has_opinion(org) {
        let opinion = org.get_opinion().0;
        world.join_group(org, opinion);
    }
    Ok(true)
}

/// The neighbouring group of `opinion` in the direction of `reg_value`,
/// wrapping from the top group to group 1 (skipping 0); `fallback` when
/// `reg_value` is 0.
fn next_group(opinion: i32, num_groups: i32, reg_value: i32, fallback: i32) -> i32 {
    let top = num_groups - 1;
    if opinion == top {
        if reg_value > 0 {
            1
        } else if reg_value < 0 {
            opinion - 1
        } else {
            fallback
        }
    } else if opinion == 1 || opinion == 0 {
        if reg_value > 0 {
            opinion + 1
        } else if reg_value < 0 {
            top
        } else {
            fallback
        }
    } else if reg_value > 0 {
        opinion + 1
    } else if reg_value < 0 {
        opinion - 1
    } else {
        fallback
    }
}

/// `Inst_JoinNextGroup`: move one group up (positive nop register) or down.
fn inst_join_next_group(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if !world.has_opinion(org) {
        return Ok(false);
    }
    if world.config().use_form_groups != 2 {
        return Ok(false);
    }
    let num_groups = world.environment().group_ids().len() as i32;
    if num_groups <= 2 {
        return Ok(false);
    }
    if !next_is_nop(hw) {
        return Ok(false);
    }
    let nop_reg = hw.find_modified_register(REG_BX);
    let reg_value = hw.reg(nop_reg);
    if reg_value == 0 {
        return Ok(false);
    }
    if join_group_failure(org, world).is_some() {
        return Ok(true);
    }
    let opinion = org.get_opinion().0;
    let new_opinion = next_group(opinion, num_groups, reg_value, -1);
    if new_opinion == -1 {
        return Ok(false);
    }
    if world.config().tolerance_window > 0 {
        world.attempt_immigrate_group(new_opinion, org);
    } else {
        world.set_opinion(new_opinion, org);
        world.leave_group(org, opinion);
        world.join_group(org, new_opinion);
    }
    Ok(true)
}

/// `Inst_NumberNextGroup` / `Inst_NumberMTNextGroup`: BX = members of the
/// neighbouring group (the number of groups is the resource count).
///
/// QUIRK: the mating-type version computes the group but writes nothing.
fn number_next_group(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    mt: bool,
) -> R {
    if !world.has_opinion(org) {
        return Ok(false);
    }
    if world.config().use_form_groups != 2 {
        return Ok(false);
    }
    let opinion = org.get_opinion().0;
    let num_groups = world.get_resources(org).len() as i32;
    if num_groups <= 2 {
        return Ok(false);
    }
    if !next_is_nop(hw) {
        return Ok(false);
    }
    let nop_reg = hw.find_modified_register(REG_BX);
    let reg_value = hw.reg(nop_reg);
    if reg_value == 0 {
        return Ok(false);
    }
    let query_group = next_group(opinion, num_groups, reg_value, opinion);
    if !mt {
        let n = world.number_of_organisms_in_group(query_group);
        hw.set_reg(REG_BX, n);
    }
    Ok(true)
}

/// `Inst_IncTolerance` (`inc`) / `Inst_DecTolerance`: the nop picks the
/// tolerance (A immigrants, B own offspring, C other offspring) unless
/// `TOLERANCE_VARIATIONS`; BX = the new tolerance.
fn change_tolerance(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    inc: bool,
) -> R {
    let cfg = world.config();
    if cfg.use_form_groups == 0 || cfg.tolerance_window <= 0 {
        return Ok(false);
    }
    if !world.has_opinion(org) {
        return Ok(false);
    }
    if !next_is_nop(hw) {
        return Ok(false);
    }
    let mut tolerance_type = 0;
    if world.config().tolerance_variations == 0 {
        tolerance_type = match hw.find_modified_register(REG_BX) {
            REG_AX => 0,
            REG_BX => 1,
            REG_CX => 2,
            _ => return Ok(false),
        };
    }
    if inc {
        let result = world.inc_tolerance(org, tolerance_type);
        if result == -1 {
            return Ok(false);
        }
        hw.set_reg(REG_BX, result);
    } else {
        let result = world.dec_tolerance(org, tolerance_type);
        hw.set_reg(REG_BX, result);
    }
    Ok(true)
}

/// `Inst_GetTolerance`: AX / BX / CX = the three own tolerances.
fn inst_get_tolerance(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cfg = world.config();
    if cfg.use_form_groups == 0 || cfg.tolerance_window <= 0 || !world.has_opinion(org) {
        return Ok(false);
    }
    world.push_tolerance_inst_exe(org, 6);
    let cfg = world.config().clone();
    let update = world.update();
    let tolerance_immigrants = org.phenotype.calc_tolerance_immigrants(&cfg, update);
    let tolerance_own = org.phenotype.calc_tolerance_offspring_own(&cfg, update);
    let tolerance_others = org.phenotype.calc_tolerance_offspring_others(&cfg, update);
    hw.set_reg(REG_AX, tolerance_immigrants);
    hw.set_reg(REG_BX, tolerance_own);
    hw.set_reg(REG_CX, tolerance_others);
    Ok(true)
}

/// `Inst_GetGroupTolerance`: AX / BX / CX = the group's admission odds
/// for immigrants, own offspring and others' offspring, in percent.
fn inst_get_group_tolerance(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let cfg = world.config();
    if cfg.use_form_groups == 0 || cfg.tolerance_window <= 0 || !world.has_opinion(org) {
        return Ok(false);
    }
    world.push_tolerance_inst_exe(org, 7);
    let group_id = org.get_opinion().0;
    let mut mating_type = -1;
    if world.config().tolerance_variations == 2 {
        mating_type = match org.phenotype.mating_type {
            MATING_TYPE_FEMALE => 0,
            MATING_TYPE_MALE => 1,
            _ => 2,
        };
    }
    let immigrant_odds = world.calc_group_odds_immigrants(org, group_id, mating_type);
    let offspring_own_odds = world.calc_group_odds_offspring_parent(org);
    let offspring_others_odds = world.calc_group_odds_offspring(org, group_id);
    hw.set_reg(REG_AX, immigrant_odds.mul_add(100.0, 0.5) as i32);
    hw.set_reg(REG_BX, offspring_own_odds.mul_add(100.0, 0.5) as i32);
    hw.set_reg(REG_CX, offspring_others_odds.mul_add(100.0, 0.5) as i32);
    Ok(true)
}

/// `Inst_NetworkBroadcast1` (`broadcast`) / `Inst_NetworkUnicast`: label
/// from ?BX?, data from the next register.
fn network_message(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    broadcast: bool,
) -> R {
    let label_reg = hw.find_modified_register(REG_BX);
    let data_reg = HardwareCpu::find_next_register(label_reg);
    let mut msg = new_message(org, world, 0);
    msg.label = hw.reg(label_reg) as u32;
    msg.data = hw.reg(data_reg) as u32;
    Ok(if broadcast {
        world.network_broadcast(&mut msg)
    } else {
        world.network_unicast(&mut msg)
    })
}

/// `Inst_RelinquishEnergyToNeighborOrganisms`: leave a share of the
/// stored energy to each occupied neighbouring cell, then die.
fn inst_relinquish_energy_to_neighbor_organisms(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let stored_energy = org.phenotype.energy_store * world.config().frac_energy_relinquish;
    let mut num_occupied = 0;
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.is_neighbor_cell_occupied() {
            num_occupied += 1;
        }
        world.rotate(1);
        i += 1;
    }
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.is_neighbor_cell_occupied() {
            let share = stored_energy / f64::from(num_occupied);
            // The organism itself when it faces its own cell.
            with_neighbor_state(world, org, |o| o.phenotype.energy_testament(share));
        }
        world.rotate(1);
        i += 1;
    }
    world.stats_add_energy_testament(TestamentTarget::NeighborOrganisms, stored_energy);
    org.die(world);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::next_group;

    #[test]
    fn next_group_wraps_past_zero() {
        // groups 0..=4
        assert_eq!(next_group(4, 5, 1, -1), 1);
        assert_eq!(next_group(4, 5, -1, -1), 3);
        assert_eq!(next_group(1, 5, -1, -1), 4);
        assert_eq!(next_group(0, 5, 1, -1), 1);
        assert_eq!(next_group(2, 5, 1, -1), 3);
        assert_eq!(next_group(2, 5, 0, 7), 7);
    }
}
