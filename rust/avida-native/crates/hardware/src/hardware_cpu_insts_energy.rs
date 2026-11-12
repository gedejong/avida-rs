//! Energy-level tests and cell-event / vitality handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb: `Inst_IfEnergyLow`
//! through `Inst_GetFacedEnergyRequestStatus`, and `Inst_KillCellEvent`
//! through `Inst_GetAttackOdds` minus the cell-data readers already in
//! the core table), with `cOrganism::GetVitality`
//! (avida-core/source/main/cOrganism.cc).
//!
//! Every energy instruction fails in the test CPU (cell id -1).

use crate::hardware_cpu::{HardwareCpu, REG_BX};
use crate::hardware_cpu_insts::{arm_div, InstFn, R};
use crate::hardware_cpu_insts_donate::{neighbor, with_nb};
use crate::org_interface::{with_neighbor_state, OrgInterface};
use crate::organism::OrgState;
use avida_phenotype::phenotype::{ENERGY_LEVEL_HIGH, ENERGY_LEVEL_LOW, ENERGY_LEVEL_MEDIUM};

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_IfEnergyLow" => |h, o, w| if_own_level(h, o, w, ENERGY_LEVEL_LOW, true),
        "Inst_IfEnergyNotLow" => |h, o, w| if_own_level(h, o, w, ENERGY_LEVEL_LOW, false),
        "Inst_IfEnergyHigh" => |h, o, w| if_own_level(h, o, w, ENERGY_LEVEL_HIGH, true),
        "Inst_IfEnergyNotHigh" => |h, o, w| if_own_level(h, o, w, ENERGY_LEVEL_HIGH, false),
        "Inst_IfEnergyMed" => |h, o, w| if_own_level(h, o, w, ENERGY_LEVEL_MEDIUM, true),
        "Inst_IfFacedEnergyLow" => |h, o, w| if_faced_level(h, o, w, ENERGY_LEVEL_LOW, true),
        "Inst_IfFacedEnergyNotLow" => |h, o, w| if_faced_level(h, o, w, ENERGY_LEVEL_LOW, false),
        "Inst_IfFacedEnergyHigh" => |h, o, w| if_faced_level(h, o, w, ENERGY_LEVEL_HIGH, true),
        "Inst_IfFacedEnergyNotHigh" => |h, o, w| if_faced_level(h, o, w, ENERGY_LEVEL_HIGH, false),
        "Inst_IfFacedEnergyMed" => |h, o, w| if_faced_level(h, o, w, ENERGY_LEVEL_MEDIUM, true),
        "Inst_IfFacedEnergyLess" => |h, o, w| if_faced_energy_cmp(h, o, w, true),
        "Inst_IfFacedEnergyMore" => |h, o, w| if_faced_energy_cmp(h, o, w, false),
        "Inst_IfEnergyInBuffer" => |h, o, w| {
            if w.get_cell_id() < 0 {
                return Ok(false);
            }
            h.skip_if(o.phenotype.energy_received_buffer == 0.0);
            Ok(true)
        },
        "Inst_IfEnergyNotInBuffer" => |h, o, w| {
            if w.get_cell_id() < 0 {
                return Ok(false);
            }
            h.skip_if(o.phenotype.energy_received_buffer > 0.0);
            Ok(true)
        },
        "Inst_GetEnergyLevel" => |h, o, w| {
            if w.get_cell_id() < 0 {
                return Ok(false);
            }
            let reg = h.find_modified_register(REG_BX);
            h.set_reg(reg, o.phenotype.energy_store.floor() as i32);
            Ok(true)
        },
        "Inst_GetFacedEnergyLevel" => |h, o, w| {
            let Some(energy) = live_faced(o, w, |o| o.phenotype.energy_store) else {
                return Ok(false);
            };
            let reg = h.find_modified_register(REG_BX);
            h.set_reg(reg, energy.floor() as i32);
            Ok(true)
        },
        "Inst_IfFacedEnergyRequestOn" => |h, o, w| {
            let Some(req) = live_faced(o, w, |o| o.phenotype.is_energy_requestor) else {
                return Ok(false);
            };
            h.skip_if(!req);
            Ok(true)
        },
        "Inst_IfFacedEnergyRequestOff" => |h, o, w| {
            let Some(req) = live_faced(o, w, |o| o.phenotype.is_energy_requestor) else {
                return Ok(false);
            };
            h.skip_if(req);
            Ok(true)
        },
        "Inst_GetEnergyRequestStatus" => |h, o, w| {
            if w.get_cell_id() < 0 {
                return Ok(false);
            }
            let reg = h.find_modified_register(REG_BX);
            h.set_reg(reg, i32::from(o.phenotype.is_energy_requestor));
            Ok(true)
        },
        "Inst_GetFacedEnergyRequestStatus" => |h, o, w| {
            let Some(req) = live_faced(o, w, |o| o.phenotype.is_energy_requestor) else {
                return Ok(false);
            };
            let reg = h.find_modified_register(REG_BX);
            h.set_reg(reg, i32::from(req));
            Ok(true)
        },
        "Inst_KillCellEvent" => |h, o, w| {
            if w.get_deme_id() < 0 || w.get_cell_id() < 0 {
                return Ok(false);
            }
            let reg = h.find_modified_register(REG_BX);
            let event_id = w.get_cell_data();
            let deme_id = w.get_deme_id();
            let killed = w.deme_kill_cell_event(o, deme_id, event_id);
            h.set_reg(reg, i32::from(killed));
            Ok(true)
        },
        "Inst_KillFacedCellEvent" => |h, o, w| {
            if w.get_deme_id() < 0 || w.get_cell_id() < 0 {
                return Ok(false);
            }
            let reg = h.find_modified_register(REG_BX);
            let event_id = w.get_neighbor_cell_contents();
            let deme_id = w.get_deme_id();
            let killed = w.deme_kill_cell_event(o, deme_id, event_id);
            h.set_reg(reg, i32::from(killed));
            if killed {
                o.killed_event = true;
            }
            Ok(true)
        },
        "Inst_CollectCellDataAndKillEvent" => |h, o, w| {
            if w.get_deme_id() < 0 || w.get_cell_id() < 0 {
                return Ok(false);
            }
            let out_reg = h.find_modified_register(REG_BX);
            let event_id = w.get_cell_data();
            h.set_reg(out_reg, event_id);
            let deme_id = w.get_deme_id();
            w.deme_kill_cell_event(o, deme_id, event_id);
            Ok(true)
        },
        "Inst_ReadFacedCellData" => inst_read_faced_cell_data,
        "Inst_MarkCellWithVitality" => |_, o, w| {
            let my_vit = (vitality(o, w)? + 0.5) as i32;
            w.set_cell_data(o, my_vit);
            Ok(true)
        },
        "Inst_GetFacedVitalityDiff" => inst_get_faced_vitality_diff,
        "Inst_AttackFacedOrg" => inst_attack_faced_org,
        "Inst_GetAttackOdds" => inst_get_attack_odds,
        _ => return None,
    };
    Some(f)
}

