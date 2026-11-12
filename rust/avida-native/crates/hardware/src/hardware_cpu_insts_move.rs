//! Movement handlers of `cHardwareCPU` (avida-core/source/cpu/
//! cHardwareCPU.cc at 47f13dadb): `tumble`, the state-grid `sg-*`
//! instructions, `move` and its event / target / pheromone-guided variants,
//! the pheromone drop switches and `get-distance-from-diagonal`, plus
//! `cOrganism::Move` (avida-core/source/main/cOrganism.cc).
//!
//! `m_organism->Rotate(ctx, d)` and `cell.ConnectionList().CircNext()`
//! are both [`OrgInterface::rotate`]: `USE_AVATARS` is rejected at start-up,
//! which leaves `cPopulationInterface::Rotate` a plain `CircNext` /
//! `CircPrev`.

use crate::hardware_cpu::{HardwareCpu, REG_BX, REG_CX};
use crate::hardware_cpu_insts::{InstFn, R};
use crate::hardware_cpu_insts_res::{deme_pheromone_sum, deme_res_names};
use crate::nhardware::MOVE_INTERRUPT;
use crate::org_interface::OrgInterface;
use crate::organism::OrgState;

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_Tumble" => inst_tumble,
        "Inst_SGMove" => inst_sg_move,
        "Inst_SGRotateL" => inst_sg_rotate_l,
        "Inst_SGRotateR" => inst_sg_rotate_r,
        "Inst_SGSense" => inst_sg_sense,
        "Inst_Move" => inst_move,
        "Inst_MoveToEvent" => inst_move_to_event,
        "Inst_IfNeighborEventInUnoccupiedCell" => inst_if_neighbor_event_in_unoccupied_cell,
        "Inst_IfFacingEventCell" => inst_if_facing_event_cell,
        "Inst_IfEventInCell" => inst_if_event_in_cell,
        "Inst_GetDistanceFromDiagonal" => inst_get_distance_from_diagonal,
        "Inst_PheroOn" => |_, o, _| {
            o.pher_drop = true;
            Ok(true)
        },
        "Inst_PheroOff" => |_, o, _| {
            o.pher_drop = false;
            Ok(true)
        },
        "Inst_PheroToggle" => |_, o, _| {
            o.pher_drop = !o.pher_drop;
            Ok(true)
        },
        "Inst_SenseTarget" => inst_sense_target,
        "Inst_SenseTargetFaced" => inst_sense_target_faced,
        "Inst_Exploit" => |h, o, w| exploit(h, o, w, &[]),
        "Inst_ExploitForward5" => |h, o, w| exploit(h, o, w, &[3, 4, 5]),
        "Inst_ExploitForward3" => |h, o, w| exploit(h, o, w, &[2, 3, 4, 5, 6]),
        "Inst_Explore" => inst_explore,
        "Inst_MoveTarget" => |h, o, w| move_target(h, o, w, &[], true),
        "Inst_MoveTargetForward5" => |h, o, w| move_target(h, o, w, &[3, 4, 5], false),
        "Inst_MoveTargetForward3" => |h, o, w| move_target(h, o, w, &[2, 3, 4, 5, 6], false),
        "Inst_SuperMove" => inst_super_move,
        "Inst_IfTarget" => inst_if_target,
        "Inst_IfNotTarget" => inst_if_not_target,
        "Inst_IfPheromone" => |h, _, w| if_pheromone(h, w, false),
        "Inst_IfNotPheromone" => |h, _, w| if_pheromone(h, w, true),
        "Inst_DropPheromone" => inst_drop_pheromone,
        _ => return None,
    };
    Some(f)
}

/// Task descriptions that make `cOrganism::Move` test outputs.
pub(crate) const MOVE_TASK_DESCS: [&str; 11] = [
    "move_up_gradient",
    "move_neutral_gradient",
    "move_down_gradient",
    "move_not_up_gradient",
    "move_to_right_side",
    "move_to_left_side",
    "move",
    "movetotarget",
    "movetoevent",
    "movebetweenevent",
    "move_to_event",
];

