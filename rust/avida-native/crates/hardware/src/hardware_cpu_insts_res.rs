//! Resource sensing and collection handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb): `sense-*`,
//! `collect-*`, `if-resources`, the `if-*-res-level` family, the pheromone
//! sensors and `res-move-head` / `res-jump-head`.
//!
//! Resource vectors follow the C++ indexing: `GetResources(ctx)` and
//! `GetDemeResources` are concatenated where the C++ uses
//! `Apto::Array::operator+`; the population's `cResourceCount` and a
//! deme's `cResourceCount` are indexed by their own (non-deme / deme)
//! resource order, which [`global_res_names`] / [`deme_res_names`] rebuild
//! from the environment as `cPopulation::cPopulation` and
//! `cDeme::SetupDemeRes` do.
//!
//! UB substitutes: an out-of-range read of a resource array is 0 and an
//! out-of-range write is dropped (the C++ reads and writes past the
//! array); see [`at`].

use crate::hardware_cpu::{HardwareCpu, REG_AX, REG_BX};
use crate::hardware_cpu_insts::{inst_jump_head, inst_move_head, InstFn, R};
use crate::nhardware::*;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_IfAboveResLevel" => inst_if_above_res_level,
        "Inst_IfAboveResLevelEnd" => inst_if_above_res_level_end,
        "Inst_IfNotAboveResLevel" => inst_if_not_above_res_level,
        "Inst_IfNotAboveResLevelEnd" => inst_if_not_above_res_level_end,
        "Inst_SenseLog2" => |h, o, w| do_sense(h, o, w, 0, 2.0),
        "Inst_SenseUnit" => |h, o, w| do_sense(h, o, w, 1, 1.0),
        "Inst_SenseMult100" => |h, o, w| do_sense(h, o, w, 1, 100.0),
        "Inst_SenseResource0" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 0),
        "Inst_SenseResource1" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 1),
        "Inst_SenseResource2" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 2),
        // The faced variants pass `GetFacedCellID()`, which DoSenseResourceX
        // ignores.
        "Inst_SenseFacedResource0" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 0),
        "Inst_SenseFacedResource1" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 1),
        "Inst_SenseFacedResource2" => |h, o, w| do_sense_resource_x(h, o, w, REG_BX, 2),
        "Inst_SenseResourceID" => inst_sense_resource_id,
        "Inst_SenseOpinionResourceQuantity" => inst_sense_opinion_resource_quantity,
        "Inst_SenseNextResLevel" => inst_sense_next_res_level,
        "Inst_SenseDiffFaced" => inst_sense_diff_faced,
        "Inst_SenseFacedHabitat" => inst_sense_faced_habitat,
        "Inst_Collect" => |h, o, w| do_collect(h, o, w, true, true, false, false, 1.0),
        "Inst_CollectNoEnvRemove" => |h, o, w| do_collect(h, o, w, false, true, false, false, 1.0),
        "Inst_Destroy" => |h, o, w| do_collect(h, o, w, true, false, false, false, 1.0),
        "Inst_NopCollect" => |h, o, w| do_collect(h, o, w, false, false, false, false, 1.0),
        "Inst_CollectUnitProbabilistic" => {
            |h, o, w| do_collect(h, o, w, true, true, true, true, 1.0)
        }
        "Inst_CollectSpecific" => inst_collect_specific,
        "Inst_CollectSpecificNeeded" => inst_collect_specific_needed,
        "Inst_CollectSpecificRatio" => inst_collect_specific_ratio,
        "Inst_IfResources" => inst_if_resources,
        "Inst_ResMoveHead" => inst_res_move_head,
        "Inst_ResJumpHead" => inst_res_jump_head,
        "Inst_SenseLog2Facing" => |h, o, w| do_sense_facing(h, o, w, 0, 2.0),
        "Inst_SenseUnitFacing" => |h, o, w| do_sense_facing(h, o, w, 1, 1.0),
        "Inst_SenseMult100Facing" => |h, o, w| do_sense_facing(h, o, w, 1, 100.0),
        "Inst_SensePheromone" => inst_sense_pheromone,
        "Inst_SensePheromoneFaced" => inst_sense_pheromone_faced,
        "Inst_SensePheromoneInDemeGlobal" => |h, _, w| do_sense_pheromone_in_deme_global(h, w),
        "Inst_SensePheromoneGlobal" => |h, o, w| do_sense_pheromone_global(h, o, w, REG_BX),
        "Inst_SensePheromoneGlobal_defaultAX" => {
            |h, o, w| do_sense_pheromone_global(h, o, w, REG_AX)
        }
        _ => return None,
    };
    Some(f)
}