/// `if (GetDiscreteEnergyLevel() != level) Advance()` (`is` true) or
/// `== level` (`is` false); fails outside a population cell.
fn if_own_level(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    level: i32,
    is: bool,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let cur = org.phenotype.discrete_energy_level(world.config());
    hw.skip_if((cur != level) == is);
    Ok(true)
}

/// The faced-organism form of [`if_own_level`]: no live neighbour, no skip.
fn if_faced_level(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    level: i32,
    is: bool,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let cfg = world.config().clone();
    if let Some(nb) = neighbor(world, org) {
        let cur = with_nb(world, org, nb, |o| {
            (!o.is_dead).then(|| o.phenotype.discrete_energy_level(&cfg))
        })
        .flatten();
        if let Some(cur) = cur {
            hw.skip_if((cur != level) == is);
        }
    }
    Ok(true)
}

/// `Inst_IfFacedEnergyLess` (`less`) / `Inst_IfFacedEnergyMore`: compare
/// with the own store, `ENERGY_COMPARISON_EPSILON` relative slack.
fn if_faced_energy_cmp(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    less: bool,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    if let Some(nb) = neighbor(world, org) {
        let ne = with_nb(world, org, nb, |o| {
            (!o.is_dead).then_some(o.phenotype.energy_store)
        })
        .flatten();
        if let Some(neighbor_energy) = ne {
            let my_energy = org.phenotype.energy_store;
            let epsilon = world.config().energy_comparison_epsilon;
            let skip = if less {
                neighbor_energy >= my_energy * (1.0 - epsilon)
            } else {
                neighbor_energy <= my_energy * (1.0 + epsilon)
            };
            hw.skip_if(skip);
        }
    }
    Ok(true)
}

