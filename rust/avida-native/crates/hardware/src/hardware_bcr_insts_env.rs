//! The environment `Inst_*` handlers of `cHardwareBCR` (avida-core/source/
//! cpu/cHardwareBCR.cc at 47f13dadb, from `Inst_SGMove` on) and the ones
//! `cHardwareGP8` shares with it: state grids, movement and rotation,
//! position offsets, resource sensing, the look sensor (`look-ahead*`,
//! `look-around*`, and the session-based `look-ahead-ex` /
//! `look-again-ex`), forage targets, resource collection, opinions and
//! groups, teaching, displays, attacks and register scrambling.
//!
//! The handlers are generic over [`GeneHw`]; where the GP8 text accepts an
//! immediate-value operand the handler follows [`GeneHw::IMM`].
//!
//! Most of these are close relatives of the `cHardwareExperimental` ones in
//! [`crate::hardware_experimental_insts_env`], whose organism-level helpers
//! (`cOrganism::MoveAV`, `SetForageTarget`, `CopyParentFT`,
//! `SetSimpDisplay`, the attack victim) they reuse. Where the BCR text
//! differs from the experimental one it is followed (noted as BCR).
//!
//! Paths the C++ leaves undefined (an out-of-range resource or bin index, a
//! modulo by zero target count, an unset state grid) and paths that need a
//! population service the interface does not offer (`InjectPreyClone`)
//! return `Err("unsupported: ...")`.

use crate::hardware_bcr::{org_do_output, GeneHw, R_AX, R_BX, R_CX, R_DX};
use crate::hardware_bcr_insts::{GeneInstFn, R, W};
use crate::hardware_cpu_insts_move::MOVE_TASK_DESCS;
use crate::hardware_experimental_insts_env::{
    copy_parent_ft, count_step, faced_org_info, injure_org, is_pred_ft, is_prey_ft, org_move_av,
    org_set_forage_target, res_info, set_simp_display, target_ids, test_prey_target,
    try_prey_clone, victim, victim_die, with_victim, Victim,
};
use crate::org_interface::{Faced, OrgInterface};
use crate::org_sensor::{LookInit, LookOut};
use crate::organism::OrgState;

/// `rGX`
const R_GX: usize = 6;

/// The environment handler for `method`, `None` when it is not one.
pub(crate) fn env_handler<H: GeneHw>(method: &str) -> Option<GeneInstFn<H>> {
    let f: GeneInstFn<H> = match method {
        "Inst_SGMove" => inst_sg_move,
        "Inst_SGRotateL" => |hw, org, w| sg_rotate(hw, org, w, -1),
        "Inst_SGRotateR" => |hw, org, w| sg_rotate(hw, org, w, 1),
        "Inst_SGSense" => inst_sg_sense,
        "Inst_Move" => |hw, org, w| inst_move(hw, org, w, false),
        "Inst_JuvMove" => |hw, org, w| inst_move(hw, org, w, true),
        "Inst_GetNorthOffset" => |hw, _, w| {
            let out_reg = hw.core_mut().fmr(R_BX);
            let compass_dir = if hw.core().use_avatar != 0 {
                w.get_av_facing(0)
            } else {
                w.get_faced_dir()
            };
            hw.core_mut().set_reg(out_reg, compass_dir, true);
            Ok(true)
        },
        "Inst_GetPositionOffset" => |hw, org, _| {
            let c = hw.core_mut();
            let out_reg = c.fmr(R_BX);
            c.set_reg(out_reg, org.northerly, true);
            let r2 = c.fmnr(out_reg);
            c.set_reg(r2, org.easterly, true);
            Ok(true)
        },
        "Inst_GetNortherly" => |hw, org, _| {
            let c = hw.core_mut();
            let out_reg = c.fmr(R_BX);
            c.set_reg(out_reg, org.northerly, true);
            Ok(true)
        },
        "Inst_GetEasterly" => |hw, org, _| {
            let c = hw.core_mut();
            let out_reg = c.fmr(R_BX);
            c.set_reg(out_reg, org.easterly, true);
            Ok(true)
        },
        "Inst_ZeroEasterly" => |_, org, _| {
            org.easterly = 0;
            Ok(true)
        },
        "Inst_ZeroNortherly" => |_, org, _| {
            org.northerly = 0;
            Ok(true)
        },
        "Inst_ZeroPosOffset" => |hw, org, _| {
            let c = hw.core_mut();
            let r = c.fmr(R_BX);
            // C++ `%`: a negative register gives a negative offset (no-op).
            match c.reg(r) % 3 {
                0 => {
                    org.easterly = 0;
                    org.northerly = 0;
                }
                1 => org.easterly = 0,
                2 => org.northerly = 0,
                _ => {}
            }
            Ok(true)
        },
        "Inst_RotateHome" => inst_rotate_home,
        "Inst_RotateUnoccupiedCell" => inst_rotate_unoccupied_cell,
        "Inst_RotateX" => inst_rotate_x,
        "Inst_RotateOrgID" => |hw, org, w| rotate_org_id(hw, org, w, false),
        "Inst_RotateAwayOrgID" => |hw, org, w| rotate_org_id(hw, org, w, true),
        "Inst_SenseResourceID" => inst_sense_resource_id,
        "Inst_SenseNest" => inst_sense_nest,
        "Inst_SenseFacedHabitat" => inst_sense_faced_habitat,
        "Inst_LookAhead" => |hw, org, w| look_ahead(hw, org, w, false),
        "Inst_LookAheadIntercept" => |hw, org, w| {
            hw.core_mut().sensor.set_return_relative_facing(true);
            look_ahead(hw, org, w, false)
        },
        "Inst_LookFT" => |hw, org, w| look_ahead(hw, org, w, true),
        "Inst_LookAround" => |hw, org, w| look_around(hw, org, w, false),
        "Inst_LookAroundIntercept" => |hw, org, w| {
            hw.core_mut().sensor.set_return_relative_facing(true);
            look_around(hw, org, w, false)
        },
        "Inst_LookAroundFT" => |hw, org, w| look_around(hw, org, w, true),
        "Inst_LookAheadEX" => |hw, org, w| look_ahead_ex(hw, org, w, false, false),
        "Inst_LookAheadFTX" => |hw, org, w| look_ahead_ex(hw, org, w, true, false),
        "Inst_LookAroundEX" => |hw, org, w| look_ahead_ex(hw, org, w, false, true),
        "Inst_LookAgainEX" => |hw, org, w| look_again_ex(hw, org, w, false),
        "Inst_LookAgainFTX" => |hw, org, w| look_again_ex(hw, org, w, true),
        "Inst_SetForageTarget" => inst_set_forage_target,
        "Inst_SetForageTargetOnce" => |hw, org, w| {
            if org.has_set_ft {
                return Ok(false);
            }
            inst_set_forage_target(hw, org, w)
        },
        "Inst_SetRandForageTargetOnce" => inst_set_rand_forage_target_once,
        "Inst_GetForageTarget" => |hw, org, _| {
            let c = hw.core_mut();
            let target_reg = c.fmr(R_BX);
            c.set_reg(target_reg, org.forage_target, false);
            Ok(true)
        },
        "Inst_CollectSpecific" => inst_collect_specific,
        "Inst_GetResStored" => inst_get_res_stored,
        "Inst_SetOpinion" => |hw, org, w| {
            let c = hw.core_mut();
            let r = c.fmr(R_BX);
            let v = c.reg(r);
            w.set_opinion(v, org);
            Ok(true)
        },
        "Inst_GetOpinion" => |hw, org, w| {
            if w.has_opinion(org) {
                let (opinion, since) = org.get_opinion();
                let update = w.update();
                let c = hw.core_mut();
                let opinion_reg = c.fmr(R_BX);
                let age_reg = c.next_reg(opinion_reg);
                c.set_reg(opinion_reg, opinion, true);
                c.set_reg(age_reg, update.wrapping_sub(since), true);
            }
            Ok(true)
        },
        "Inst_JoinGroup" => inst_join_group,
        "Inst_GetGroupID" => |hw, org, _| {
            if org.has_opinion() {
                let opinion = org.get_opinion().0;
                let c = hw.core_mut();
                let group_reg = c.fmr(R_BX);
                c.set_reg(group_reg, opinion, false);
            }
            Ok(true)
        },
        "Inst_GetFacedOrgID" => inst_get_faced_org_id,
        "Inst_TeachOffspring" => |_, org, _| {
            org.teach = true;
            Ok(true)
        },
        "Inst_LearnParent" => inst_learn_parent,
        "Inst_ModifySimpDisplay" => inst_modify_simp_display,
        "Inst_ReadLastSimpDisplay" => inst_read_last_simp_display,
        "Inst_KillDisplay" => |_, org, _| {
            if !org.display {
                return Ok(false);
            }
            org.display = false;
            Ok(true)
        },
        "Inst_AttackPrey" => inst_attack_prey,
        "Inst_AttackFTPrey" => inst_attack_ft_prey,
        "Inst_ScrambleReg" => |hw, _, w| {
            for i in 0..8 {
                let v = w.rng().get_int_full();
                hw.core_mut().set_reg(R_AX + i, v, true);
            }
            Ok(true)
        },
        _ => return None,
    };
    Some(f)
}