/// `cOrganism::Move(ctx)`: move into the faced cell, keep the step
/// counters, drop pheromone, test move tasks and raise the move
/// interrupt. Returns the C++ result.
pub(crate) fn org_move(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> Result<bool, String> {
    if org.is_dead {
        return Ok(false);
    }
    let here = world.get_cell_data();
    let faced = world.get_faced_cell_data();
    org.gradient_movement = if here < faced {
        1.0
    } else if here == faced {
        0.0
    } else {
        -1.0
    };

    let fromcell_id = world.get_cell_id();
    let destcell_id = world.get_faced_cell_id();
    let facing = world.get_faced_dir();

    if !world.move_org(org, fromcell_id, destcell_id) {
        return Ok(false);
    }
    let err = world.config().step_counting_error;
    if err == 0 || world.rng().get_int_range(0, 101) > err {
        let (dn, de) = match facing {
            0 => (-1, 0),
            1 => (-1, 1),
            2 => (0, 1),
            3 => (1, 1),
            4 => (1, 0),
            5 => (1, -1),
            6 => (0, -1),
            7 => (-1, -1),
            _ => (0, 0),
        };
        org.northerly = org.northerly.wrapping_add(dn);
        org.easterly = org.easterly.wrapping_add(de);
    }
    if org.phenotype.to_delete {
        return Ok(false);
    }
    world.stats_move(org);

    let cfg = world.config();
    if cfg.pheromone_enabled && org.pher_drop {
        let pher_amount = cfg.pheromone_amount;
        let drop_mode = cfg.pheromone_drop_mode;
        let deme_id = world.get_deme_id();
        match drop_mode {
            0 => {
                world.deme_add_pheromone(deme_id, fromcell_id, pher_amount / 2.0);
                world.deme_add_pheromone(deme_id, destcell_id, pher_amount / 2.0);
            }
            1 => world.deme_add_pheromone(deme_id, fromcell_id, pher_amount),
            2 => world.deme_add_pheromone(deme_id, destcell_id, pher_amount),
            _ => {}
        }
    }

    let has_move_task = {
        let env = world.environment();
        let tl = env.tasklib();
        (0..env.num_tasks())
            .any(|i| MOVE_TASK_DESCS.contains(&avida_environment::TaskRegistry::task_desc(tl, i)))
    };
    if has_move_task {
        hw.org_do_output(org, world, None, false)?;
    }
    if world.config().active_messages_enabled > 0 {
        hw.interrupt_thread_impl(org, world, MOVE_INTERRUPT)?;
    }
    Ok(true)
}

/// `Inst_Tumble`: turn a random 1 to N-1 steps.
fn inst_tumble(_: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let num_neighbors = world.get_num_neighbors();
    if 0 < num_neighbors {
        let irot = world.rng().get_uint((num_neighbors - 1) as u32) as i32;
        for _ in 0..=irot {
            world.rotate(1);
        }
    }
    Ok(true)
}

/// The organism's state grid and the extended memory holding its
/// position (`m_ext_mem`: x, y, facing, per-state counts, history).
///
/// The C++ indexes `m_ext_mem` and the environment's state grids without
/// checks; with no state grid (empty extended memory) that is undefined,
/// and is the `Err` here.
fn state_grid_check(
    hw: &HardwareCpu,
    org: &OrgState,
    world: &dyn OrgInterface,
    inst: &str,
) -> Result<avida_environment::StateGrid, String> {
    let n = world.environment().num_state_grids();
    let sg = org.cur_sg;
    if hw.base.ext_mem.len() < 3 || sg < 0 || sg >= n {
        return Err(format!(
            "unsupported: {inst} without a state grid (C++: cHardwareCPU.cc:cHardwareCPU::Inst_SGMove indexes an empty m_ext_mem)"
        ));
    }
    Ok(world.environment().state_grid(sg as usize).clone())
}

/// `sg.GetStateAt(x, y)` with the position checked.
fn sg_state(sg: &avida_environment::StateGrid, x: i32, y: i32) -> Result<i32, String> {
    if x < 0 || y < 0 || x >= sg.width() || y >= sg.height() {
        return Err(format!(
            "unsupported: state-grid position ({x}, {y}) off the grid (C++: undefined read)"
        ));
    }
    Ok(sg.state_at(x, y))
}

/// `Inst_SGMove`: step on the toroidal state grid in the facing direction,
/// count the state reached and record the position.
fn inst_sg_move(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let sg = state_grid_check(hw, org, world, "sg-move")?;
    let (w, h) = (sg.width(), sg.height());
    let mem = &mut hw.base.ext_mem;
    let (mut x, mut y) = (mem[0], mem[1]);
    let facing = mem[2];
    let inc = |v: &mut i32, lim: i32| {
        *v = v.wrapping_add(1);
        if *v == lim {
            *v = 0;
        }
    };
    let dec = |v: &mut i32, lim: i32| {
        *v = v.wrapping_sub(1);
        if *v == -1 {
            *v = lim - 1;
        }
    };
    match facing {
        0 => inc(&mut y, h),
        1 => {
            inc(&mut x, w);
            inc(&mut y, h);
        }
        2 => inc(&mut x, w),
        3 => {
            inc(&mut x, w);
            dec(&mut y, h);
        }
        4 => dec(&mut y, h),
        5 => {
            dec(&mut x, w);
            dec(&mut y, h);
        }
        6 => dec(&mut x, w),
        7 => {
            dec(&mut x, w);
            inc(&mut y, h);
        }
        _ => {}
    }
    mem[0] = x;
    mem[1] = y;
    let state = sg_state(&sg, x, y)?;
    let slot = 3usize.wrapping_add(state as usize);
    match mem.get_mut(slot) {
        Some(v) => *v = v.wrapping_add(1),
        None => {
            return Err(format!(
                "unsupported: state {state} past the extended memory (C++: undefined write)"
            ))
        }
    }
    mem.push(sg.id_for(x, y));
    Ok(true)
}

/// `Inst_SGRotateL`
fn inst_sg_rotate_l(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    state_grid_check(hw, org, world, "sg-rotate-l")?;
    let f = &mut hw.base.ext_mem[2];
    *f = f.wrapping_sub(1);
    if *f < 0 {
        *f = 7;
    }
    Ok(true)
}

/// `Inst_SGRotateR`
fn inst_sg_rotate_r(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    state_grid_check(hw, org, world, "sg-rotate-r")?;
    let f = &mut hw.base.ext_mem[2];
    *f = f.wrapping_add(1);
    if *f > 7 {
        *f = 0;
    }
    Ok(true)
}

/// `Inst_SGSense`: the sense value of the current state-grid cell.
fn inst_sg_sense(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let sg = state_grid_check(hw, org, world, "sg-sense")?;
    let reg_used = hw.find_modified_register(REG_BX);
    let (x, y) = (hw.base.ext_mem[0], hw.base.ext_mem[1]);
    let state = sg_state(&sg, x, y)?;
    let v = usize::try_from(state)
        .ok()
        .and_then(|s| sg.sense_values().get(s).copied())
        .ok_or_else(|| {
            format!("unsupported: state {state} without a sense value (C++: undefined read)")
        })?;
    hw.set_reg(reg_used, v);
    Ok(true)
}

/// `Inst_Move`: move into the faced cell; ?BX? gets whether it worked.
/// Fails in the test CPU.
pub(crate) fn inst_move(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_id() == -1 {
        return Ok(false);
    }
    let move_success = org_move(hw, org, world)?;
    let out_reg = hw.find_modified_register(REG_BX);
    hw.set_reg(out_reg, i32::from(move_success));
    Ok(true)
}

/// `Inst_MoveToEvent`: turn towards a neighbouring event cell and move.
fn inst_move_to_event(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let reg_used = hw.find_modified_register(REG_BX);
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.get_neighbor_cell_contents() > 0 {
            inst_move(hw, org, world)?;
            hw.set_reg(reg_used, 1);
            return Ok(true);
        }
        world.rotate(1);
        i += 1;
    }
    inst_move(hw, org, world)?;
    hw.set_reg(reg_used, 0);
    Ok(true)
}