/// The guard of the `GetFaced...` / `IfFacedEnergyRequest...` handlers:
/// `None` (the handler fails) outside a cell or without a live faced
/// organism, else `f` of the neighbour (`org` itself when it faces its own
/// cell).
fn live_faced<T>(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    f: impl FnOnce(&OrgState) -> T,
) -> Option<T> {
    if world.get_cell_id() < 0 {
        return None;
    }
    with_neighbor_state(world, org, |o| (!o.is_dead).then(|| f(o))).flatten()
}

/// `cOrganism::GetVitality()`: the `COLLECT_SPECIFIC_RESOURCE` bin scaled
/// by `VITALITY_BIN_EXTREMES` for ages beyond one standard deviation of the
/// population mean, else by `VITALITY_BIN_CENTER`. A bin index past the
/// environment's resources is an error: the C++ reads past
/// `m_cur_rbins_avail` (the reference binary crashes, e.g. with
/// `mark-cell-with-vitality` in an environment without resources).
fn vitality_with(
    org: &OrgState,
    cfg: &avida_config::config::Config,
    age_stats: (f64, f64),
) -> Result<f64, String> {
    let (mean_age, age_stddev) = age_stats;
    let org_age = f64::from(org.phenotype.age);
    let resource = cfg.collect_specific_resource;
    let mut res_level = 0.0;
    if resource >= 0 {
        let bins = &org.phenotype.cur_rbins_avail;
        res_level = bins.get(resource as usize).copied().ok_or_else(|| {
            format!(
                "unsupported: cOrganism::GetVitality with COLLECT_SPECIFIC_RESOURCE {resource} \
                 and {} resource bins (C++: reads past cPhenotype::m_cur_rbins_avail)",
                bins.len()
            )
        })?;
    }
    Ok(
        if org_age < (mean_age - age_stddev) || org_age > (mean_age + age_stddev) {
            cfg.vitality_bin_extremes * res_level
        } else {
            cfg.vitality_bin_center * res_level
        },
    )
}

/// [`vitality_with`] of the running organism.
fn vitality(org: &OrgState, world: &dyn OrgInterface) -> Result<f64, String> {
    vitality_with(org, world.config(), world.stats_creature_age_mean_stddev())
}