// ----------------------------------------------------------------------
// Small helpers
// ----------------------------------------------------------------------

/// The `Err` for a path reached at run time that is not ported.
pub(crate) fn unsupported<H: GeneHw>(what: &str, method: &str) -> String {
    let cls = H::CLASS;
    format!("unsupported: {what} (C++: {cls}.cc:{cls}::{method})")
}

/// `GetNeighborhoodSize()`, or `GetAVNumNeighbors()` with avatars.
fn num_neighbors<H: GeneHw>(hw: &H, w: &dyn OrgInterface) -> i32 {
    if hw.core().use_avatar != 0 {
        w.get_av_num_neighbors(0)
    } else {
        w.get_num_neighbors()
    }
}

/// `GetFacedDir()`, or `GetAVFacing()` with avatars.
fn facing<H: GeneHw>(hw: &H, w: &dyn OrgInterface) -> i32 {
    if hw.core().use_avatar != 0 {
        w.get_av_facing(0)
    } else {
        w.get_faced_dir()
    }
}

/// `GetCellID()`, or `GetAVCellID()` with avatars.
fn cell<H: GeneHw>(hw: &H, w: &dyn OrgInterface) -> i32 {
    if hw.core().use_avatar != 0 {
        w.get_av_cell_id(0)
    } else {
        w.get_cell_id()
    }
}

/// `GetResources(ctx)`, or `GetAVResources(ctx)` with avatars.
fn here_resources<H: GeneHw>(hw: &H, org: &mut OrgState, w: W<'_>) -> Vec<f64> {
    if hw.core().use_avatar != 0 {
        w.get_av_resources(org, 0)
    } else {
        w.get_resources(org)
    }
}

/// `array[i]`, the C++ reading out of bounds (undefined) for a bad index.
fn at<H: GeneHw>(v: &[f64], i: i32, method: &str) -> Result<f64, String> {
    usize::try_from(i)
        .ok()
        .and_then(|i| v.get(i).copied())
        .ok_or_else(|| {
            unsupported::<H>(
                &format!("resource index {i} outside the resource array (undefined in C++)"),
                method,
            )
        })
}

/// `set<int>::iterator` advanced `nth` times from the start of the target
/// ids (`nth <= 0`: the first).
fn nth_target<H: GeneHw>(ids: &[i32], nth: i32, method: &str) -> Result<i32, String> {
    let n = usize::try_from(nth).unwrap_or(0);
    ids.get(n).copied().ok_or_else(|| {
        unsupported::<H>(
            "walking past the end of the target-id set (undefined in C++)",
            method,
        )
    })
}

/// The "mod help" fold of an invalid forage target onto the valid ones:
/// -1 when that is the only target, else the `|prop| % num_fts`-th target
/// id (counted from the start of the whole id set). The C++ divides by zero
/// when there is no resource target and -1 is not a target either.
fn fold_target<H: GeneHw>(
    w: &dyn OrgInterface,
    prop_target: i32,
    method: &str,
) -> Result<i32, String> {
    let (ids, num_fts) = target_ids(w);
    if w.environment().is_target_id(-1) && num_fts == 0 {
        return Ok(-1);
    }
    if num_fts == 0 {
        return Err(unsupported::<H>(
            "folding a forage target with no resource targets (the C++ divides by zero)",
            method,
        ));
    }
    // `abs(prop_target) % num_fts` is compiled as an unsigned remainder
    // (`udiv`), so INT_MIN folds as 2^31.
    let ft_num = (prop_target.unsigned_abs() % num_fts.unsigned_abs()) as i32;
    nth_target::<H>(&ids, ft_num, method)
}

// ----------------------------------------------------------------------
// State grid
// ----------------------------------------------------------------------

/// The organism's state grid (checked: the C++ indexes without checks).
fn state_grid<H: GeneHw>(
    hw: &H,
    org: &OrgState,
    w: &dyn OrgInterface,
    method: &str,
) -> Result<avida_environment::StateGrid, String> {
    let n = w.environment().num_state_grids();
    let sg = org.cur_sg;
    if hw.base().ext_mem.len() < 3 || sg < 0 || sg >= n {
        return Err(unsupported::<H>(
            "a state-grid instruction without a state grid (undefined in C++)",
            method,
        ));
    }
    Ok(w.environment().state_grid(sg as usize).clone())
}

/// `sg.GetStateAt(x, y)` with the position checked.
fn sg_state<H: GeneHw>(
    sg: &avida_environment::StateGrid,
    x: i32,
    y: i32,
    method: &str,
) -> Result<i32, String> {
    if x < 0 || y < 0 || x >= sg.width() || y >= sg.height() {
        return Err(unsupported::<H>(
            &format!("state-grid position ({x}, {y}) off the grid (undefined in C++)"),
            method,
        ));
    }
    Ok(sg.state_at(x, y))
}

/// `Inst_SGMove`: step on the toroidal state grid, count the state reached
/// and push the position onto the movement history.
fn inst_sg_move<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_SGMove";
    let sg = state_grid(hw, org, w, M)?;
    let (width, height) = (sg.width(), sg.height());
    let mem = &mut hw.base_mut().ext_mem;
    let (mut x, mut y) = (mem[0], mem[1]);
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
    match mem[2] {
        0 => inc(&mut y, height),
        1 => {
            inc(&mut x, width);
            inc(&mut y, height);
        }
        2 => inc(&mut x, width),
        3 => {
            inc(&mut x, width);
            dec(&mut y, height);
        }
        4 => dec(&mut y, height),
        5 => {
            dec(&mut x, width);
            dec(&mut y, height);
        }
        6 => dec(&mut x, width),
        7 => {
            dec(&mut x, width);
            inc(&mut y, height);
        }
        _ => {}
    }
    mem[0] = x;
    mem[1] = y;
    let state = sg_state::<H>(&sg, x, y, M)?;
    let mem = &mut hw.base_mut().ext_mem;
    match mem.get_mut(3usize.wrapping_add(state as usize)) {
        Some(v) => *v = v.wrapping_add(1),
        None => {
            return Err(unsupported::<H>(
                "state count past the extended memory (undefined in C++)",
                M,
            ))
        }
    }
    mem.push(sg.id_for(x, y));
    Ok(true)
}

/// `Inst_SGRotateL` (`dir` -1) and `Inst_SGRotateR` (`dir` 1).
fn sg_rotate<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, dir: i32) -> R {
    if hw.base().ext_mem.len() < 3 {
        let m = if dir < 0 {
            "Inst_SGRotateL"
        } else {
            "Inst_SGRotateR"
        };
        state_grid(hw, org, w, m)?;
    }
    let f = &mut hw.base_mut().ext_mem[2];
    *f = f.wrapping_add(dir);
    if dir < 0 && *f < 0 {
        *f = 7;
    } else if dir > 0 && *f > 7 {
        *f = 0;
    }
    Ok(true)
}