// ----------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------

/// `values[i]`; an out-of-range read (undefined in C++) gives 0.
pub(crate) fn at(values: &[f64], i: i32) -> f64 {
    usize::try_from(i)
        .ok()
        .and_then(|i| values.get(i))
        .copied()
        .unwrap_or(0.0)
}

/// `values[i] = v`; an out-of-range write (undefined in C++) is dropped.
pub(crate) fn set_at(values: &mut [f64], i: i32, v: f64) {
    if let Some(slot) = usize::try_from(i).ok().and_then(|i| values.get_mut(i)) {
        *slot = v;
    }
}

/// Resource names of the population's `cResourceCount`: the non-deme
/// resources in library order (`cPopulation::cPopulation`).
pub(crate) fn global_res_names(world: &dyn OrgInterface) -> Vec<String> {
    world
        .environment()
        .resource_lib()
        .resources()
        .iter()
        .filter(|r| !r.deme_resource)
        .map(|r| r.name().to_string())
        .collect()
}

/// Resource names of a deme's `cResourceCount`: the deme resources in
/// library order (`cDeme::SetupDemeRes`).
pub(crate) fn deme_res_names(world: &dyn OrgInterface) -> Vec<String> {
    world
        .environment()
        .resource_lib()
        .resources()
        .iter()
        .filter(|r| r.deme_resource)
        .map(|r| r.name().to_string())
        .collect()
}

/// `strncmp(name, "pheromone", 9) == 0`
pub(crate) fn is_pheromone_name(name: &str) -> bool {
    name.starts_with("pheromone")
}

/// `GetResources(ctx) + GetDemeResources(GetDemeID(), ctx)`
fn resources_with_deme(org: &mut OrgState, world: &mut dyn OrgInterface) -> Vec<f64> {
    let mut res = world.get_resources(org);
    let deme_id = world.get_deme_id();
    res.extend(world.get_deme_resources(deme_id));
    res
}

/// `(int) ceil(log((double) n) / log((double) num_nops))`, the label length
/// that addresses `n` resources.
fn max_label_length(n: usize, num_nops: i32) -> i32 {
    ((n as f64).ln() / f64::from(num_nops).ln()).ceil() as i32
}

/// The pheromone level `DoSensePheromoneGlobal` and the
/// `if-*-res-level` family read: `resLib.GetResource("pheromone")`, then
/// `GetResources(ctx)[id]` if the population resource with the same index
/// is named `pheromone*`.
///
/// The C++ dereferences NULL when the environment has no resource called
/// `pheromone` (after printing `cResourceLib::GetResource`'s error); that
/// is the `Err`. An index past the population's resources (possible when
/// `pheromone` is a deme resource) is undefined in C++ and reads as "not a
/// pheromone" here.
fn global_pheromone_amount(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    inst: &str,
) -> Result<f64, String> {
    // `get_resource_id` prints `cResourceLib::GetResource`'s error.
    let Some(id) = world
        .environment()
        .resource_lib()
        .get_resource_id("pheromone")
    else {
        return Err(format!(
            "unsupported: {inst} without a resource named 'pheromone' (C++: cHardwareCPU.cc:cHardwareCPU::Inst_IfAboveResLevel / DoSensePheromoneGlobal dereference NULL)"
        ));
    };
    let resource_count_array = world.get_resources(org);
    let names = global_res_names(world);
    let mut pher_amount = 0.0;
    if names.get(id).is_some_and(|n| is_pheromone_name(n)) {
        pher_amount += at(&resource_count_array, id as i32);
    }
    Ok(pher_amount)
}