/// The faced live organism's vitality and cell (`org` itself when it faces
/// its own cell); `Ok(None)` when the faced cell is empty or its organism
/// dead (the handler fails).
fn faced_vitality(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> Result<Option<(f64, i32)>, String> {
    if !world.is_neighbor_cell_occupied() {
        return Ok(None);
    }
    let cfg = world.config().clone();
    let stats = world.stats_creature_age_mean_stddev();
    let out = with_neighbor_state(world, org, |o| {
        (!o.is_dead).then(|| vitality_with(o, &cfg, stats))
    });
    let Some(v) = out.flatten().transpose()? else {
        return Ok(None);
    };
    let cell = world.get_faced_cell_id();
    Ok(Some((v, cell)))
}

/// `Inst_ReadFacedCellData`: ?BX? = faced cell data relative to the own
/// rounded vitality, in percent (integer arithmetic). A zero vitality, a
/// division by zero in C++, gives 0 as on the arm64 reference build.
fn inst_read_faced_cell_data(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let out_reg = hw.find_modified_register(REG_BX);
    let my_vit = (vitality(org, world)? + 0.5) as i32;
    let vit_diff =
        arm_div(world.get_faced_cell_data().wrapping_sub(my_vit), my_vit).wrapping_mul(100);
    hw.set_reg(out_reg, vit_diff);
    Ok(true)
}

/// `Inst_GetFacedVitalityDiff`: ?BX? = relative vitality difference to the
/// faced organism, in percent.
fn inst_get_faced_vitality_diff(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let Some((nv, _)) = faced_vitality(org, world)? else {
        return Ok(false);
    };
    let out_reg = hw.find_modified_register(REG_BX);
    let mv = vitality(org, world)?;
    // The reference binary computes this in the Rust helper
    // `avd_cpu_inst_get_faced_vitality_diff`: 0 for a zero own vitality
    // (the pre-Rust C++ divided by it), and an unfused multiply and add.
    let vit_diff = if mv != 0.0 {
        let scaled = (nv - mv) / mv * 100.0;
        (scaled + 0.5) as i32
    } else {
        0
    };
    hw.set_reg(out_reg, vit_diff);
    Ok(true)
}

/// `Inst_AttackFacedOrg`: fight the faced organism; the loser dies
/// (`MOVEMENT_COLLISIONS_SELECTION_TYPE` 0: a coin flip; 1: by vitality,
/// possibly nobody).
fn inst_attack_faced_org(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let Some((target_vitality, target_cell)) = faced_vitality(org, world)? else {
        return Ok(false);
    };
    let mut kill_attacker = true;
    let selection = world.config().movement_collisions_selection_type;
    if selection == 0 {
        kill_attacker = world.rng().p(0.5);
    } else if selection == 1 {
        let attacker_vitality = vitality(org, world)?;
        let attacker_win_odds = attacker_vitality / (attacker_vitality + target_vitality);
        let target_win_odds = target_vitality / (attacker_vitality + target_vitality);
        let odds_someone_dies = cmax(attacker_win_odds, target_win_odds);
        let odds_target_dies = (1.0 - target_win_odds) * odds_someone_dies;
        let decider = world.rng().get_double_max(1.0);
        if decider < (1.0 - odds_someone_dies) {
            return Ok(true);
        } else if decider < ((1.0 - odds_someone_dies) + odds_target_dies) {
            kill_attacker = false;
        }
    }
    if kill_attacker {
        org.die(world);
        return Ok(true);
    }
    // `AttackFacedOrg(ctx, target_cell)`: the attacker itself when it faces
    // its own cell (killed without `Die`'s `m_is_dead`, as in the C++).
    world.attack_faced_org(org, target_cell);
    Ok(true)
}

/// `Inst_GetAttackOdds`: ?BX? = the percent chance of surviving an attack
/// on the faced organism.
fn inst_get_attack_odds(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let Some((target_vitality, _)) = faced_vitality(org, world)? else {
        return Ok(false);
    };
    let out_reg = hw.find_modified_register(REG_BX);
    // The reference binary computes the odds in the Rust helper
    // `avd_cpu_inst_get_attack_odds`: 50 when the two vitalities sum to 0
    // (the pre-Rust C++ divided by the sum), the larger odds by `>`, and
    // unfused multiplies and adds.
    let attacker_vitality = vitality(org, world)?;
    let total = attacker_vitality + target_vitality;
    if total == 0.0 {
        hw.set_reg(out_reg, 50);
        return Ok(true);
    }
    let attacker_win_odds = attacker_vitality / total;
    let target_win_odds = target_vitality / total;
    let odds_someone_dies = if attacker_win_odds > target_win_odds {
        attacker_win_odds
    } else {
        target_win_odds
    };
    let odds_target_dies = (1.0 - target_win_odds) * odds_someone_dies;
    let odds_i_dont_die = (1.0 - odds_someone_dies) + odds_target_dies;
    let percent = odds_i_dont_die * 100.0;
    hw.set_reg(out_reg, (percent + 0.5) as i32);
    Ok(true)
}

/// `std::max(a, b)`: `a` unless `a < b` (NaN parity).
fn cmax(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}