/// `Inst_SGSense`: ?BX? = the sense value of the current state-grid cell.
fn inst_sg_sense<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_SGSense";
    let sg = state_grid(hw, org, w, M)?;
    let reg_used = hw.core_mut().fmr(R_BX);
    let (x, y) = (hw.base().ext_mem[0], hw.base().ext_mem[1]);
    let state = sg_state::<H>(&sg, x, y, M)?;
    let v = usize::try_from(state)
        .ok()
        .and_then(|s| sg.sense_values().get(s).copied())
        .ok_or_else(|| {
            unsupported::<H>(
                &format!("state {state} without a sense value (undefined in C++)"),
                M,
            )
        })?;
    hw.core_mut().set_reg(reg_used, v, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Movement
// ----------------------------------------------------------------------

/// `cOrganism::Move(ctx)` (`InterruptThread` is `cHardwareBase`'s, which
/// does nothing). Also used by the GP8's queued move.
pub(crate) fn org_move<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
) -> Result<bool, String> {
    if org.is_dead {
        return Ok(false);
    }
    let here = w.get_cell_data();
    let faced = w.get_faced_cell_data();
    org.gradient_movement = if here < faced {
        1.0
    } else if here == faced {
        0.0
    } else {
        -1.0
    };
    let fromcell_id = w.get_cell_id();
    let destcell_id = w.get_faced_cell_id();
    let facing = w.get_faced_dir();
    if !w.move_org(org, fromcell_id, destcell_id) {
        return Ok(false);
    }
    let err = w.config().step_counting_error;
    if err == 0 || w.rng().get_int_range(0, 101) > err {
        count_step(org, facing);
    }
    if org.phenotype.to_delete {
        return Ok(false);
    }
    w.stats_move(org);

    let cfg = w.config();
    if cfg.pheromone_enabled && org.pher_drop {
        let pher_amount = cfg.pheromone_amount;
        let drop_mode = cfg.pheromone_drop_mode;
        let deme_id = w.get_deme_id();
        match drop_mode {
            0 => {
                w.deme_add_pheromone(deme_id, fromcell_id, pher_amount / 2.0);
                w.deme_add_pheromone(deme_id, destcell_id, pher_amount / 2.0);
            }
            1 => w.deme_add_pheromone(deme_id, fromcell_id, pher_amount),
            2 => w.deme_add_pheromone(deme_id, destcell_id, pher_amount),
            _ => {}
        }
    }
    let has_move_task = {
        let env = w.environment();
        let tl = env.tasklib();
        (0..env.num_tasks())
            .any(|i| MOVE_TASK_DESCS.contains(&avida_environment::TaskRegistry::task_desc(tl, i)))
    };
    if has_move_task {
        org_do_output(hw, org, w, None, false)?;
    }
    Ok(true)
}

/// `Move` or `MoveAV` by `m_use_avatar`.
pub(crate) fn move_any<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
) -> Result<bool, String> {
    if hw.core().use_avatar == 0 {
        org_move(hw, org, w)
    } else {
        Ok(org_move_av(org, w))
    }
}

/// `Inst_Move` / `Inst_JuvMove` (BCR): ?BX? = whether the move worked. Fails
/// in the test CPU, and for `juv` before `JUV_PERIOD`.
fn inst_move<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, juv: bool) -> R {
    if w.get_cell_id() == -1 {
        return Ok(false);
    }
    if juv && org.phenotype.time_used < w.config().juv_period {
        return Ok(false);
    }
    let ok = move_any(hw, org, w)?;
    let c = hw.core_mut();
    let out_reg = c.fmr(R_BX);
    c.set_reg(out_reg, i32::from(ok), true);
    Ok(true)
}

// ----------------------------------------------------------------------
// Rotation
// ----------------------------------------------------------------------

/// `avd_cpu_gradient_facing(northerly, easterly)`: the direction back to
/// the birth (or last zeroed) position, -1 for the zero vector.
pub(crate) fn home_facing(northerly: i32, easterly: i32) -> i32 {
    match (northerly.signum(), easterly.signum()) {
        (1, 0) => 0,
        (1, -1) => 1,
        (0, -1) => 2,
        (-1, -1) => 3,
        (-1, 0) => 4,
        (-1, 1) => 5,
        (0, 1) => 6,
        (1, 1) => 7,
        _ => -1,
    }
}

/// Rotate right up to `rotates` times until facing `correct_facing`.
pub(crate) fn rotate_until<H: GeneHw>(hw: &H, w: W<'_>, rotates: i32, correct_facing: i32) {
    let ua = hw.core().use_avatar;
    for _ in 0..rotates {
        w.rotate(1);
        if (ua == 0 && w.get_faced_dir() == correct_facing)
            || (ua != 0 && w.get_av_facing(0) == correct_facing)
        {
            break;
        }
    }
}

/// `Inst_RotateHome` (BCR): face the birth cell (north for the zero
/// vector); the avatar neighbourhood counts only with `USE_AVATARS` 2.
fn inst_rotate_home<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let correct_facing = home_facing(org.northerly, org.easterly).max(0);
    let mut rotates = w.get_num_neighbors();
    if hw.core().use_avatar == 2 {
        rotates = w.get_av_num_neighbors(0);
    }
    rotate_until(hw, w, rotates, correct_facing);
    Ok(true)
}

/// `Inst_RotateUnoccupiedCell`: turn to an empty neighbour; ?BX? = whether
/// one was found.
fn inst_rotate_unoccupied_cell<H: GeneHw>(hw: &mut H, _org: &mut OrgState, w: W<'_>) -> R {
    let ua = hw.core().use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let reg_used = hw.core_mut().fmr(R_BX);
    let n = num_neighbors(hw, w);
    for _ in 0..n {
        if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_av(0)) {
            hw.core_mut().set_reg(reg_used, 1, true);
            return Ok(true);
        }
        w.rotate(1);
    }
    hw.core_mut().set_reg(reg_used, 0, true);
    Ok(true)
}

/// `Inst_RotateX` (BCR): turn ?BX? steps (sign = direction, modulo 8);
/// ?BX? = the turn made. Fails without neighbours.
fn inst_rotate_x<H: GeneHw>(hw: &mut H, _org: &mut OrgState, w: W<'_>) -> R {
    if num_neighbors(hw, w) == 0 {
        return Ok(false);
    }
    let reg_used = hw.core_mut().fmr(R_BX);
    let raw = hw.core().reg(reg_used);
    let rot_dir = if raw < 0 { -1 } else { 1 };
    // `abs(rot_num)`, then `% 8` above 7: the reference binary computes
    // `abs & 7`, so INT_MIN turns 0 steps and writes 0.
    let rot_num = (raw.unsigned_abs() & 7) as i32;
    for _ in 0..rot_num {
        w.rotate(rot_dir);
    }
    hw.core_mut()
        .set_reg(reg_used, rot_num.wrapping_mul(rot_dir), true);
    Ok(true)
}