/// `Inst_IfNeighborEventInUnoccupiedCell`
fn inst_if_neighbor_event_in_unoccupied_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.get_neighbor_cell_contents() > 0 && !world.is_neighbor_cell_occupied() {
            return Ok(true);
        }
        world.rotate(1);
        i += 1;
    }
    hw.ip_advance();
    Ok(true)
}

/// `Inst_IfFacingEventCell`
fn inst_if_facing_event_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_neighbor_cell_contents() > 0 {
        return Ok(true);
    }
    hw.ip_advance();
    Ok(true)
}

/// `Inst_IfEventInCell`
fn inst_if_event_in_cell(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_data() > 0 {
        return Ok(true);
    }
    hw.ip_advance();
    Ok(true)
}

/// `Inst_GetDistanceFromDiagonal`: half the signed distance of the cell
/// from its deme's diagonal, rounded away from zero.
fn inst_get_distance_from_diagonal(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let absolute_cell_id = world.get_cell_id();
    let deme_id = world.get_deme_id();
    if deme_id < 0 || absolute_cell_id < 0 {
        return Ok(false);
    }
    let (x, y) = world.deme_cell_position(deme_id, absolute_cell_id);
    let reg = hw.find_modified_register(REG_BX);
    let half = f64::from(x.wrapping_sub(y)) / 2.0;
    let v = if x > y { half.ceil() } else { half.floor() };
    hw.set_reg(reg, v as i32);
    Ok(true)
}