/// The label arithmetic shared by `DoSense` and `DoSenseFacing`: read a
/// label of up to the length that addresses every resource, sum (or log)
/// the resources it may denote and put the result in BX.
///
/// QUIRK: the C++ caches the label length in function statics keyed only
/// on the number of resources, so a later instruction set with a
/// different nop count would reuse the first set's length. The port
/// recomputes it, which agrees whenever all instruction sets of a run
/// have the same number of nops. `IncSenseCount` is a no-op in the C++.
fn sense_label(
    hw: &mut HardwareCpu,
    world: &dyn OrgInterface,
    res_count: &[f64],
    conversion_method: i32,
    base: f64,
) -> R {
    let reg_to_set = REG_BX;
    if res_count.is_empty() {
        return Ok(false);
    }
    let num_nops = hw.base.inst_set.num_nops();
    let n = res_count.len() as i32;
    let max_label_length = max_label_length(res_count.len(), num_nops);
    let m = world.config().max_label_exe_size;
    hw.read_label_max(m, max_label_length);
    let real_label_length = hw.label().get_size();

    let mut start_label = hw.label().clone();
    let mut end_label = hw.label().clone();
    for _ in 0..max_label_length.wrapping_sub(real_label_length) {
        start_label.add_nop(0);
        end_label.add_nop(num_nops - 1);
    }
    let start_index = start_label.as_int(num_nops);
    let end_index = end_label.as_int(num_nops);
    if start_index >= n {
        return Ok(false);
    }

    let mut resource_result: i32 = 0;
    let mut dresource_result = 0.0;
    let mut i = start_index;
    while i <= end_index {
        if i < n {
            if conversion_method == 0 {
                dresource_result += res_count[i as usize];
            } else if conversion_method == 1 {
                let add_amount = (res_count[i as usize] * base) as i32;
                // Signed overflow in the C++ range check wraps here.
                resource_result = if i32::MAX.wrapping_sub(resource_result) <= add_amount {
                    i32::MAX
                } else {
                    resource_result.wrapping_add(add_amount)
                };
            }
        }
        i += 1;
    }
    if conversion_method == 0 {
        resource_result = if dresource_result == 0.0 {
            i32::MIN
        } else {
            (dresource_result.ln() / base.ln()) as i32
        };
    }
    hw.set_reg(reg_to_set, resource_result);
    Ok(true)
}

// ----------------------------------------------------------------------
// if-*-res-level
// ----------------------------------------------------------------------