/// `Inst_RotateOrgID` / `Inst_RotateAwayOrgID`: turn toward (away from) the
/// living organism whose id is in ?BX? when it is within `LOOK_DIST`, and
/// report it as the look sensor does. Fails for an invalid id, for itself
/// and for an id that is not alive (always, in the test CPU). The BCR and
/// GP8 have no torus correction (the experimental hardware does).
fn rotate_org_id<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, away: bool) -> R {
    let method = if away {
        "Inst_RotateAwayOrgID"
    } else {
        "Inst_RotateOrgID"
    };
    let ua = hw.core().use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let id_sought_reg = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
    let id_sought = hw.core().reg_ctx(w, id_sought_reg);
    let worldx = w.config().world_x;
    let worldy = w.config().world_y;
    let long_axis = (f64::from(worldx.max(worldy)) * 0.5 + 0.5) as i32;
    let max_dist = match w.config().look_dist {
        -1 => long_axis,
        d => d,
    };
    if id_sought < 0 || id_sought == org.id {
        return Ok(false);
    }
    let Some(target_uid) = w.find_live_org(id_sought) else {
        return Ok(false);
    };
    let Some(target) = w.seen_org(target_uid) else {
        return Err(unsupported::<H>(
            "a live organism id with no organism in the population (an invariant; unreachable)",
            method,
        ));
    };
    let mut target_org_cell = target.cell_id;
    let mut searching_org_cell = w.get_cell_id();
    if ua == 2 {
        target_org_cell = target.av_cell_id;
        searching_org_cell = w.get_av_cell_id(0);
        if target_org_cell == searching_org_cell {
            return Ok(true);
        }
    }
    let x_dist = target_org_cell % worldx - searching_org_cell % worldx;
    let y_dist = target_org_cell / worldx - searching_org_cell / worldx;
    let travel_dist = x_dist.abs().max(y_dist.abs());
    if travel_dist > max_dist {
        return Ok(false);
    }
    let toward = match (y_dist.signum(), x_dist.signum()) {
        (-1, 0) => 0,
        (-1, 1) => 1,
        (0, 1) => 2,
        (1, 1) => 3,
        (1, 0) => 4,
        (1, -1) => 5,
        (0, -1) => 6,
        (-1, -1) => 7,
        _ => -1,
    };
    let correct_facing = match (toward, away) {
        (-1, _) => 0,
        (t, false) => t,
        (t, true) => (t + 4) % 8,
    };
    let mut found_org = false;
    if ua == 2 {
        w.set_av_facing(correct_facing, 0);
        found_org = true;
    } else {
        let rotates = w.get_num_neighbors();
        for _ in 0..rotates {
            w.rotate(-1);
            if w.get_faced_dir() == correct_facing {
                found_org = true;
                break;
            }
        }
    }
    if found_org {
        let c = hw.core_mut();
        let dist_reg = c.fmnr(id_sought_reg.max(0) as usize);
        let dir_reg = c.fmnr(dist_reg);
        let fat_reg = c.fmnr(dir_reg);
        let ft_reg = c.fmnr(fat_reg);
        let group_reg = c.fmnr(ft_reg);
        c.set_reg(dist_reg, -2, true);
        let (target_facing, org_facing) = if c.sensor.uses_avatar() {
            (target.av_facing, w.get_av_facing(0))
        } else {
            (target.faced_dir, w.get_faced_dir())
        };
        let c = hw.core_mut();
        let rel = c.sensor.return_relative_facing(target_facing, org_facing);
        c.set_reg(dir_reg, rel, true);
        c.set_reg(fat_reg, target.cur_bonus as i32, true);
        c.set_reg(ft_reg, target.forage_target, true);
        if let Some(op) = target.opinion {
            c.set_reg(group_reg, op, true);
        }
        if target.is_displaying || w.config().use_display != 0 {
            if let Some(display) = target.display {
                hw.core_mut().sensor.set_last_seen_display(&display);
            }
        }
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Sensing
// ----------------------------------------------------------------------

/// `Inst_SenseResourceID`: ?BX? = the id of the most plentiful resource
/// here (unchanged when none is above 0).
fn inst_sense_resource_id<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let cell_res = here_resources(hw, org, w);
    let c = hw.core_mut();
    let reg_to_set = c.fmr(R_BX);
    let mut max_resource = 0.0;
    for (i, &v) in cell_res.iter().enumerate() {
        if v > max_resource {
            max_resource = v;
            c.set_reg(reg_to_set, i as i32, true);
        }
    }
    Ok(true)
}

/// `Inst_SenseNest`: ?BX? = the nest asked for (or the first one here at
/// level 1 or more), the next register = its level.
fn inst_sense_nest<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_SenseNest";
    let cell_res = here_resources(hw, org, w);
    let info = res_info(w);
    let size = info.len() as i32;
    let is_nest = |i: usize| info.get(i).is_some_and(|r| r.0 == 3 || r.0 == 4);
    let reg_used = hw.core_mut().fmr(R_BX);
    let mut nest_id = hw.core().reg(reg_used);
    let mut nest_val = 0;
    if nest_id < 0 || nest_id >= size || !is_nest(nest_id as usize) {
        for (i, &v) in cell_res.iter().enumerate() {
            if i >= info.len() {
                return Err(unsupported::<H>(
                    "more cell resources than library resources (undefined in C++)",
                    M,
                ));
            }
            if is_nest(i) && v >= 1.0 {
                nest_id = i as i32;
                nest_val = v as i32;
                break;
            }
        }
    } else {
        nest_val = at::<H>(&cell_res, nest_id, M)? as i32;
    }
    let c = hw.core_mut();
    c.set_reg(reg_used, nest_id, true);
    let val_reg = c.fmnr(reg_used);
    c.set_reg(val_reg, nest_val, true);
    Ok(true)
}

/// `Inst_SenseFacedHabitat`: ?BX? = 3 for a simulated predator, 2 for a
/// barrier, 1 for a hill, else 0. QUIRK (BCR, GP8): despite the name, the
/// resources read are those of the organism's own cell.
fn inst_sense_faced_habitat<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let reg_to_set = hw.core_mut().fmr(R_BX);
    let info = res_info(w);
    let cell_res = here_resources(hw, org, w);
    if cell_res.len() > info.len() {
        return Err(unsupported::<H>(
            "more cell resources than library resources (undefined in C++)",
            "Inst_SenseFacedHabitat",
        ));
    }
    for (habitat, value) in [(5, 3), (2, 2), (1, 1)] {
        if cell_res
            .iter()
            .zip(&info)
            .any(|(&v, &(h, _, _))| h == habitat && v > 0.0)
        {
            hw.core_mut().set_reg(reg_to_set, value, true);
            return Ok(true);
        }
    }
    hw.core_mut().set_reg(reg_to_set, 0, true);
    Ok(true)
}

// ----------------------------------------------------------------------
// Looking
// ----------------------------------------------------------------------

/// `sLookRegAssign`
struct LookRegs {
    habitat: usize,
    distance: usize,
    search_type: usize,
    id_sought: usize,
    count: usize,
    value: usize,
    group: usize,
    ft: usize,
}

/// Whether `LOOK_DISABLE_TYPE` selects this organism.
fn look_disable_target(org: &OrgState, w: &dyn OrgInterface) -> bool {
    let org_type = w.config().look_disable_type;
    (org_type == 0 && !is_prey_ft(org)) || (org_type == 1 && is_prey_ft(org)) || org_type == 2
}

/// A random value with a random sign (the `LOOK_DISABLE` overrides).
fn rand_signed(w: W<'_>) -> i32 {
    let randsign = if w.rng().get_uint_range(0, 2) != 0 {
        -1
    } else {
        1
    };
    w.rng().get_int(i32::MAX).wrapping_mul(randsign)
}

/// `TRACK_LOOK_SETTINGS`: the look settings line.
fn track_look_settings(org: &OrgState, w: W<'_>, r: &LookOut) {
    if w.config().track_look_settings != 0 {
        let s = format!(
            "{},{},{},{},{},{}",
            org.forage_target, r.report_type, r.habitat, r.distance, r.search_type, r.id_sought
        );
        w.try_write_look_data(&s);
    }
}

/// `Inst_LookAhead` (`use_ft` false) and `Inst_LookFT` (true).
fn look_ahead<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, use_ft: bool) -> R {
    let c = cell(hw, w);
    let f = facing(hw, w);
    go_look(hw, org, w, f, c, use_ft)
}