/// `Inst_SenseTarget`: ?CX? = 1 if this cell holds a target.
fn inst_sense_target(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let reg_to_set = hw.find_modified_register(REG_CX);
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    let cell_data = world.population_cell_data(cellid);
    hw.set_reg(reg_to_set, i32::from(cell_data > 0));
    Ok(true)
}

/// `Inst_SenseTargetFaced`: ?CX? = 1 if the faced cell holds a target.
fn inst_sense_target_faced(
    hw: &mut HardwareCpu,
    _: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let reg_to_set = hw.find_modified_register(REG_CX);
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    let cell_data = world.get_faced_cell_data();
    hw.set_reg(reg_to_set, i32::from(cell_data > 0));
    Ok(true)
}

/// The summed pheromone in the faced cell, read through the organism's
/// deme (`deme.GetRelativeCellID(mycell.GetCellFaced().GetID())`).
fn faced_pheromone(world: &mut dyn OrgInterface, deme_id: i32, names: &[String]) -> f64 {
    let faced = world.get_faced_cell_id();
    let cell_resources = world.get_deme_cell_resources(deme_id, faced);
    deme_pheromone_sum(names, &cell_resources)
}

/// `Inst_Exploit` / `Inst_ExploitForward5` / `Inst_ExploitForward3`: with
/// probability `EXPLOIT_EXPLORE_PROB` turn randomly, else towards the
/// neighbour with most pheromone (skipping the neighbour indices in
/// `skip`), then move.
fn exploit(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    skip: &[i32],
) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    let deme_id = world.cell_deme_id(cellid);
    let names = deme_res_names(world);
    let mut num_rotations: i32 = 0;
    let mut max_pheromone = 0.0;
    let prob = world.config().exploit_explore_prob;
    if prob >= 0.0 && world.rng().p(prob) {
        let n = world.get_num_neighbors();
        num_rotations = world.rng().get_uint(n as u32) as i32;
    } else {
        let n = world.get_num_neighbors();
        for i in 0..n {
            if skip.contains(&i) {
                world.rotate(1);
                continue;
            }
            let phero_amount = faced_pheromone(world, deme_id, &names);
            if phero_amount > max_pheromone {
                num_rotations = i;
                max_pheromone = phero_amount;
            }
            world.rotate(1);
        }
    }
    for _ in 0..num_rotations {
        world.rotate(1);
    }
    org_move(hw, org, world)?;
    Ok(true)
}

/// `Inst_Explore`: turn randomly and move.
///
/// QUIRK: the loop bound `GetUInt(num_neighbors)` is redrawn on every
/// iteration.
fn inst_explore(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if world.get_cell_id() == -1 {
        return Ok(true);
    }
    let num_neighbors = world.get_num_neighbors();
    let mut i: u32 = 0;
    while i < world.rng().get_uint(num_neighbors as u32) {
        world.rotate(1);
        i += 1;
    }
    org_move(hw, org, world)?;
    Ok(true)
}