/// `Inst_IfAboveResLevel`: skip the next instruction when the global
/// pheromone level exceeds 100.
fn inst_if_above_res_level(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    w: &mut dyn OrgInterface,
) -> R {
    if global_pheromone_amount(org, w, "if-above-res-level")? > 100.0 {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_IfAboveResLevelEnd`
fn inst_if_above_res_level_end(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    w: &mut dyn OrgInterface,
) -> R {
    if global_pheromone_amount(org, w, "if-above-res-level-end")? > 100.0 {
        hw.else_top_half();
    }
    Ok(true)
}

/// `Inst_IfNotAboveResLevel`
fn inst_if_not_above_res_level(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    w: &mut dyn OrgInterface,
) -> R {
    if global_pheromone_amount(org, w, "if-not-above-res-level")? <= 100.0 {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_IfNotAboveResLevelEnd`
fn inst_if_not_above_res_level_end(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    w: &mut dyn OrgInterface,
) -> R {
    if global_pheromone_amount(org, w, "if-not-above-res-level-end")? <= 100.0 {
        hw.else_top_half();
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Sensing
// ----------------------------------------------------------------------

/// `DoSense(ctx, conversion_method, base)`
fn do_sense(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    conversion_method: i32,
    base: f64,
) -> R {
    let res_count = resources_with_deme(org, world);
    sense_label(hw, world, &res_count, conversion_method, base)
}

/// `DoSenseFacing(ctx, conversion_method, base)`: as `DoSense` on the
/// population's resources in the faced cell. Returns true without reading
/// a label when there is no faced cell (the test CPU).
fn do_sense_facing(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    conversion_method: i32,
    base: f64,
) -> R {
    let faced_id = world.get_faced_cell_id();
    if faced_id < 0 {
        return Ok(true);
    }
    let res_count = world.get_cell_resources(org, faced_id);
    sense_label(hw, world, &res_count, conversion_method, base)
}

/// `DoSenseResourceX(reg_to_set, cell_id, resid, ctx)` (`cell_id` is
/// unused by the C++).
fn do_sense_resource_x(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    reg_to_set: usize,
    resid: usize,
) -> R {
    let res_count = resources_with_deme(org, world);
    if resid >= res_count.len() {
        return Ok(false);
    }
    hw.set_reg(reg_to_set, res_count[resid] as i32);
    Ok(true)
}

/// `Inst_SenseResourceID`: the id of the most abundant resource.
fn inst_sense_resource_id(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let res_count = world.get_resources(org);
    let reg_to_set = hw.find_modified_register(REG_BX);
    let mut max_resource = 0.0;
    for (i, &r) in res_count.iter().enumerate() {
        if r > max_resource {
            max_resource = r;
            hw.set_reg(reg_to_set, i as i32);
        }
    }
    Ok(true)
}

/// `Inst_SenseOpinionResourceQuantity`
fn inst_sense_opinion_resource_quantity(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let res_count = world.get_resources(org);
    if world.has_opinion(org) {
        let opinion = org.get_opinion().0;
        let res_opinion = at(&res_count, opinion).mul_add(100.0, 0.5) as i32;
        let reg_to_set = hw.find_modified_register(REG_BX);
        hw.set_reg(reg_to_set, res_opinion);
    }
    Ok(true)
}

/// `Inst_SenseNextResLevel`
fn inst_sense_next_res_level(
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
    let opinion = org.get_opinion().0;
    let num_groups = world.get_resources(org).len() as i32;
    if num_groups <= 2 {
        return Ok(false);
    }
    let is = hw.base.inst_set.clone();
    if !is.is_nop(hw.ip_head().get_next_inst(&hw.memory, &is)) {
        return Ok(false);
    }
    let nop_register = hw.find_modified_register(REG_BX);
    let register_value = hw.reg(nop_register);
    if register_value == 0 {
        return Ok(false);
    }
    let res_count = world.get_resources(org);
    let level = |i: i32| at(&res_count, i).mul_add(100.0, 0.5) as i32;
    if opinion == num_groups - 1 {
        if register_value > 0 {
            hw.set_reg(REG_BX, level(1));
        } else if register_value < 0 {
            hw.set_reg(REG_BX, level(opinion - 1));
        }
    } else if opinion == 1 || opinion == 0 {
        if register_value > 0 {
            hw.set_reg(REG_BX, level(opinion + 1));
        } else if register_value < 0 {
            hw.set_reg(REG_BX, level(num_groups - 1));
        }
    } else if register_value > 0 {
        hw.set_reg(REG_BX, level(opinion + 1));
    } else if register_value < 0 {
        hw.set_reg(REG_BX, level(opinion - 1));
    }
    Ok(true)
}

/// `Inst_SenseDiffFaced`: percentage difference of the opinion resource
/// between the faced cell and here.
fn inst_sense_diff_faced(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let res_count = world.get_resources(org);
    if world.has_opinion(org) {
        let opinion = org.get_opinion().0;
        let reg_to_set = hw.find_modified_register(REG_BX);
        let faced_res = at(&world.get_faced_cell_resources(org), opinion);
        let here = at(&res_count, opinion);
        let res_diff = if here == 0.0 {
            faced_res as i32
        } else {
            ((faced_res - here) / here).mul_add(100.0, 0.5) as i32
        };
        hw.set_reg(reg_to_set, res_diff);
    }
    Ok(true)
}

/// `Inst_SenseFacedHabitat`: 2 if a barrier resource is present in the
/// faced cell, else 1 for a hill, else 0.
fn inst_sense_faced_habitat(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let reg_to_set = hw.find_modified_register(REG_BX);
    let cell_resource_levels = world.get_faced_cell_resources(org);
    let habitats: Vec<i32> = {
        let lib = world.environment().resource_lib();
        lib.resources().iter().map(|r| r.habitat).collect()
    };
    for wanted in [2, 1] {
        for (i, &level) in cell_resource_levels.iter().enumerate() {
            if habitats.get(i) == Some(&wanted) && level > 0.0 {
                hw.set_reg(reg_to_set, wanted);
                return Ok(true);
            }
        }
    }
    hw.set_reg(reg_to_set, 0);
    Ok(true)
}

// ----------------------------------------------------------------------
// Collection
// ----------------------------------------------------------------------

/// `FindModifiedResource(ctx, spec_id)`: the resource bin a (possibly
/// partial) nop label selects, drawing uniformly within the range a
/// partial label covers. `None` when there are no resources.
fn find_modified_resource(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> Option<(i32, i32)> {
    let num_resources = world.get_resources(org).len() as i32;
    if num_resources <= 0 {
        return None;
    }
    let num_nops = hw.base.inst_set.num_nops();
    let max_label_length = max_label_length(num_resources as usize, num_nops);
    let m = world.config().max_label_exe_size;
    hw.read_label_max(m, max_label_length);
    let real_label_length = hw.label().get_size();
    let spec_id = hw.label().as_int_unique(num_nops);
    let label_int = hw.label().as_int(num_nops);
    let chunk_size = 1.0 / f64::from(num_nops).powf(f64::from(real_label_length));
    let r = world.rng().get_double_max(chunk_size);
    let resource_approx = f64::from(label_int).mul_add(chunk_size, r);
    let bin_used = (resource_approx * f64::from(num_resources)).floor() as i32;
    Some((bin_used, spec_id))
}

/// `DoCollect(ctx, env_remove, internal_add, probabilistic, unit, nUnits)`
#[allow(clippy::too_many_arguments)]
fn do_collect(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    env_remove: bool,
    internal_add: bool,
    probabilistic: bool,
    unit: bool,
    n_units: f32,
) -> R {
    let Some((bin_used, spec_id)) = find_modified_resource(hw, org, world) else {
        return Ok(false);
    };
    // `IncCollectSpecCount(spec_id)`
    if let Some(c) = usize::try_from(spec_id)
        .ok()
        .and_then(|i| org.phenotype.cur_collect_spec_counts.get_mut(i))
    {
        *c = c.wrapping_add(1);
    }
    do_actual_collect(
        org,
        world,
        bin_used,
        env_remove,
        internal_add,
        probabilistic,
        unit,
        n_units,
    )
}

/// `DoActualCollect(ctx, bin_used, env_remove, internal_add,
/// probabilistic, unit, nUnits)`. `nUnits` is a C++ `float`.
#[allow(clippy::too_many_arguments)]
fn do_actual_collect(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    bin_used: i32,
    env_remove: bool,
    internal_add: bool,
    probabilistic: bool,
    unit: bool,
    n_units: f32,
) -> R {
    let res_count = world.get_resources(org);
    let mut res_change = vec![0.0; res_count.len()];
    let total = org.rbins_total();
    let max = world.config().max_total_stored;
    let level = at(&res_count, bin_used);

    if probabilistic {
        let success_chance = level / f64::from(world.config().collect_prob_divisor);
        if success_chance < world.rng().get_double() {
            return Ok(false);
        }
    }

    let n_units = f64::from(n_units);
    let mut change = if unit {
        if level >= n_units {
            -n_units
        } else {
            return Ok(false);
        }
    } else {
        -(level * world.config().absorb_resource_fraction)
    };
    set_at(&mut res_change, bin_used, change);

    if internal_add && (max < 0.0 || (total + -change) <= max) {
        org.add_to_rbin(bin_used, -change);
    }
    if !env_remove || (max >= 0.0 && (total + -change) > max) {
        change = 0.0;
        set_at(&mut res_change, bin_used, change);
    }
    world.update_resources(org, &res_change);
    Ok(true)
}

/// `Inst_CollectSpecific`: collect `COLLECT_AMOUNT` of
/// `COLLECT_SPECIFIC_RESOURCE`; the amount gained goes in ?BX?.
fn inst_collect_specific(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let resource = world.config().collect_specific_resource;
    let amount = world.config().collect_amount as f32;
    let res_before = org.rbin(resource);
    let success = do_actual_collect(org, world, resource, true, true, false, false, amount)?;
    let res_after = org.rbin(resource);
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, (res_after - res_before) as i32);
    Ok(success)
}

/// `Inst_CollectSpecificNeeded`: probabilistically collect the resource
/// whose index is the opcode under the read head.
///
/// UB substitute: an opcode at or past the number of resources reads a
/// level of 0 (so the probabilistic test fails unless the RNG draws 0) and
/// stores nothing; the C++ reads and writes past the arrays.
fn inst_collect_specific_needed(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    hw.head_adjust(HEAD_READ);
    let resource = hw.get_head(HEAD_READ).get_inst(&hw.memory).op();
    let amount = world.config().collect_amount as f32;
    let res_before = org.rbin(resource);
    let success = do_actual_collect(org, world, resource, false, true, true, true, amount)?;
    let res_after = org.rbin(resource);
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, (res_after - res_before) as i32);
    Ok(success)
}

/// C `atoi`: optional whitespace and sign, then decimal digits (overflow,
/// undefined in C, wraps).
pub(crate) fn c_atoi(s: &str) -> i32 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let mut v: i32 = 0;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add(i32::from(b - b'0'));
    }
    if neg {
        v.wrapping_neg()
    } else {
        v
    }
}

/// C `atof` on a decimal number (`[ws][sign]digits[.digits][e[sign]digits]`
/// prefix; anything else gives 0).
pub(crate) fn c_atof(s: &str) -> f64 {
    let t = s.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let b = t.as_bytes();
    let mut end = 0;
    if matches!(b.first(), Some(b'+') | Some(b'-')) {
        end = 1;
    }
    let digits_start = end;
    while end < b.len() && b[end].is_ascii_digit() {
        end += 1;
    }
    let mut mantissa_digits = end - digits_start;
    if end < b.len() && b[end] == b'.' {
        end += 1;
        let frac_start = end;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        mantissa_digits += end - frac_start;
    }
    if mantissa_digits == 0 {
        return 0.0;
    }
    if end < b.len() && (b[end] == b'e' || b[end] == b'E') {
        let mut e = end + 1;
        if e < b.len() && (b[e] == b'+' || b[e] == b'-') {
            e += 1;
        }
        let exp_start = e;
        while e < b.len() && b[e].is_ascii_digit() {
            e += 1;
        }
        if e > exp_start {
            end = e;
        }
    }
    t[..end].parse::<f64>().unwrap_or(0.0)
}

/// `Inst_CollectSpecificRatio`: collect every resource in the ratios of
/// `NON_1_RESOURCE_RATIOS` (`"id:ratio,id:ratio"`, scaled by
/// `COLLECT_AMOUNT`); ?BX? gets the amount gained of the last resource.
///
/// The C++ stops calling `DoActualCollect` after the first failure
/// (`success = success && ...`). An odd number of tokens makes the C++
/// call `strlen(NULL)`; that is the `Err`.
fn inst_collect_specific_ratio(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let ratios = world.config().non_1_resource_ratios.clone();
    let tokens: Vec<&str> = ratios.split([',', ':']).filter(|t| !t.is_empty()).collect();
    if !tokens.len().is_multiple_of(2) {
        return Err(format!(
            "unsupported: NON_1_RESOURCE_RATIOS={ratios:?} has an odd number of tokens (C++: cHardwareCPU.cc:cHardwareCPU::Inst_CollectSpecificRatio calls strlen(NULL))"
        ));
    }
    // `map<int, float>`: later entries overwrite earlier ones.
    let mut ratio_map: Vec<(i32, f32)> = Vec::new();
    for pair in tokens.chunks(2) {
        let idx = c_atoi(pair[0]);
        let val = c_atof(pair[1]) as f32;
        match ratio_map.iter_mut().find(|(k, _)| *k == idx) {
            Some(slot) => slot.1 = val,
            None => ratio_map.push((idx, val)),
        }
    }

    let mut res_before = 0.0;
    let mut res_after = 0.0;
    let mut success = true;
    let n = org.rbins().len() as i32;
    for i in 0..n {
        let mut coll_amnt = world.config().collect_amount as f32;
        if let Some((_, r)) = ratio_map.iter().find(|(k, _)| *k == i) {
            coll_amnt *= *r;
        }
        res_before = org.rbin(i);
        if success {
            success = do_actual_collect(org, world, i, false, true, false, true, coll_amnt)?;
        }
        res_after = org.rbin(i);
    }
    let r = hw.find_modified_register(REG_BX);
    hw.set_reg(r, (res_after - res_before) as i32);
    Ok(success)
}

/// `Inst_IfResources`: skip the next instruction when some reaction
/// process lacks its minimum resource level here.
fn inst_if_resources(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let resources = resources_with_deme(org, world);
    let lacking = world
        .environment()
        .reaction_lib()
        .reactions()
        .iter()
        .flat_map(|rx| rx.processes.iter())
        .any(|p| {
            p.resource
                .is_some_and(|id| at(&resources, id as i32) < p.min_number)
        });
    if lacking {
        hw.ip_advance();
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Resource-dependent head moves
// ----------------------------------------------------------------------

/// The `INST_RES` level `Inst_ResMoveHead` / `Inst_ResJumpHead` read: the
/// organism's deme's resource of that name, else the population's.
/// `Ok(None)` after the C++ "Cannot find resource" message.
///
/// The C++ looks the deme up through `GetCell(GetCellID())`, which is
/// undefined for the test CPU's cell -1; that is the `Err`.
fn inst_res_level(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    inst: &str,
) -> Result<Option<f64>, String> {
    let cell_id = world.get_cell_id();
    if cell_id < 0 {
        return Err(format!(
            "unsupported: {inst} in a test CPU (C++: cHardwareCPU.cc:cHardwareCPU::Inst_ResMoveHead indexes cPopulation::GetCell(-1))"
        ));
    }
    let deme_id = world.cell_deme_id(cell_id);
    let resname = world.config().inst_res.clone();
    if let Some(i) = deme_res_names(world).iter().position(|n| *n == resname) {
        return Ok(Some(world.deme_resource_amount(deme_id, i as i32)));
    }
    if let Some(i) = global_res_names(world).iter().position(|n| *n == resname) {
        return Ok(Some(world.population_resource_amount(org, i as i32)));
    }
    println!("Error: Cannot find resource '{resname}'");
    Ok(None)
}

/// `(current_level - floor) / (ceil - floor)` with `INST_RES_FLOOR` /
/// `INST_RES_CEIL`.
fn inst_res_fraction(world: &dyn OrgInterface, current_level: f64) -> f64 {
    let floor = world.config().inst_res_floor;
    let ceil = world.config().inst_res_ceil;
    (current_level - floor) / (ceil - floor)
}

/// `Inst_ResMoveHead`: `move-head` with probability given by the
/// `INST_RES` level.
fn inst_res_move_head(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let Some(level) = inst_res_level(org, world, "res-move-head")? else {
        return Ok(true);
    };
    let frac = inst_res_fraction(world, level);
    if world.rng().p(frac) {
        return inst_move_head(hw, org, world);
    }
    Ok(true)
}

/// `Inst_ResJumpHead`: `jump-head` with probability given by the
/// `INST_RES` level.
fn inst_res_jump_head(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let Some(level) = inst_res_level(org, world, "res-jump-head")? else {
        return Ok(true);
    };
    let frac = inst_res_fraction(world, level);
    if world.rng().p(frac) {
        return inst_jump_head(hw, org, world);
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Pheromone sensing
// ----------------------------------------------------------------------

/// The summed `pheromone*` levels of a deme resource array.
pub(crate) fn deme_pheromone_sum(names: &[String], cell_resources: &[f64]) -> f64 {
    let mut pher_amount = 0.0;
    for (i, name) in names.iter().enumerate() {
        if is_pheromone_name(name) {
            pher_amount += at(cell_resources, i as i32);
        }
    }
    pher_amount
}

/// `DoSensePheromone(ctx, cellid)`: the pheromone level of a cell's deme
/// resources, rounded, in ?BX?.
fn do_sense_pheromone(hw: &mut HardwareCpu, world: &mut dyn OrgInterface, cellid: i32) -> R {
    let reg_to_set = hw.find_modified_register(REG_BX);
    if cellid == -1 {
        return Ok(false);
    }
    let deme_id = world.cell_deme_id(cellid);
    let cell_resources = world.get_deme_cell_resources(deme_id, cellid);
    let names = deme_res_names(world);
    if names.is_empty() {
        return Ok(false);
    }
    let pher_amount = deme_pheromone_sum(&names, &cell_resources);
    hw.set_reg(reg_to_set, (pher_amount + 0.5).floor() as i32);
    Ok(true)
}

/// `Inst_SensePheromone`
fn inst_sense_pheromone(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    do_sense_pheromone(hw, world, cellid)
}

/// `Inst_SensePheromoneFaced`
fn inst_sense_pheromone_faced(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    let fcellid = world.get_faced_cell_id();
    do_sense_pheromone(hw, world, fcellid)
}

/// `DoSensePheromoneInDemeGlobal(ctx, REG_BX)`: the deme-wide pheromone
/// total.
fn do_sense_pheromone_in_deme_global(hw: &mut HardwareCpu, world: &mut dyn OrgInterface) -> R {
    if world.get_cell_id() == -1 {
        return Ok(false);
    }
    let reg_to_set = hw.find_modified_register(REG_BX);
    let deme_id = world.get_deme_id();
    let names = deme_res_names(world);
    let mut pher_amount = 0.0;
    for (i, name) in names.iter().enumerate() {
        if is_pheromone_name(name) {
            pher_amount += world.deme_resource_amount(deme_id, i as i32);
        }
    }
    hw.set_reg(reg_to_set, (pher_amount + 0.5).floor() as i32);
    Ok(true)
}

/// `DoSensePheromoneGlobal(ctx, reg_default)`
fn do_sense_pheromone_global(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    reg_default: usize,
) -> R {
    let reg_to_set = hw.find_modified_register(reg_default);
    let pher_amount = global_pheromone_amount(org, world, "sense-pheromone-global")?;
    hw.set_reg(reg_to_set, (pher_amount + 0.5).floor() as i32);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_number_parsing_matches_libc() {
        assert_eq!(c_atoi(" 12abc"), 12);
        assert_eq!(c_atoi("-3"), -3);
        assert_eq!(c_atoi("x"), 0);
        assert_eq!(c_atof("1.5"), 1.5);
        assert_eq!(c_atof(" 2e1x"), 20.0);
        assert_eq!(c_atof("3e"), 3.0);
        assert_eq!(c_atof("."), 0.0);
        assert_eq!(c_atof("-.5"), -0.5);
    }

    #[test]
    fn label_length_addresses_every_resource() {
        assert_eq!(max_label_length(1, 3), 0);
        assert_eq!(max_label_length(3, 3), 1);
        assert_eq!(max_label_length(4, 3), 2);
    }

    #[test]
    fn out_of_range_resource_reads_zero() {
        let v = [1.0, 2.0];
        assert_eq!(at(&v, 1), 2.0);
        assert_eq!(at(&v, 2), 0.0);
        assert_eq!(at(&v, -1), 0.0);
    }
}