/// `Inst_LookAround` / `Inst_LookAroundFT`: look ahead, left or right by
/// the fifth modified register.
fn look_around<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, use_ft: bool) -> R {
    let c = hw.core_mut();
    let hab_reg = c.fmr(R_BX);
    let dist_reg = c.fmnr(hab_reg);
    let st_reg = c.fmnr(dist_reg);
    let id_reg = c.fmnr(st_reg);
    let dir_reg = c.fmnr(id_reg);
    // `abs(dir) % 3`, compiled as an unsigned remainder (INT_MIN gives 2).
    let mut search_dir = (c.reg(dir_reg).unsigned_abs() % 3) as i32;
    if w.config().look_disable == 5 && look_disable_target(org, w) {
        let rand = w.rng().get_int(i32::MAX);
        search_dir = rand % 3;
    }
    if search_dir == 1 {
        search_dir = -1;
    } else if search_dir == 2 {
        search_dir = 1;
    }
    let mut f = facing(hw, w).wrapping_add(search_dir);
    if f == -1 {
        f = 7;
    } else if f == 9 {
        f = 1;
    } else if f == 8 {
        f = 0;
    }
    let c = cell(hw, w);
    go_look(hw, org, w, f, c, use_ft)
}

/// `GoLook(ctx, look_dir, cell_id, use_ft)`: fails off a bounded grid,
/// without neighbours (always in the test CPU) and for a confused
/// predator; otherwise looks through the sensor.
fn go_look<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
    look_dir: i32,
    cell_id: i32,
    use_ft: bool,
) -> R {
    if w.config().world_geometry != 1 {
        return Ok(false);
    }
    let ua = hw.core().use_avatar;
    if (ua == 0 && w.get_num_neighbors() == 0)
        || (ua != 0 && w.get_av_num_neighbors(0) == 0)
        || (is_pred_ft(org) && w.config().pred_confusion == 4)
    {
        return Ok(false);
    }
    let c = hw.core_mut();
    let habitat = c.fmr(R_BX);
    let distance = c.fmnr(habitat);
    let search_type = c.fmnr(distance);
    let id_sought = c.fmnr(search_type);
    let count = c.fmnr(id_sought);
    let value = c.fmnr(count);
    let group = c.fmnr(value);
    let ft = c.fmnr(group);
    let regs = LookRegs {
        habitat,
        distance,
        search_type,
        id_sought,
        count,
        value,
        group,
        ft,
    };
    let reg_init = LookInit {
        habitat: c.reg(habitat),
        distance: c.reg(distance),
        search_type: c.reg(search_type),
        id_sought: c.reg(id_sought),
    };
    let results = c
        .sensor
        .set_looking(org, w, &reg_init, look_dir, cell_id, use_ft)?;
    look_results(hw, org, w, &regs, &results);
    Ok(true)
}

/// `LookResults(ctx, regs, results)` (BCR: no `LOOK_DISABLE_COMBO`).
fn look_results<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
    regs: &LookRegs,
    results: &LookOut,
) {
    track_look_settings(org, w, results);
    let c = hw.core_mut();
    if results.report_type == 0 {
        c.set_reg(regs.habitat, results.habitat, true);
        c.set_reg(regs.distance, -1, true);
        c.set_reg(regs.search_type, results.search_type, true);
        c.set_reg(regs.id_sought, results.id_sought, true);
        c.set_reg(regs.count, 0, true);
        c.set_reg(regs.value, 0, true);
        c.set_reg(regs.group, -9, true);
        c.set_reg(regs.ft, -9, true);
    } else if results.report_type == 1 {
        c.set_reg(regs.habitat, results.habitat, true);
        c.set_reg(regs.distance, results.distance, true);
        c.set_reg(regs.search_type, results.search_type, true);
        c.set_reg(regs.id_sought, results.id_sought, true);
        c.set_reg(regs.count, results.count, true);
        c.set_reg(regs.value, results.value, true);
        c.set_reg(regs.group, results.group, true);
        c.set_reg(regs.ft, results.forage, true);
    }
    if w.config().look_disable > 5 && look_disable_target(org, w) {
        let rand = rand_signed(w);
        let r = match w.config().look_disable {
            6 => Some(regs.habitat),
            7 => Some(regs.distance),
            8 => Some(regs.search_type),
            9 => Some(regs.id_sought),
            10 => Some(regs.count),
            11 => Some(regs.value),
            12 => Some(regs.group),
            13 => Some(regs.ft),
            _ => None,
        };
        if let Some(r) = r {
            hw.core_mut().set_reg(r, rand, true);
        }
    }
    if w.config().track_look_output != 0 {
        let c = hw.core();
        let vals = [
            regs.habitat,
            regs.distance,
            regs.search_type,
            regs.id_sought,
            regs.count,
            regs.value,
            regs.group,
            regs.ft,
        ]
        .map(|r| c.reg(r).to_string());
        let s = format!(
            "{},{},{}",
            org.forage_target,
            results.report_type,
            vals.join(",")
        );
        w.try_write_look_output(&s);
    }
}

/// The sensor session `reg_session` names: `Abs(value % sessions)`.
fn session_idx<H: GeneHw>(hw: &H, reg_session: i32) -> usize {
    let c = hw.core();
    let len = c.sensor_sessions.len().max(1) as i32;
    (c.reg(reg_session.max(0) as usize) % len).wrapping_abs() as usize
}

/// `DoLookAheadEX(ctx, use_ft[, use_dir])`: look with the registers named
/// by the following nops (habitat, id sought, then travel distance,
/// deviance and count/value in the three registers from ?DX? on, and the
/// optional search distance, search type and found-id registers), saving
/// the settings in the thread's (and an upstream nop's) sensor session.
/// `use_dir` (GP8 `look-around-ex`) turns the look by ?GX?.
fn look_ahead_ex<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
    use_ft: bool,
    use_dir: bool,
) -> R {
    let cell_id = cell(hw, w);
    let mut facing = facing(hw, w);
    if w.config().world_geometry != 1 {
        return Ok(false);
    }
    if num_neighbors(hw, w) == 0 {
        return Ok(false);
    }
    // GP8 only.
    if H::IMM && is_pred_ft(org) && w.config().pred_confusion == 4 {
        return Ok(false);
    }
    let c = hw.core_mut();
    let reg_session = c.find_upstream_modified_register(0, -1);
    let reg_habitat = c.fmr(R_BX);
    let reg_id_sought = c.fmnr(reg_habitat);
    let reg_travel_distance = c.fmnr(reg_id_sought);
    let reg_deviance = c.next_reg(reg_travel_distance);
    let reg_cv = c.next_reg(reg_deviance);
    if use_dir {
        let reg_dir = c.fmr(R_GX);
        // `abs(dir) % 3`, compiled as an unsigned remainder (INT_MIN gives 2).
        let mut search_dir = (c.reg(reg_dir).unsigned_abs() % 3) as i32;
        if search_dir == 1 {
            search_dir = -1;
        } else if search_dir == 2 {
            search_dir = 1;
        }
        facing = facing.wrapping_add(search_dir);
        if facing == -1 {
            facing = 7;
        } else if facing == 9 {
            facing = 1;
        } else if facing == 8 {
            facing = 0;
        }
    }
    let reg_search_distance = c.fmr_ex(-1, H::IMM);
    let reg_search_type = c.fmr_ex(-1, H::IMM);
    let reg_id_found = c.fmr_ex(-1, H::IMM);

    let habitat = c.reg(reg_habitat);
    let distance = if reg_search_distance == -1 {
        i32::MAX
    } else {
        hw.core().reg_ctx(w, reg_search_distance)
    };
    let search_type = if reg_search_type == -1 {
        0
    } else {
        hw.core().reg_ctx(w, reg_search_type)
    };
    let id_sought = hw.core().reg(reg_id_sought);
    let mut look_init = LookInit {
        habitat,
        distance,
        search_type,
        id_sought,
    };
    let results =
        hw.core_mut()
            .sensor
            .set_looking_inout(org, w, &mut look_init, facing, cell_id, use_ft)?;
    track_look_settings(org, w, &results);

    let session = LookInit {
        habitat: results.habitat,
        distance: look_init.distance,
        search_type: results.search_type,
        id_sought: results.id_sought,
    };
    let ct = hw.core().ct();
    hw.core_mut().threads[ct].sensor_session = session;
    if reg_session != -1 {
        let idx = session_idx(hw, reg_session);
        if let Some(s) = hw.core_mut().sensor_sessions.get_mut(idx) {
            *s = session;
        }
    }

    // Registers named by an immediate value (GP8) are not set.
    let opt = |r: i32| usize::try_from(r).ok();
    let c = hw.core_mut();
    if results.report_type == 0 {
        c.set_reg(reg_habitat, results.habitat, true);
        c.set_reg(reg_id_sought, results.id_sought, true);
        c.set_reg(reg_travel_distance, -1, true);
        c.set_reg(reg_deviance, 0, true);
        c.set_reg(reg_cv, 0, true);
        if let Some(r) = opt(reg_search_type) {
            c.set_reg(r, results.search_type, true);
        }
        if let Some(r) = opt(reg_id_found) {
            c.set_reg(r, -9, true);
        }
    } else if results.report_type == 1 {
        c.set_reg(reg_habitat, results.habitat, true);
        c.set_reg(reg_id_sought, results.id_sought, true);
        c.set_reg(reg_travel_distance, results.distance, true);
        c.set_reg(reg_deviance, results.deviance, true);
        let cv = if results.count <= 1 {
            results.value
        } else {
            results.count
        };
        c.set_reg(reg_cv, cv, true);
        if let Some(r) = opt(reg_search_type) {
            c.set_reg(r, results.search_type, true);
        }
        if let Some(r) = opt(reg_id_found) {
            let v = if results.forage == -9 {
                results.group
            } else {
                results.forage
            };
            c.set_reg(r, v, true);
        }
    }
    if w.config().look_disable > 5 && look_disable_target(org, w) {
        let rand = rand_signed(w);
        let r = match w.config().look_disable {
            6 => Some(reg_habitat),
            7 => Some(reg_travel_distance),
            8 => opt(reg_search_type),
            9 => Some(reg_id_sought),
            10 | 11 => Some(reg_cv),
            12 | 13 => opt(reg_id_found),
            _ => None,
        };
        if let Some(r) = r {
            hw.core_mut().set_reg(r, rand, true);
        }
    }
    if w.config().track_look_output != 0 {
        let c = hw.core();
        let s = format!(
            "{},{},{},{},{},{},{}",
            org.forage_target,
            results.report_type,
            c.reg(reg_habitat),
            c.reg(reg_id_sought),
            c.reg(reg_travel_distance),
            c.reg(reg_deviance),
            c.reg(reg_cv)
        );
        w.try_write_look_ex_output(&s);
    }
    Ok(true)
}