/// Rotate through the neighbours (skipping the indices in `skip`) and
/// return the index of the last one whose cell holds a target.
fn last_target_neighbor(world: &mut dyn OrgInterface, skip: &[i32]) -> i32 {
    let mut num_rotations = 0;
    let n = world.get_num_neighbors();
    for i in 0..n {
        if skip.contains(&i) {
            world.rotate(1);
            continue;
        }
        if world.get_faced_cell_data() > 0 {
            num_rotations = i;
        }
        world.rotate(1);
    }
    num_rotations
}

/// `Inst_MoveTarget` / `Inst_MoveTargetForward5` / `Inst_MoveTargetForward3`:
/// turn towards the last neighbouring target and move. Without a cell
/// (the test CPU) `move-target` succeeds and the forward variants fail.
fn move_target(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    skip: &[i32],
    test_cpu_result: bool,
) -> R {
    if world.get_cell_id() == -1 {
        return Ok(test_cpu_result);
    }
    let num_rotations = last_target_neighbor(world, skip);
    for _ in 0..num_rotations {
        world.rotate(1);
    }
    org_move(hw, org, world)?;
    Ok(true)
}

/// `Inst_SuperMove`: a random turn, overridden by the strongest pheromone,
/// overridden by a target; then move. The pheromone sums are C++ `float`s.
fn inst_super_move(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(false);
    }
    let deme_id = world.cell_deme_id(cellid);
    let names = deme_res_names(world);
    // The C++ reads the own cell's deme resources (and so runs the deme's
    // resource updates) before overwriting them unread.
    let _ = world.get_deme_cell_resources(deme_id, cellid);
    let num_neighbors = world.get_num_neighbors();
    let mut num_rotations = world.rng().get_uint(num_neighbors as u32) as i32;
    let mut max_pheromone: f32 = 0.0;
    let n = world.get_num_neighbors();
    for i in 0..n {
        let faced = world.get_faced_cell_id();
        let cell_resources = world.get_deme_cell_resources(deme_id, faced);
        let mut phero_amount: f32 = 0.0;
        for (j, name) in names.iter().enumerate() {
            if name.starts_with("pheromone") {
                let v = cell_resources.get(j).copied().unwrap_or(0.0);
                phero_amount = (f64::from(phero_amount) + v) as f32;
            }
        }
        if phero_amount > max_pheromone {
            num_rotations = i;
            max_pheromone = phero_amount;
        }
        world.rotate(1);
    }
    for i in 0..n {
        if world.get_faced_cell_data() > 0 {
            num_rotations = i;
        }
        world.rotate(1);
    }
    for _ in 0..num_rotations {
        world.rotate(1);
    }
    org_move(hw, org, world)?;
    Ok(true)
}

/// `Inst_IfTarget`: skip the next instruction when this cell's data is -1.
fn inst_if_target(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(true);
    }
    if world.population_cell_data(cellid) == -1 {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_IfNotTarget`: skip the next instruction when this cell holds a
/// target.
fn inst_if_not_target(hw: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(true);
    }
    if world.population_cell_data(cellid) > 0 {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_IfPheromone` (`negate == false`: skip when there is no
/// pheromone here) / `Inst_IfNotPheromone` (skip when there is some).
fn if_pheromone(hw: &mut HardwareCpu, world: &mut dyn OrgInterface, negate: bool) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(true);
    }
    let deme_id = world.cell_deme_id(cellid);
    let cell_resources = world.get_deme_cell_resources(deme_id, cellid);
    let names = deme_res_names(world);
    if names.is_empty() {
        return Ok(false);
    }
    let pher_amount = deme_pheromone_sum(&names, &cell_resources);
    let skip = if negate {
        pher_amount > 0.0
    } else {
        pher_amount == 0.0
    };
    if skip {
        hw.ip_advance();
    }
    Ok(true)
}

/// `Inst_DropPheromone`: add `PHEROMONE_AMOUNT` to this cell when
/// pheromones are enabled (whatever the organism's drop status).
fn inst_drop_pheromone(_: &mut HardwareCpu, _: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let cellid = world.get_cell_id();
    if cellid == -1 {
        return Ok(true);
    }
    let deme_id = world.cell_deme_id(cellid);
    if world.config().pheromone_enabled {
        let pher_amount = world.config().pheromone_amount;
        world.deme_add_pheromone(deme_id, cellid, pher_amount);
    }
    Ok(true)
}