/// `DoLookAgainEX(ctx, use_ft)`: look again with the thread's (or an
/// upstream nop's) sensor session; results go to ?DX? and the two
/// registers after it, and the optional found-id register.
fn look_again_ex<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>, use_ft: bool) -> R {
    let cell_id = cell(hw, w);
    let facing = facing(hw, w);
    if w.config().world_geometry != 1 {
        return Ok(false);
    }
    if num_neighbors(hw, w) == 0 {
        return Ok(false);
    }
    let c = hw.core_mut();
    let reg_session = c.find_upstream_modified_register(0, -1);
    let reg_travel_distance = c.fmnr(R_DX);
    let reg_deviance = c.next_reg(reg_travel_distance);
    let reg_cv = c.next_reg(reg_deviance);
    let reg_id_found = c.fmr_ex(-1, false);
    let ct = c.ct();
    let mut look_init = c.threads[ct].sensor_session;
    if reg_session != -1 {
        let idx = session_idx(hw, reg_session);
        if let Some(s) = hw.core().sensor_sessions.get(idx) {
            look_init = *s;
        }
    }
    let results =
        hw.core_mut()
            .sensor
            .set_looking_inout(org, w, &mut look_init, facing, cell_id, use_ft)?;
    track_look_settings(org, w, &results);
    let c = hw.core_mut();
    if results.report_type == 0 {
        c.set_reg(reg_travel_distance, -1, true);
        c.set_reg(reg_deviance, 0, true);
        c.set_reg(reg_cv, 0, true);
        if reg_id_found != -1 {
            c.set_reg(reg_id_found as usize, -9, true);
        }
    } else if results.report_type == 1 {
        c.set_reg(reg_travel_distance, results.distance, true);
        c.set_reg(reg_deviance, results.deviance, true);
        let cv = if results.count <= 1 {
            results.value
        } else {
            results.count
        };
        c.set_reg(reg_cv, cv, true);
        if reg_id_found != -1 {
            let v = if results.forage == -9 {
                results.group
            } else {
                results.forage
            };
            c.set_reg(reg_id_found as usize, v, true);
        }
    }
    if w.config().look_disable > 5 && look_disable_target(org, w) {
        let rand = rand_signed(w);
        let r = match w.config().look_disable {
            7 => Some(reg_travel_distance),
            10 | 11 => Some(reg_cv),
            12 | 13 if reg_id_found != -1 => Some(reg_id_found as usize),
            _ => None,
        };
        if let Some(r) = r {
            hw.core_mut().set_reg(r, rand, true);
        }
    }
    if w.config().track_look_output != 0 {
        let c = hw.core();
        let s = format!(
            "{},{},{},{},{},{},{}",
            org.forage_target,
            results.report_type,
            results.habitat,
            results.id_sought,
            c.reg(reg_travel_distance),
            c.reg(reg_deviance),
            c.reg(reg_cv)
        );
        w.try_write_look_ex_output(&s);
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Forage targets
// ----------------------------------------------------------------------

/// `Inst_SetForageTarget` (BCR): switch to the forage target in ?BX?
/// (folded onto the valid targets); ?BX? = the target set. The avatar
/// lists are switched when crossing between prey and predator; unlike the
/// experimental hardware there is no `MakePred` here.
fn inst_set_forage_target<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_SetForageTarget";
    let r = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
    let mut prop_target = hw.core().reg_ctx(w, r);
    let old_target = org.forage_target;
    if old_target == prop_target {
        return Ok(false);
    }
    let pps = w.config().pred_prey_switch;
    if old_target <= -2 && prop_target > -2 && (pps == 0 || pps == 2) {
        return Ok(false);
    }
    if prop_target <= -2 && pps < 0 {
        return Ok(false);
    }
    if prop_target <= -2 && pps == 2 {
        return Ok(false);
    }
    if !w.environment().is_target_id(prop_target) && prop_target != -2 && prop_target != -3 {
        prop_target = fold_target::<H>(w, prop_target, M)?;
    }
    let pred = |t: i32| t == -2 || t == -3;
    if hw.core().use_avatar != 0
        && ((pred(prop_target) && old_target > -2) || (prop_target > -2 && pred(old_target)))
        && w.get_av_cell_id(0) != -1
    {
        w.switch_pred_prey(0);
    }
    org_set_forage_target(org, w, prop_target, false)?;
    org.has_set_ft = true;
    let c = hw.core_mut();
    let r = c.fmr_ex(R_BX as i32, H::IMM);
    c.set_reg(r.max(0) as usize, prop_target, false);
    Ok(true)
}

/// `Inst_SetRandForageTargetOnce`: with a population cap and the population
/// at half of it or more, half the time pick a random resource target
/// (once); otherwise `Inst_SetForageTargetOnce`.
fn inst_set_rand_forage_target_once<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_SetRandForageTargetOnce";
    let cfg = w.config();
    let cap = if cfg.population_cap != 0 {
        cfg.population_cap
    } else {
        cfg.pop_cap_eldest
    };
    if cap != 0 && w.live_org_ids().len() as f64 >= f64::from(cap) * 0.5 && w.rng().p(0.5) {
        if org.has_set_ft {
            return Ok(false);
        }
        let (_, num_fts) = target_ids(w);
        let mut prop_target = w.rng().get_uint(num_fts as u32) as i32;
        prop_target = fold_target::<H>(w, prop_target, M)?;
        org_set_forage_target(org, w, prop_target, false)?;
        org.has_set_ft = true;
        let c = hw.core_mut();
        let r = c.fmr_ex(R_BX as i32, H::IMM);
        c.set_reg(r.max(0) as usize, prop_target, false);
        return Ok(true);
    }
    if org.has_set_ft {
        return Ok(false);
    }
    inst_set_forage_target(hw, org, w)
}

// ----------------------------------------------------------------------
// Collection
// ----------------------------------------------------------------------

/// `DoActualCollect(ctx, bin_used, false)`: absorb `ABSORB_RESOURCE_
/// FRACTION` of the resource here (up to `MAX_TOTAL_STORED`) into the bin.
fn do_actual_collect<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
    bin_used: i32,
) -> Result<bool, String> {
    const M: &str = "Inst_CollectSpecific";
    let res_count = here_resources(hw, org, w);
    let mut res_change = vec![0.0; res_count.len()];
    let total = org.rbins_total();
    let max = w.config().max_total_stored;
    let has_max = max > 0.0;
    let mut res_consumed = at::<H>(&res_count, bin_used, M)? * w.config().absorb_resource_fraction;
    if has_max && res_consumed + total >= max {
        res_consumed = max - total;
    }
    res_change[bin_used as usize] = -res_consumed;
    if res_consumed > 0.0 {
        org.add_to_rbin(bin_used, res_consumed);
        if hw.core().use_avatar != 0 {
            w.update_av_resources(org, &res_change, 0);
        } else {
            w.update_resources(org, &res_change);
        }
        return Ok(true);
    }
    Ok(false)
}

/// `Inst_CollectSpecific`: collect `COLLECT_SPECIFIC_RESOURCE`; ?BX? = the
/// amount gained, the next register = the bin's content.
fn inst_collect_specific<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let resource = w.config().collect_specific_resource;
    let res_before = org.rbin(resource);
    let success = do_actual_collect(hw, org, w, resource)?;
    let res_after = org.rbin(resource);
    let c = hw.core_mut();
    let out_reg = c.fmr(R_BX);
    c.set_reg(out_reg, (res_after - res_before) as i32, true);
    let r2 = c.fmnr_ex(out_reg as i32, H::IMM);
    c.set_reg(r2.max(0) as usize, res_after as i32, true);
    Ok(success)
}

/// `Inst_GetResStored`: ?BX? = the content of bin |?BX?| (modulo the bins).
fn inst_get_res_stored<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_GetResStored";
    let r = hw.core_mut().fmr_ex(R_BX as i32, H::IMM);
    let mut resource_id = hw.core().reg_ctx(w, r).wrapping_abs();
    let bins = org.rbins().to_vec();
    if bins.is_empty() {
        return Err(unsupported::<H>(
            "no resource bins (the C++ divides by zero)",
            M,
        ));
    }
    resource_id %= bins.len() as i32;
    let v = at::<H>(&bins, resource_id, M)?;
    let c = hw.core_mut();
    let out_reg = c.fmr(R_BX);
    c.set_reg(out_reg, v as i32, true);
    Ok(true)
}

// ----------------------------------------------------------------------
// Groups, teaching, displays
// ----------------------------------------------------------------------

/// `Inst_JoinGroup`: join the group in ?BX?.
fn inst_join_group<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let c = hw.core_mut();
    let r = c.fmr(R_BX);
    let prop_group_id = c.reg(r);
    if w.config().use_form_groups == 2 && !w.environment().is_group_id(prop_group_id) {
        return Ok(false);
    }
    if w.has_opinion(org) {
        let opinion = org.get_opinion().0;
        if opinion == prop_group_id {
            return Ok(false);
        }
        let percent_failure = w.config().join_group_failure;
        if percent_failure > 0 {
            let prob_failure = f64::from(percent_failure) / 100.0;
            let rand = w.rng().get_double();
            if rand <= prob_failure {
                return Ok(true);
            }
        }
        if w.config().tolerance_window > 0 {
            w.attempt_immigrate_group(prop_group_id, org);
            return Ok(true);
        }
        w.leave_group(org, opinion);
    }
    w.set_opinion(prop_group_id, org);
    if w.has_opinion(org) {
        let opinion = org.get_opinion().0;
        w.join_group(org, opinion);
    }
    Ok(true)
}

/// `Inst_GetFacedOrgID`: ?BX? = the id of the faced organism (a random
/// faced avatar's with avatars; the organism itself when it faces its own
/// cell or avatar); fails for an empty or dead one.
fn inst_get_faced_org_id<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let ua = hw.core().use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_av(0)) {
        return Ok(false);
    }
    let Some((dead, id)) = faced_org_info(org, w, ua) else {
        return Err(unsupported::<H>(
            "no organism behind an occupied faced cell or avatar (null in C++)",
            "Inst_GetFacedOrgID",
        ));
    };
    if dead {
        return Ok(false);
    }
    let c = hw.core_mut();
    let out_reg = c.fmr(R_BX);
    c.set_reg(out_reg, id, true);
    Ok(true)
}

/// `Inst_LearnParent` (BCR): copy a teaching parent's forage target.
/// QUIRK: the avatar-list switch tests `prop_target == -3` where
/// `old_target == -3` was meant, so a top predator taking a prey target
/// keeps its predator avatar.
fn inst_learn_parent<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    let mut halt = false;
    if org.parent_teacher {
        let old_target = org.forage_target;
        let prop_target = org.parent_ft;
        halt = prop_target <= -2 && w.config().pred_prey_switch < 0;
        if !halt {
            if hw.core().use_avatar != 0
                && w.get_av_cell_id(0) != -1
                && (((prop_target == -2 || prop_target == -3) && old_target > -2)
                    || (prop_target > -2 && (old_target == -2 || prop_target == -3)))
            {
                w.switch_pred_prey(0);
            }
            copy_parent_ft(org, w)?;
        }
    }
    Ok(!halt)
}

/// `Inst_ModifySimpDisplay`: for up to four following nops, write AX, BX,
/// CX and the nop's register into the display from the part the nop
/// names on (the C++ `switch` falls through; the nop's register is
/// written once).
fn inst_modify_simp_display<H: GeneHw>(hw: &mut H, org: &mut OrgState, _w: W<'_>) -> R {
    let mut message_used = false;
    for _ in 0..4 {
        let c = hw.core_mut();
        let ip = c.ip();
        if !c.is_nop(c.h_next_inst(&ip)) {
            break;
        }
        c.ip_advance();
        let this_nop = c.nop_mod(c.h_inst(&c.ip()));
        if this_nop == 0 {
            set_simp_display(org, 0, c.reg(R_AX));
        }
        if this_nop <= 1 {
            set_simp_display(org, 1, c.reg(R_BX));
        }
        if this_nop <= 2 {
            set_simp_display(org, 2, c.reg(R_CX));
        }
        if !message_used {
            set_simp_display(org, 3, c.reg(this_nop.max(0) as usize));
        }
        message_used = true;
    }
    Ok(true)
}

/// `Inst_ReadLastSimpDisplay`: read the last display seen into AX..DX (all
/// four when no nop follows), or from the part each following nop names
/// on (the `switch` falls through).
fn inst_read_last_simp_display<H: GeneHw>(hw: &mut H, _org: &mut OrgState, _w: W<'_>) -> R {
    let c = hw.core_mut();
    if !c.sensor.has_seen_display() {
        return Ok(false);
    }
    let last_seen = *c.sensor.last_seen_display();
    let mut message_read = false;
    for i in 0..4 {
        let ip = c.ip();
        if c.is_nop(c.h_next_inst(&ip)) {
            c.ip_advance();
            let this_nop = c.nop_mod(c.h_inst(&c.ip()));
            if this_nop == 0 {
                c.set_reg(R_AX, last_seen.distance, true);
            }
            if this_nop <= 1 {
                c.set_reg(R_BX, last_seen.direction, true);
            }
            if this_nop <= 2 {
                c.set_reg(R_CX, last_seen.value, true);
            }
            if !message_read {
                c.set_reg(this_nop.max(0) as usize, last_seen.message, true);
            }
            message_read = true;
        } else if i == 0 {
            c.set_reg(R_AX, last_seen.distance, true);
            c.set_reg(R_BX, last_seen.direction, true);
            c.set_reg(R_CX, last_seen.value, true);
            c.set_reg(R_DX, last_seen.message, true);
            break;
        } else {
            break;
        }
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Attacks
// ----------------------------------------------------------------------

/// `sAttackReg` / `AttackRegisters`.
#[derive(Clone, Copy)]
struct AttackReg {
    success_reg: usize,
    bonus_reg: usize,
    bin_reg: usize,
}

/// `setAttackReg(reg)`
fn set_attack_reg<H: GeneHw>(hw: &mut H) -> AttackReg {
    let c = hw.core_mut();
    let success_reg = c.fmr(R_BX);
    let bonus_reg = c.fmnr(success_reg);
    let bin_reg = c.fmnr(bonus_reg);
    AttackReg {
        success_reg,
        bonus_reg,
        bin_reg,
    }
}

/// `testAttack(ctx)` (BCR): prey in front, not on a refuge (a refuge
/// resource at its threshold or more in the faced cell).
fn test_attack<H: GeneHw>(hw: &H, org: &mut OrgState, w: W<'_>) -> bool {
    let ua = hw.core().use_avatar;
    if ua != 0 && ua != 2 {
        return false;
    }
    if w.config().pred_prey_switch < 0 {
        return false;
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_prey_av(0)) {
        return false;
    }
    let info = res_info(w);
    for (i, &(_, threshold, refuge)) in info.iter().enumerate() {
        if refuge != 0 {
            if ua == 0 && w.get_faced_resource_val(org, i as i32) >= threshold {
                return false;
            }
            if ua == 2 && w.get_av_faced_resource_val(org, i as i32, 0) >= threshold {
                return false;
            }
        }
    }
    true
}

/// The faced organism (`GetNeighbor()`) as a victim: the attacker itself
/// when it faces its own cell.
fn neighbor_victim(org: &OrgState, w: W<'_>) -> Option<Victim> {
    let mut uid = None;
    let uid = match w.with_neighbor(&mut |n| uid = Some(n.state.uid)) {
        Faced::Empty => return None,
        Faced::SelfOrg => org.uid,
        Faced::Other => uid?,
    };
    victim(org, w, uid)
}

/// `Inst_AttackPrey`: kill the faced prey (a random faced prey avatar with
/// avatars) and take its merit, reactions, bins and bonus.
fn inst_attack_prey<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPrey";
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let target = if hw.core().use_avatar == 0 {
        neighbor_victim(org, w)
    } else {
        w.get_rand_faced_prey_av(0)
            .and_then(|uid| victim(org, w, uid))
    };
    let Some(target) = target else {
        return Err(unsupported::<H>(
            "an attack target that is NULL (the C++ dereferences it)",
            M,
        ));
    };
    if !test_prey_target(&target) {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    execute_attack(hw, org, w, &target, reg, M)
}

/// `Inst_AttackFTPrey`: `Inst_AttackPrey` on faced prey whose forage target
/// is ?BX?. QUIRK: with avatars the C++ loop stops in its first pass, so
/// only the faced prey avatar at the random index is tried.
fn inst_attack_ft_prey<H: GeneHw>(hw: &mut H, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackFTPrey";
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let c = hw.core_mut();
    let target_reg = c.fmr(R_BX);
    let target_org_type = c.reg(target_reg);
    let target = if hw.core().use_avatar == 0 {
        let Some(t) = neighbor_victim(org, w) else {
            return Err(unsupported::<H>(
                "an attack target that is NULL (the C++ dereferences it)",
                M,
            ));
        };
        if t.forage_target != target_org_type || !is_prey(t.forage_target) {
            return Ok(false);
        }
        t
    } else {
        let av_neighbors = w.get_faced_prey_avs(0);
        let rand_index = w.rng().get_uint_range(0, av_neighbors.len() as u32) as usize;
        let Some(&uid) = av_neighbors.get(rand_index) else {
            return Ok(false);
        };
        let Some(t) = victim(org, w, uid) else {
            return Err(unsupported::<H>(
                "a faced prey avatar without a living organism (C++: dangling cOrganism*)",
                M,
            ));
        };
        if t.forage_target != target_org_type {
            return Ok(false);
        }
        t
    };
    if !test_prey_target(&target) {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    execute_attack(hw, org, w, &target, reg, M)
}

/// `cOrganism::IsPreyFT()` of a forage target.
fn is_prey(ft: i32) -> bool {
    ft > -2
}

/// `executeAttack(ctx, target, reg, -1)`.
fn execute_attack<H: GeneHw>(
    hw: &mut H,
    org: &mut OrgState,
    w: W<'_>,
    target: &Victim,
    reg: AttackReg,
    method: &str,
) -> R {
    // testAttackChance
    let odds = w.config().pred_odds;
    let min_prey = w.config().min_prey;
    let draw = w.rng().get_double();
    if draw >= odds || (min_prey > 0 && w.stats_num_prey_creatures()? <= min_prey) {
        injure_org(org, w, target, method)?;
        let c = hw.core_mut();
        c.set_reg(reg.success_reg, -1, true);
        c.set_reg(reg.bonus_reg, -1, true);
        if w.config().use_resource_bins {
            hw.core_mut().set_reg(reg.bin_reg, -1, true);
        }
        return Ok(false);
    }
    let mut effic = w.config().pred_efficiency;
    if org.forage_target < -2 {
        effic *= effic;
    }
    // applyKilledPreyMerit (fused, as the reference build compiles it)
    if w.config().merit_inc_apply_immediate {
        let attacker_merit = target
            .merit
            .mul_add(effic, org.phenotype.merit.get_double());
        if attacker_merit <= 0.0 {
            return Err(unsupported::<H>(
                "a merit of zero or less after an attack (C++: cPopulation::UpdateMerit \
                 kills the organism, then dereferences its empty cell)",
                method,
            ));
        }
        w.update_merit(org, attacker_merit);
    }
    // applyKilledPreyReactions
    for (i, s) in org
        .phenotype
        .cur_stolen_reaction_count
        .iter_mut()
        .enumerate()
    {
        *s = s.wrapping_add(target.last_reaction_count.get(i).copied().unwrap_or(0));
    }
    // applyKilledPreyResBins
    if w.config().use_resource_bins {
        for (i, b) in target.rbins.iter().enumerate() {
            org.add_to_rbin(i as i32, b * effic);
        }
        if effic > 0.0 {
            let bins = target.rbins.clone();
            with_victim(org, w, target, &mut |st: &mut OrgState| {
                for (i, b) in bins.iter().enumerate() {
                    st.add_to_rbin(i as i32, -(b * effic));
                }
            });
        }
        let spec = w.config().collect_specific_resource;
        let spec_bin = at::<H>(org.rbins(), spec, method)? as i32;
        hw.core_mut().set_reg(reg.bin_reg, spec_bin, true);
    }
    hw.core_mut().set_reg(reg.success_reg, 1, true);
    // applyKilledPreyBonus
    org.phenotype.cur_bonus = target.cur_bonus.mul_add(effic, org.phenotype.cur_bonus);
    hw.core_mut()
        .set_reg(reg.bonus_reg, target.cur_bonus as i32, true);

    // `target->Die(ctx)`: "kill first" (the attacker itself when it faces
    // its own cell; it then goes on, dead, as in the C++).
    victim_die(org, w, target);
    make_pred(hw, org, w)?;
    try_prey_clone(org, w)?;
    Ok(true)
}

/// `makePred(ctx)`: the `MAX_PRED` cull counts predators only
/// (`GetNumPredCreatures`, not the total with top predators).
fn make_pred<H: GeneHw>(hw: &H, org: &mut OrgState, w: W<'_>) -> Result<(), String> {
    if is_prey_ft(org) {
        let max_pred = w.config().max_pred;
        if max_pred != 0 && w.stats_num_pred_creatures()? >= max_pred {
            w.kill_rand_pred(org);
        }
        if hw.core().use_avatar != 0 && w.get_av_cell_id(0) != -1 {
            w.switch_pred_prey(0);
        }
        org_set_forage_target(org, w, -2, false)?;
    }
    Ok(())
}
