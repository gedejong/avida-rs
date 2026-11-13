//! The environment `cHardwareExperimental::Inst_*` handlers
//! (avida-core/source/cpu/cHardwareExperimental.cc at 47f13dadb, lines
//! 3055-6912 plus the attack helpers after 6913): state grids, movement and
//! rotation, avatars, resource sensing, looking, forage targets, resource
//! collection and deposits, cell data, opinions and messages, groups,
//! attacks and fights, cell marking, teaching, guards, displays and
//! tolerances; and the `cOrganism` members they call that the heads CPU
//! port does not share (`Move` without interrupts, `MoveAV`,
//! `SetForageTarget`, `CopyParentFT`, `SetSimpDisplay`).
//!
//! The remaining `Err("unsupported: ...")` returns mark paths the C++
//! leaves undefined: an out-of-range resource, bin or state index, a
//! division by zero, a NULL organism or cell -1 dereferenced, a merit of 0
//! handed to `UpdateMerit` (which kills the organism and then reads its
//! emptied cell). One is a port limit: under `MAX_PREY_BT` a test CPU
//! reads the population's live organisms as the world gathered them for
//! the test, but cannot kill one of them, which the C++ `Die` does from
//! inside the test run. `MULTI_ABSORB_TYPE` values other than 1 and 2
//! print the C++ driver error and fail the instruction, as the C++ does.

use crate::hardware_cpu_insts_move::MOVE_TASK_DESCS;
use crate::hardware_cpu_insts_msg::{
    deliver_to_self, message_sent, new_message, org_broadcast_message, org_retrieve_message,
};
use crate::hardware_experimental::{HardwareExperimental, R_AX, R_BX, R_CX, R_DX};
use crate::hardware_experimental_insts::{mark_from, ExpInstFn, R};
use crate::nhardware::{MATING_TYPE_FEMALE, MATING_TYPE_MALE};
use crate::org_interface::{with_neighbor_state, OrgInterface, OrgMessage};
use crate::org_sensor::{LookInit, LookOut, OrgDisplay};
use crate::organism::{OrgState, OrgUid, Organism};
use avida_genome::InstructionSequence;

type Hw = HardwareExperimental;
type W<'a> = &'a mut dyn OrgInterface;

/// The environment handler for `method`, `None` when it is not one.
pub(crate) fn handler(method: &str) -> Option<ExpInstFn> {
    let f: ExpInstFn = match method {
        "Inst_SGMove" => inst_sg_move,
        "Inst_SGRotateL" => |hw, org, w| sg_rotate(hw, org, w, -1),
        "Inst_SGRotateR" => |hw, org, w| sg_rotate(hw, org, w, 1),
        "Inst_SGSense" => inst_sg_sense,
        "Inst_Move" => inst_move,
        "Inst_JuvMove" => inst_juv_move,
        "Inst_RangeMove" => |hw, org, w| range_move(hw, org, w, false),
        "Inst_RangePredMove" => |hw, org, w| range_move(hw, org, w, true),
        "Inst_GetCellPosition" => inst_get_cell_position,
        "Inst_GetCellPositionX" => |hw, _, w| {
            let x = w.get_cell_x_position();
            if x == -1 {
                return Ok(false);
            }
            let r = hw.find_modified_register(R_BX);
            hw.set_iv(r, x, true, false, false);
            Ok(true)
        },
        "Inst_GetCellPositionY" => |hw, _, w| {
            let y = w.get_cell_y_position();
            if y == -1 {
                return Ok(false);
            }
            let r = hw.find_modified_register(R_BX);
            hw.set_iv(r, y, true, false, false);
            Ok(true)
        },
        "Inst_GetNorthOffset" => |hw, _, w| {
            let out_reg = hw.find_modified_register(R_BX);
            let compass_dir = facing(hw, w);
            hw.set_iv(out_reg, compass_dir, true, false, false);
            Ok(true)
        },
        "Inst_GetPositionOffset" => |hw, org, _| {
            let out_reg = hw.find_modified_register(R_BX);
            hw.set_iv(out_reg, org.northerly, true, false, false);
            let r2 = hw.find_modified_next_register(out_reg);
            hw.set_iv(r2, org.easterly, true, false, false);
            Ok(true)
        },
        "Inst_GetNortherly" => |hw, org, _| {
            let out_reg = hw.find_modified_register(R_BX);
            hw.set_iv(out_reg, org.northerly, true, false, false);
            Ok(true)
        },
        "Inst_GetEasterly" => |hw, org, _| {
            let out_reg = hw.find_modified_register(R_BX);
            hw.set_iv(out_reg, org.easterly, true, false, false);
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
            let r = hw.find_modified_register(R_BX);
            let offset = hw.reg(r) % 3;
            if offset == 0 {
                org.easterly = 0;
                org.northerly = 0;
            } else if offset == 1 {
                org.easterly = 0;
            } else if offset == 2 {
                org.northerly = 0;
            }
            Ok(true)
        },
        "Inst_RotateLeftOne" => |_, _, w| {
            w.rotate(1);
            Ok(true)
        },
        "Inst_RotateRightOne" => |_, _, w| {
            w.rotate(-1);
            Ok(true)
        },
        "Inst_RotateUphill" => inst_rotate_uphill,
        "Inst_RotateUpFtHill" => inst_rotate_up_ft_hill,
        "Inst_RotateHome" => inst_rotate_home,
        "Inst_RotateUnoccupiedCell" => inst_rotate_unoccupied_cell,
        "Inst_RotateX" => inst_rotate_x,
        "Inst_RotateOrgID" => |hw, org, w| rotate_org_id(hw, org, w, "Inst_RotateOrgID"),
        "Inst_RotateAwayOrgID" => |hw, org, w| rotate_org_id(hw, org, w, "Inst_RotateAwayOrgID"),
        "Inst_RotateNeuronAVLeft" => |hw, _, w| {
            let av = neuron_av(hw, w);
            Ok(w.rotate_av(-1, av))
        },
        "Inst_RotateNeuronAVRight" => |hw, _, w| {
            let av = neuron_av(hw, w);
            Ok(w.rotate_av(1, av))
        },
        "Inst_RotateNeuronAVbyX" => |hw, _, w| {
            let avatar_reg = hw.find_modified_register(R_BX);
            let rotate_reg = hw.find_modified_next_register(avatar_reg);
            let avatar_num = hw.reg(avatar_reg);
            let rotate = hw.reg(rotate_reg);
            let av = w.find_av(true, false, avatar_num);
            Ok(w.rotate_av(rotate, av))
        },
        "Inst_MoveNeuronAV" => |hw, org, w| {
            let av = neuron_av(hw, w);
            Ok(w.move_av(org, av))
        },
        "Inst_IfNeuronInputHasOutputAV" => |hw, _, w| {
            let av = neuron_av(hw, w);
            if !w.has_output_av(av) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfNotNeuronInputHasOutputAV" => |hw, _, w| {
            let av = neuron_av(hw, w);
            if w.has_output_av(av) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfNeuronInputFacedHasOutputAV" => |hw, _, w| {
            let av = neuron_av(hw, w);
            if !w.faced_has_output_av(av) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_IfNotNeuronInputFacedHasOutputAV" => |hw, _, w| {
            let av = neuron_av(hw, w);
            if w.faced_has_output_av(av) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_SenseResourceID" => inst_sense_resource_id,
        "Inst_SenseResQuant" => inst_sense_res_quant,
        "Inst_SenseNest" => inst_sense_nest,
        "Inst_SenseResDiff" => inst_sense_res_diff,
        "Inst_LookAhead" => |hw, org, w| look_ahead(hw, org, w, false),
        "Inst_LookAheadIntercept" => |hw, org, w| {
            hw.sensor.set_return_relative_facing(true);
            look_ahead(hw, org, w, false)
        },
        "Inst_LookAround" => |hw, org, w| look_around(hw, org, w, false),
        "Inst_LookAroundIntercept" => |hw, org, w| {
            hw.sensor.set_return_relative_facing(true);
            look_around(hw, org, w, false)
        },
        "Inst_LookFT" => |hw, org, w| look_ahead(hw, org, w, true),
        "Inst_LookAroundFT" => |hw, org, w| look_around(hw, org, w, true),
        "Inst_SenseFacedHabitat" => inst_sense_faced_habitat,
        "Inst_SetForageTarget" => inst_set_forage_target,
        "Inst_SetForageTargetOnce" => |hw, org, w| {
            if org.has_set_ft {
                return Ok(false);
            }
            inst_set_forage_target(hw, org, w)
        },
        "Inst_SetRandForageTargetOnce" => inst_set_rand_forage_target_once,
        "Inst_SetRandPFTOnce" => inst_set_rand_pft_once,
        "Inst_ShowForageTarget" => inst_show_forage_target,
        "Inst_ShowForageTargetGenetic" => |hw, org, _| {
            if org.forage_target != 1 {
                return Ok(false);
            }
            if hw.next_is_nop() {
                let next = hw.ip_head().get_next_inst(&hw.memory, &hw.base.inst_set);
                let mut prop_target = hw.base.inst_set.nop_mod(next) % 2;
                if prop_target == 1 {
                    prop_target = 2;
                }
                org.show_ft = prop_target;
            }
            Ok(true)
        },
        "Inst_GetForageTarget" => |hw, org, _| {
            let target_reg = hw.find_modified_register(R_BX);
            hw.set_ivp(target_reg, org.forage_target);
            Ok(true)
        },
        "Inst_GetLocOrgDensity" => |hw, org, w| org_density(hw, org, w, false),
        "Inst_GetFacedOrgDensity" => |hw, org, w| org_density(hw, org, w, true),
        "Inst_CollectEdible" => |hw, org, w| collect_edible(hw, org, w, false),
        "Inst_Nop2CollectEdible" => |hw, org, w| collect_edible(hw, org, w, true),
        "Inst_NopCollectEdible" => inst_nop_collect_edible,
        "Inst_CollectSpecific" => inst_collect_specific,
        "Inst_DepositResource" => |hw, org, w| deposit_resource(hw, org, w, false),
        "Inst_NopDepositResource" => |hw, org, w| deposit_resource(hw, org, w, true),
        "Inst_DepositSpecific" => |hw, org, w| deposit_specific(hw, org, w, false),
        "Inst_NopDepositSpecific" => |hw, org, w| deposit_specific(hw, org, w, true),
        "Inst_DepositAllAsSpecific" => {
            |hw, org, w| deposit_all_as_specific(hw, org, w, DepositAll::Real)
        }
        "Inst_NopDepositAllAsSpecific" => {
            |hw, org, w| deposit_all_as_specific(hw, org, w, DepositAll::Nop)
        }
        "Inst_Nop2DepositAllAsSpecific" => {
            |hw, org, w| deposit_all_as_specific(hw, org, w, DepositAll::Nop2)
        }
        "Inst_GetResStored" => inst_get_res_stored,
        "Inst_GetSpecificStored" => |hw, org, w| {
            let bins = org.rbins().to_vec();
            let out_reg = hw.find_modified_register(R_BX);
            let spec = w.config().collect_specific_resource;
            let v = bin_at(&bins, spec, "Inst_GetSpecificStored")?;
            hw.set_iv(out_reg, v as i32, true, false, false);
            Ok(true)
        },
        "Inst_CollectCellData" => |hw, _, w| {
            let out_reg = hw.find_modified_register(R_BX);
            let value = w.get_cell_data();
            hw.set_iv(out_reg, value, true, false, false);
            hw.last_cell_data = (true, value);
            Ok(true)
        },
        "Inst_IfCellDataChanged" => |hw, _, w| {
            if !hw.last_cell_data.0 || hw.last_cell_data.1 == w.get_cell_data() {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_ReadCellData" => |hw, _, w| {
            let out_reg = hw.find_modified_register(R_BX);
            let v = w.get_cell_data();
            hw.set_iv(out_reg, v, true, false, false);
            Ok(true)
        },
        "Inst_SetOpinion" => |hw, org, w| {
            let r = hw.find_modified_register(R_BX);
            w.set_opinion(hw.reg(r), org);
            Ok(true)
        },
        "Inst_GetOpinion" => |hw, org, w| {
            if w.has_opinion(org) {
                let opinion_reg = hw.find_modified_register(R_BX);
                let age_reg = Hw::find_next_register(opinion_reg);
                let (op, when) = org.get_opinion();
                hw.set_iv(opinion_reg, op, true, false, false);
                hw.set_iv(age_reg, w.update().wrapping_sub(when), true, false, false);
            }
            Ok(true)
        },
        "Inst_SendMessage" => inst_send_message,
        "Inst_RetrieveMessage" => inst_retrieve_message,
        "Inst_Broadcast1" => |hw, org, w| {
            let label_reg = hw.find_modified_register(R_BX);
            let data_reg = Hw::find_next_register(label_reg);
            let mut msg = new_message(org, w, 0);
            msg.label = hw.reg(label_reg) as u32;
            msg.data = hw.reg(data_reg) as u32;
            org_broadcast_message(hw, org, w, &mut msg, 1)
        },
        "Inst_DonateResToDeme" => |_, org, w| {
            // cOrganism::DonateResConsumedToDeme: no deme in a test CPU.
            let deme_id = w.get_deme_id();
            if deme_id >= 0 {
                let consumed = org.phenotype.take_resources_consumed();
                w.deme_add_resources_consumed(deme_id, consumed);
            }
            Ok(true)
        },
        "Inst_JoinGroup" => inst_join_group,
        "Inst_ChangePredGroup" => inst_change_pred_group,
        "Inst_MakePredGroup" => inst_make_pred_group,
        "Inst_LeavePredGroup" => inst_leave_pred_group,
        "Inst_AdoptPredGroup" => inst_adopt_pred_group,
        "Inst_GetGroupID" => |hw, org, _| {
            if org.has_opinion() {
                let group_reg = hw.find_modified_register(R_BX);
                hw.set_ivp(group_reg, org.get_opinion().0);
            }
            Ok(true)
        },
        "Inst_GetPredGroupID" => |hw, org, _| {
            if is_prey_ft(org) {
                return Ok(false);
            }
            if org.has_opinion() {
                let group_reg = hw.find_modified_register(R_BX);
                hw.set_ivp(group_reg, org.get_opinion().0);
            }
            Ok(true)
        },
        "Inst_GetFacedOrgID" => inst_get_faced_org_id,
        "Inst_AttackPrey" => inst_attack_prey,
        "Inst_AttackPreyArea" => inst_attack_prey_area,
        "Inst_AttackPreyGroup" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                true,
                Sharing::No,
                "attack-prey-group",
                "Inst_AttackPreyGroup",
            )
        },
        "Inst_AttackPreyShare" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                false,
                Sharing::Share,
                "attack-prey-share",
                "Inst_AttackPreyShare",
            )
        },
        "Inst_AttackPreyNoShare" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                false,
                Sharing::No,
                "attack-prey-no-share",
                "Inst_AttackPreyNoShare",
            )
        },
        "Inst_AttackPreyFakeShare" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                false,
                Sharing::Fake,
                "attack-prey-fake-share",
                "Inst_AttackPreyFakeShare",
            )
        },
        "Inst_AttackPreyGroupShare" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                true,
                Sharing::Share,
                "attack-prey-group-share",
                "Inst_AttackPreyGroupShare",
            )
        },
        "Inst_AttackPreyFakeGroupShare" => |hw, org, w| {
            attack_prey_pack(
                hw,
                org,
                w,
                true,
                Sharing::Fake,
                "attack-prey-fake-group-share",
                "Inst_AttackPreyFakeGroupShare",
            )
        },
        "Inst_AttackFTPrey" => inst_attack_ft_prey,
        "Inst_AttackSpecPrey" => inst_attack_spec_prey,
        "Inst_AttackSpecPreyChance" => inst_attack_spec_prey_chance,
        "Inst_AttackPoisonPrey" => inst_attack_poison_prey,
        "Inst_AttackPoisonFTPrey" => inst_attack_poison_ft_prey,
        "Inst_AttackPoisonFTPreyGenetic" => inst_attack_poison_ft_prey_genetic,
        "Inst_AttackPoisonFTMixedPrey" => inst_attack_poison_ft_mixed_prey,
        "Inst_AttackPred" => |hw, org, w| attack_pred(hw, org, w, false, "Inst_AttackPred"),
        "Inst_KillPred" => |hw, org, w| attack_pred(hw, org, w, true, "Inst_KillPred"),
        "Inst_FightPred" => inst_fight_pred,
        "Inst_FightMeritOrg" => |hw, org, w| fight_org(hw, org, w, false, "Inst_FightMeritOrg"),
        "Inst_FightBonusOrg" => |hw, org, w| fight_org(hw, org, w, true, "Inst_FightBonusOrg"),
        "Inst_GetMeritFightOdds" => inst_get_merit_fight_odds,
        "Inst_FightOrg" => inst_fight_org,
        "Inst_MarkCell" => |hw, org, w| mark_cell(hw, org, w, false),
        "Inst_MarkPredCell" => |hw, org, w| mark_cell(hw, org, w, true),
        "Inst_MarkGroupCell" => |hw, org, w| mark_cell(hw, org, w, false),
        "Inst_ReadGroupCell" => inst_read_group_cell,
        "Inst_ReadFacedCell" => |hw, org, w| read_faced_cell(hw, org, w, false),
        "Inst_ReadFacedPredCell" => |hw, org, w| read_faced_cell(hw, org, w, true),
        "Inst_TeachOffspring" => |_, org, _| {
            org.teach = true;
            Ok(true)
        },
        "Inst_LearnParent" => inst_learn_parent,
        "Inst_SetGuard" => inst_set_guard,
        "Inst_SetGuardOnce" => |hw, org, w| {
            let set_ok = if !org.guard {
                inst_set_guard(hw, org, w)?
            } else {
                w.stats_inc_guard_fail();
                false
            };
            let r = hw.find_modified_register(R_BX);
            hw.set_iv(r, i32::from(set_ok), true, false, false);
            Ok(set_ok)
        },
        // QUIRK: skips the next instruction when the organism IS on a
        // nest or den, the opposite of its description.
        "Inst_IfNest" => |hw, org, w| {
            if on_den(hw, org, w) {
                hw.ip_advance();
            }
            Ok(true)
        },
        "Inst_GetNumGuards" => |hw, org, w| on_den_count(hw, org, w, false),
        "Inst_GetNumJuvs" => |hw, org, w| on_den_count(hw, org, w, true),
        "Inst_ActivateDisplay" => |_, org, _| {
            if org.org_display.is_none() {
                return Ok(false);
            }
            org.display = true;
            Ok(true)
        },
        // cOrganism::UpdateOrgDisplay: swap in what the sensor last saw.
        "Inst_UpdateDisplay" => |_, org, _| match org.queued_display.take() {
            Some(d) => {
                org.org_display = Some(d);
                Ok(true)
            }
            None => Ok(false),
        },
        "Inst_ModifyDisplay" => inst_modify_display,
        "Inst_ReadLastSeenDisplay" => |hw, _, _| read_last_display(hw, false),
        "Inst_ModifySimpDisplay" => inst_modify_simp_display,
        "Inst_ReadLastSimpDisplay" => |hw, _, _| read_last_display(hw, true),
        "Inst_KillDisplay" => |_, org, _| {
            if !org.display {
                return Ok(false);
            }
            org.display = false;
            Ok(true)
        },
        "Inst_IncPredTolerance" => |hw, org, w| change_pred_tolerance(hw, org, w, true),
        "Inst_DecPredTolerance" => |hw, org, w| change_pred_tolerance(hw, org, w, false),
        "Inst_GetPredTolerance" => inst_get_pred_tolerance,
        "Inst_GetPredGroupTolerance" => inst_get_pred_group_tolerance,
        "Inst_DonateSpecific" => inst_donate_specific,
        "Inst_GetFacedEditDistance" => inst_get_faced_edit_distance,
        _ => return None,
    };
    Some(f)
}

// ----------------------------------------------------------------------
// Small helpers
// ----------------------------------------------------------------------

/// The `Err` for a path reached at run time that is not ported.
fn unsupported(what: &str, method: &str) -> String {
    format!("unsupported: {what} (C++: cHardwareExperimental.cc:cHardwareExperimental::{method})")
}

/// `cOrganism::IsPreyFT()`
pub(crate) fn is_prey_ft(org: &OrgState) -> bool {
    org.forage_target > -2
}

/// `cOrganism::IsPredFT()`
pub(crate) fn is_pred_ft(org: &OrgState) -> bool {
    org.forage_target == -2
}

/// `GetNeighborhoodSize()`, or `GetAVNumNeighbors()` with avatars.
fn num_neighbors(hw: &Hw, w: &dyn OrgInterface) -> i32 {
    if hw.use_avatar != 0 {
        w.get_av_num_neighbors(0)
    } else {
        w.get_num_neighbors()
    }
}

/// `GetFacedDir()`, or `GetAVFacing()` with avatars.
fn facing(hw: &Hw, w: &dyn OrgInterface) -> i32 {
    if hw.use_avatar != 0 {
        w.get_av_facing(0)
    } else {
        w.get_faced_dir()
    }
}

/// `GetResourceVal(ctx, i)`, or `GetAVResourceVal(ctx, i)` with avatars.
fn here_res_val(hw: &Hw, org: &mut OrgState, w: W<'_>, i: i32) -> f64 {
    if hw.use_avatar != 0 {
        w.get_av_resource_val(org, i, 0)
    } else {
        w.get_resource_val(org, i)
    }
}

/// `GetFacedResourceVal(ctx, i)`, or `GetAVFacedResourceVal(ctx, i)`.
fn faced_res_val(hw: &Hw, org: &mut OrgState, w: W<'_>, i: i32) -> f64 {
    if hw.use_avatar != 0 {
        w.get_av_faced_resource_val(org, i, 0)
    } else {
        w.get_faced_resource_val(org, i)
    }
}

/// `GetResources(ctx)`, or `GetAVResources(ctx)`.
fn here_resources(hw: &Hw, org: &mut OrgState, w: W<'_>) -> Vec<f64> {
    if hw.use_avatar != 0 {
        w.get_av_resources(org, 0)
    } else {
        w.get_resources(org)
    }
}

/// `GetFacedCellResources(ctx)`, or `GetAVFacedResources(ctx)`.
fn faced_resources(hw: &Hw, org: &mut OrgState, w: W<'_>) -> Vec<f64> {
    if hw.use_avatar != 0 {
        w.get_av_faced_resources(org, 0)
    } else {
        w.get_faced_cell_resources(org)
    }
}

/// `UpdateResources(ctx, change)`, or `UpdateAVResources(ctx, change)`.
fn update_res(hw: &Hw, org: &mut OrgState, w: W<'_>, change: &[f64]) {
    if hw.use_avatar != 0 {
        w.update_av_resources(org, change, 0);
    } else {
        w.update_resources(org, change);
    }
}

/// The resource library's `(habitat, threshold, refuge)` per resource.
pub(crate) fn res_info(w: &dyn OrgInterface) -> Vec<(i32, f64, i32)> {
    w.environment()
        .resource_lib()
        .resources()
        .iter()
        .map(|r| (r.habitat, r.threshold, r.refuge))
        .collect()
}

/// `array[i]` of a resource array; the C++ reads out of bounds
/// (undefined) when `i` is not a valid index.
fn at(v: &[f64], i: i32, method: &str) -> Result<f64, String> {
    usize::try_from(i)
        .ok()
        .and_then(|i| v.get(i).copied())
        .ok_or_else(|| {
            unsupported(
                &format!("resource index {i} outside the resource array (undefined in C++)"),
                method,
            )
        })
}

/// `GetRBins()[i]`, with the same check as [`at`].
fn bin_at(bins: &[f64], i: i32, method: &str) -> Result<f64, String> {
    at(bins, i, method)
}

/// The Nth (`nth <= 0`: first) entry of an ordered target-id set, as the
/// C++ `set<int>::iterator` walk does.
fn nth_target(ids: &[i32], nth: i32, method: &str) -> Result<i32, String> {
    let n = usize::try_from(nth).unwrap_or(0);
    ids.get(n).copied().ok_or_else(|| {
        unsupported(
            "walking past the end of the target-id set (undefined in C++)",
            method,
        )
    })
}

/// One step of `m_northerly` / `m_easterly` for a move in `facing`
/// (`cOrganism::Move` / `MoveAV`).
pub(crate) fn count_step(org: &mut OrgState, facing: i32) {
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

// ----------------------------------------------------------------------
// State grid
// ----------------------------------------------------------------------

/// The organism's state grid. The C++ indexes `m_ext_mem` and the
/// environment's state grids without checks; with no state grid that is
/// undefined, and is the `Err` here.
fn state_grid(
    hw: &Hw,
    org: &OrgState,
    w: &dyn OrgInterface,
    method: &str,
) -> Result<avida_environment::StateGrid, String> {
    let n = w.environment().num_state_grids();
    let sg = org.cur_sg;
    if hw.base.ext_mem.len() < 3 || sg < 0 || sg >= n {
        return Err(unsupported(
            "a state-grid instruction without a state grid (undefined in C++)",
            method,
        ));
    }
    Ok(w.environment().state_grid(sg as usize).clone())
}

/// `sg.GetStateAt(x, y)` with the position checked.
fn sg_state(
    sg: &avida_environment::StateGrid,
    x: i32,
    y: i32,
    method: &str,
) -> Result<i32, String> {
    if x < 0 || y < 0 || x >= sg.width() || y >= sg.height() {
        return Err(unsupported(
            &format!("state-grid position ({x}, {y}) off the grid (undefined in C++)"),
            method,
        ));
    }
    Ok(sg.state_at(x, y))
}

/// `Inst_SGMove`: step on the toroidal state grid, count the state reached
/// and push the position onto the movement history.
fn inst_sg_move(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let sg = state_grid(hw, org, w, "Inst_SGMove")?;
    let (width, height) = (sg.width(), sg.height());
    let mem = &mut hw.base.ext_mem;
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
    let state = sg_state(&sg, x, y, "Inst_SGMove")?;
    match mem.get_mut(3usize.wrapping_add(state as usize)) {
        Some(v) => *v = v.wrapping_add(1),
        None => {
            return Err(unsupported(
                "state count past the extended memory (undefined in C++)",
                "Inst_SGMove",
            ))
        }
    }
    mem.push(sg.id_for(x, y));
    Ok(true)
}

/// `Inst_SGRotateL` (`dir` -1) and `Inst_SGRotateR` (`dir` 1).
fn sg_rotate(hw: &mut Hw, org: &mut OrgState, w: W<'_>, dir: i32) -> R {
    if hw.base.ext_mem.len() < 3 {
        // assert(m_ext_mem.GetSize() > 3) in C++; the write is undefined.
        state_grid(
            hw,
            org,
            w,
            if dir < 0 {
                "Inst_SGRotateL"
            } else {
                "Inst_SGRotateR"
            },
        )?;
    }
    let f = &mut hw.base.ext_mem[2];
    *f = f.wrapping_add(dir);
    if dir < 0 && *f < 0 {
        *f = 7;
    } else if dir > 0 && *f > 7 {
        *f = 0;
    }
    Ok(true)
}

/// `Inst_SGSense`: ?BX? = the sense value of the current state-grid cell.
fn inst_sg_sense(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let sg = state_grid(hw, org, w, "Inst_SGSense")?;
    let reg_used = hw.find_modified_register(R_BX);
    let (x, y) = (hw.base.ext_mem[0], hw.base.ext_mem[1]);
    let state = sg_state(&sg, x, y, "Inst_SGSense")?;
    let v = usize::try_from(state)
        .ok()
        .and_then(|s| sg.sense_values().get(s).copied())
        .ok_or_else(|| {
            unsupported(
                &format!("state {state} without a sense value (undefined in C++)"),
                "Inst_SGSense",
            )
        })?;
    hw.set_ivp(reg_used, v);
    Ok(true)
}

// ----------------------------------------------------------------------
// Movement
// ----------------------------------------------------------------------

/// `cOrganism::Move(ctx)` for this hardware: as in the heads CPU, except
/// that `InterruptThread` is `cHardwareBase`'s (does nothing).
fn org_move(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> Result<bool, String> {
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
        hw.org_do_output(org, w, None, false)?;
    }
    Ok(true)
}

/// `cOrganism::MoveAV(ctx)`.
pub(crate) fn org_move_av(org: &mut OrgState, w: W<'_>) -> bool {
    if org.is_dead {
        return false;
    }
    if w.move_av(org, 0) {
        let err = w.config().step_counting_error;
        if err == 0 || w.rng().get_int_range(0, 101) > err {
            let facing = w.get_av_facing(0);
            count_step(org, facing);
        } else {
            // QUIRK: a miscounted step reports the (successful) move as failed.
            return false;
        }
    }
    // QUIRK: a failed avatar move still reports success.
    !org.phenotype.to_delete
}

/// `Move` or `MoveAV` by `m_use_avatar`.
fn move_any(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> Result<bool, String> {
    if hw.use_avatar == 0 {
        org_move(hw, org, w)
    } else {
        Ok(org_move_av(org, w))
    }
}

/// `Inst_Move`: ?BX? = whether the move worked. Fails in the test CPU.
fn inst_move(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if w.get_cell_id() == -1 {
        return Ok(false);
    }
    let ok = move_any(hw, org, w)?;
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, i32::from(ok), true, false, false);
    Ok(true)
}

/// `Inst_JuvMove`: `Inst_Move` for adults that are not guarding.
fn inst_juv_move(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if w.get_cell_id() == -1 {
        return Ok(false);
    }
    if org.phenotype.time_used < w.config().juv_period {
        return Ok(false);
    }
    if org.guard {
        return Ok(false);
    }
    inst_move_after_checks(hw, org, w)
}

fn inst_move_after_checks(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let ok = move_any(hw, org, w)?;
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, i32::from(ok), true, false, false);
    Ok(true)
}

/// `Inst_RangeMove` / `Inst_RangePredMove`: move unless the faced cell is
/// a fresh marking of another group (always allowed for prey in the
/// predator version).
fn range_move(hw: &mut Hw, org: &mut OrgState, w: W<'_>, pred: bool) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if w.get_cell_id() == -1 {
        return Ok(false);
    }
    let mut safe_passage = true;
    let mut move_success = false;
    let mut faced_range = w.get_faced_cell_data_territory();
    if ua == 2 {
        faced_range = w.get_av_faced_data_territory(0);
    }
    let mut marked_update = w.get_faced_cell_data_update();
    if ua == 2 {
        marked_update = w.get_av_faced_data_update(0);
    }
    if (!pred || !is_prey_ft(org))
        && faced_range != -1
        && faced_range != org.get_opinion().0
        && w.update().wrapping_sub(marked_update) <= w.config().marking_expire_date
    {
        safe_passage = false;
    }
    if safe_passage {
        move_success = move_any(hw, org, w)?;
    }
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, i32::from(move_success), true, false, false);
    let r2 = hw.find_modified_next_register(R_BX);
    hw.set_iv(r2, i32::from(safe_passage), true, false, false);
    Ok(true)
}

/// `Inst_GetCellPosition`: ?BX? = x, the next register = y. Fails in the
/// test CPU.
fn inst_get_cell_position(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    let x = w.get_cell_x_position();
    let y = w.get_cell_y_position();
    if x == -1 || y == -1 {
        return Ok(false);
    }
    let xreg = hw.find_modified_register(R_BX);
    let yreg = Hw::find_next_register(xreg);
    hw.set_iv(xreg, x, true, false, false);
    hw.set_iv(yreg, y, true, false, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Rotation
// ----------------------------------------------------------------------

/// `res_diff` of the uphill rotations: the best faced level, or the
/// percent gain over the current level, rounded.
fn res_diff(max_res: f64, current_res: f64) -> i32 {
    if current_res == 0.0 {
        max_res as i32
    } else {
        ((max_res - current_res) / current_res).mul_add(100.0, 0.5) as i32
    }
}

/// `Inst_RotateUphill`: face the neighbour richest in the resource named
/// by the opinion (0 without one); ?BX? = the gain.
fn inst_rotate_uphill(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let group = if org.has_opinion() {
        org.get_opinion().0
    } else {
        0
    };
    let current_res = here_res_val(hw, org, w, group);
    let n = num_neighbors(hw, w);
    let mut max_res = 0.0;
    for _ in 0..n {
        w.rotate(1);
        let faced = faced_resources(hw, org, w);
        let v = at(&faced, group, "Inst_RotateUphill")?;
        if v > max_res {
            max_res = v;
        }
    }
    if max_res > current_res {
        for _ in 0..n {
            let faced = faced_resources(hw, org, w);
            if at(&faced, group, "Inst_RotateUphill")? != max_res {
                w.rotate(1);
            }
        }
    }
    let diff = res_diff(max_res, current_res);
    let reg_to_set = hw.find_modified_register(R_BX);
    hw.set_iv(reg_to_set, diff, true, false, false);
    Ok(true)
}

/// `Inst_RotateUpFtHill`: `Inst_RotateUphill` for the forage target's
/// resource.
fn inst_rotate_up_ft_hill(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let ft = org.forage_target;
    let current_res = here_res_val(hw, org, w, ft);
    let n = num_neighbors(hw, w);
    let mut max_res = 0.0;
    for _ in 0..n {
        w.rotate(1);
        let faced = faced_res_val(hw, org, w, ft);
        if faced > max_res {
            max_res = faced;
        }
    }
    if max_res > current_res {
        for _ in 0..n {
            if faced_res_val(hw, org, w, ft) != max_res {
                w.rotate(1);
            }
        }
    }
    let diff = res_diff(max_res, current_res);
    let reg_to_set = hw.find_modified_register(R_BX);
    hw.set_iv(reg_to_set, diff, true, false, false);
    Ok(true)
}

/// `Inst_RotateHome`: face the birth cell (or the last zeroed position).
fn inst_rotate_home(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let easterly = org.easterly;
    let northerly = org.northerly;
    let correct_facing = if northerly > 0 && easterly == 0 {
        0
    } else if northerly > 0 && easterly < 0 {
        1
    } else if northerly == 0 && easterly < 0 {
        2
    } else if northerly < 0 && easterly < 0 {
        3
    } else if northerly < 0 && easterly == 0 {
        4
    } else if northerly < 0 && easterly > 0 {
        5
    } else if northerly == 0 && easterly > 0 {
        6
    } else if northerly > 0 && easterly > 0 {
        7
    } else {
        0
    };
    let rotates = num_neighbors(hw, w);
    for _ in 0..rotates {
        w.rotate(1);
        if (hw.use_avatar == 0 && w.get_faced_dir() == correct_facing)
            || (hw.use_avatar != 0 && w.get_av_facing(0) == correct_facing)
        {
            break;
        }
    }
    Ok(true)
}

/// `Inst_RotateUnoccupiedCell`: turn to an empty neighbour; ?BX? = whether
/// one was found.
fn inst_rotate_unoccupied_cell(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let reg_used = hw.find_modified_register(R_BX);
    let n = num_neighbors(hw, w);
    for _ in 0..n {
        if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_av(0)) {
            hw.set_iv(reg_used, 1, true, false, false);
            return Ok(true);
        }
        w.rotate(1);
    }
    hw.set_iv(reg_used, 0, true, false, false);
    Ok(true)
}

/// `Inst_RotateX`: turn ?BX? steps (sign = direction, modulo 8); ?BX? =
/// the turn made. Fails without neighbours.
fn inst_rotate_x(hw: &mut Hw, _org: &mut OrgState, w: W<'_>) -> R {
    if num_neighbors(hw, w) == 0 {
        return Ok(false);
    }
    let reg_used = hw.find_modified_register(R_BX);
    let raw = hw.reg(reg_used);
    mark_from(hw, reg_used);
    let rot_dir = if raw < 0 { -1 } else { 1 };
    // `abs(rot_num)`, then `% 8` above 7. QUIRK: the reference binary
    // computes `abs & 7` (the compiler takes `abs` as non-negative), so
    // INT_MIN turns 0 steps and writes 0.
    let rot_num = (raw.unsigned_abs() & 7) as i32;
    for _ in 0..rot_num {
        w.rotate(rot_dir);
    }
    hw.set_iv(reg_used, rot_num.wrapping_mul(rot_dir), true, false, false);
    Ok(true)
}

/// `Inst_RotateOrgID` / `Inst_RotateAwayOrgID`: turn toward (away from)
/// the living organism whose id is in ?BX? when it is within `LOOK_DIST`,
/// and report it as the look sensor does. Fails for an invalid id, for
/// itself and for an id that is not alive (always, in the test CPU).
/// Without avatars the organism turns until it faces that way (or a full
/// circle).
fn rotate_org_id(hw: &mut Hw, org: &mut OrgState, w: W<'_>, method: &str) -> R {
    let away = method == "Inst_RotateAwayOrgID";
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let id_sought_reg = hw.find_modified_register(R_BX);
    let id_sought = hw.reg(id_sought_reg);
    mark_from(hw, id_sought_reg);
    let worldx = w.config().world_x;
    let worldy = w.config().world_y;
    let long_axis = (f64::from(worldx.max(worldy)) * 0.5 + 0.5) as i32;
    let max_dist = match w.config().look_dist {
        -1 => long_axis,
        d => d,
    };

    // return false if invalid number or self
    if id_sought < 0 || id_sought == org.id {
        return Ok(false);
    }
    // if valid number, does the value represent a living organism?
    let Some(target_uid) = w.find_live_org(id_sought) else {
        return Ok(false);
    };
    let Some(target) = w.seen_org(target_uid) else {
        // Only the running organism is out of its cell, and its own id
        // returned above.
        return Err(unsupported(
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
            return Ok(true); // avatars in same cell
        }
    }
    let target_x = target_org_cell % worldx;
    let target_y = target_org_cell / worldx;
    let searching_x = searching_org_cell % worldx;
    let searching_y = searching_org_cell / worldx;
    let mut x_dist = target_x - searching_x;
    let mut y_dist = target_y - searching_y;

    let mut travel_dist = x_dist.abs().max(y_dist.abs());
    if w.config().world_geometry == 2 {
        let travel_dist2 = travel_dist;
        let facing = if ua == 2 {
            w.get_av_facing(0)
        } else {
            w.get_faced_dir()
        };
        crate::org_sensor::OrgSensor::get_torus_travel_dist(
            &mut travel_dist,
            &mut x_dist,
            &mut y_dist,
            facing,
            worldx,
            worldy,
        );
        if travel_dist2 != travel_dist {
            x_dist *= -1;
            y_dist *= -1;
        }
    }
    if travel_dist > max_dist {
        return Ok(false);
    }

    // The direction toward the target, N = 0 clockwise; away turns it round.
    let toward = if y_dist < 0 && x_dist == 0 {
        0
    } else if y_dist < 0 && x_dist > 0 {
        1
    } else if y_dist == 0 && x_dist > 0 {
        2
    } else if y_dist > 0 && x_dist > 0 {
        3
    } else if y_dist > 0 && x_dist == 0 {
        4
    } else if y_dist > 0 && x_dist < 0 {
        5
    } else if y_dist == 0 && x_dist < 0 {
        6
    } else if y_dist < 0 && x_dist < 0 {
        7
    } else {
        -1
    };
    // `correct_facing` starts at 0 when no case matches (same cell).
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
            if ua == 0 && w.get_faced_dir() == correct_facing {
                found_org = true;
                break;
            }
        }
    }
    // return some data as in look sensor
    if found_org {
        let dist_reg = hw.find_modified_next_register(id_sought_reg);
        let dir_reg = hw.find_modified_next_register(dist_reg);
        let fat_reg = hw.find_modified_next_register(dir_reg);
        let ft_reg = hw.find_modified_next_register(fat_reg);
        let group_reg = hw.find_modified_next_register(ft_reg);

        hw.set_iv(dist_reg, -2, true, true, false);
        // `m_sensor.ReturnRelativeFacing(target_org)`
        let (target_facing, org_facing) = if hw.sensor.uses_avatar() {
            (target.av_facing, w.get_av_facing(0))
        } else {
            (target.faced_dir, w.get_faced_dir())
        };
        let rel = hw.sensor.return_relative_facing(target_facing, org_facing);
        hw.set_iv(dir_reg, rel, true, true, false);
        hw.set_iv(fat_reg, target.cur_bonus as i32, true, true, false);
        hw.set_iv(ft_reg, target.forage_target, true, true, false);
        if let Some(op) = target.opinion {
            hw.set_iv(group_reg, op, true, true, false);
        }
        if target.is_displaying || w.config().use_display != 0 {
            if let Some(display) = target.display {
                hw.sensor.set_last_seen_display(&display);
            }
        }
    }
    Ok(true)
}

/// The input avatar selected by ?BX? (`FindAV(true, false, ?BX?)`).
fn neuron_av(hw: &mut Hw, w: W<'_>) -> i32 {
    let avatar_reg = hw.find_modified_register(R_BX);
    let avatar_num = hw.reg(avatar_reg);
    w.find_av(true, false, avatar_num)
}

// ----------------------------------------------------------------------
// Sensing
// ----------------------------------------------------------------------

/// `Inst_SenseResourceID`: ?BX? = the id of the most plentiful resource
/// here (unchanged when none is above 0).
fn inst_sense_resource_id(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let cell_res = here_resources(hw, org, w);
    let reg_to_set = hw.find_modified_register(R_BX);
    let mut max_resource = 0.0;
    for (i, &v) in cell_res.iter().enumerate() {
        if v > max_resource {
            max_resource = v;
            hw.set_iv(reg_to_set, i as i32, true, true, false);
        }
    }
    Ok(true)
}

/// `Inst_SenseResQuant`: the faced amount of the resource asked for in
/// ?BX? (or of all food resources).
fn inst_sense_res_quant(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let info = res_info(w);
    let size = info.len() as i32;
    let req_reg = hw.find_modified_register(R_BX);
    let mut res_sought = -1;
    let r = hw.find_modified_register(R_BX);
    let res_req = hw.reg(r);
    if res_req < size && res_req > 0 && info[res_req as usize].0 != 3 {
        res_sought = res_req;
    }
    let res_amount = 0;
    let mut faced_res: i32 = 0;
    if res_sought != -1 {
        faced_res = faced_res_val(hw, org, w, res_sought) as i32;
    } else {
        for (i, &(habitat, _, _)) in info.iter().enumerate() {
            if habitat == 0 || habitat > 5 {
                faced_res = faced_res.wrapping_add(faced_res_val(hw, org, w, i as i32) as i32);
            }
        }
    }
    // res_amount is always 0, so the "% change" is the faced amount.
    let res_diff = faced_res;
    hw.set_iv(req_reg, res_sought, true, true, false);
    let res_tot_reg = hw.find_modified_next_register(req_reg);
    hw.set_iv(res_tot_reg, res_amount, true, true, false);
    let r3 = hw.find_modified_next_register(res_tot_reg);
    hw.set_iv(r3, res_diff, true, true, false);
    Ok(true)
}

/// `Inst_SenseNest`: ?BX? = the nest asked for (or the first one here at
/// level 1 or more), the next register = its level.
fn inst_sense_nest(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let info = res_info(w);
    let size = info.len() as i32;
    let is_nest = |i: i32| info[i as usize].0 == 3 || info[i as usize].0 == 4;
    let reg_used = hw.find_modified_register(R_BX);
    let mut nest_id = hw.reg(reg_used);
    let mut nest_val = 0;
    if nest_id < 0 || nest_id >= size || !is_nest(nest_id) {
        for i in 0..size {
            if is_nest(i) {
                let cell_res = here_res_val(hw, org, w, i);
                if cell_res >= 1.0 {
                    nest_id = i;
                    nest_val = cell_res as i32;
                    break;
                }
            }
        }
    } else {
        nest_val = here_res_val(hw, org, w, nest_id) as i32;
    }
    hw.set_iv(reg_used, nest_id, true, true, false);
    let val_reg = hw.find_modified_next_register(reg_used);
    hw.set_iv(val_reg, nest_val, true, true, false);
    Ok(true)
}

/// `Inst_SenseResDiff`: here minus faced amount of the resource asked for
/// (or of all resources).
fn inst_sense_res_diff(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let size = w.environment().resource_lib().size();
    let req_reg = hw.find_modified_register(R_BX);
    let mut res_sought = -1;
    let r = hw.find_modified_register(R_BX);
    let res_req = hw.reg(r);
    if res_req < size && res_req > 0 {
        res_sought = res_req;
    }
    let mut faced_res: i32 = 0;
    let mut res_amount: i32 = 0;
    if res_sought != -1 {
        // QUIRK: the cell amount is read and never used.
        let _cell_res = here_res_val(hw, org, w, res_sought) as i32;
        faced_res = faced_res_val(hw, org, w, res_sought) as i32;
    } else if hw.use_avatar != 0 {
        // QUIRK: with avatars the arrays are read and never summed.
        let _here = w.get_av_resources(org, 0);
        let _faced = w.get_av_faced_resources(org, 0);
    } else {
        let here = w.get_resources(org);
        let faced = w.get_faced_cell_resources(org);
        for i in 0..size {
            res_amount = res_amount.wrapping_add(at(&here, i, "Inst_SenseResDiff")? as i32);
            faced_res = faced_res.wrapping_add(at(&faced, i, "Inst_SenseResDiff")? as i32);
        }
    }
    let r2 = hw.find_modified_next_register(req_reg);
    hw.set_iv(r2, res_sought, true, true, false);
    let inner = hw.find_modified_next_register(req_reg);
    let res_tot_reg = hw.find_modified_next_register(inner);
    hw.set_iv(
        res_tot_reg,
        res_amount.wrapping_sub(faced_res),
        true,
        true,
        false,
    );
    Ok(true)
}

/// `Inst_SenseFacedHabitat`: ?BX? = 3 for a simulated predator ahead, 2
/// for a barrier, 1 for a hill, else 0.
fn inst_sense_faced_habitat(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let reg_to_set = hw.find_modified_register(R_BX);
    let info = res_info(w);
    let cell_res = faced_resources(hw, org, w);
    if cell_res.len() > info.len() {
        return Err(unsupported(
            "more faced resources than library resources (undefined in C++)",
            "Inst_SenseFacedHabitat",
        ));
    }
    for (habitat, value) in [(5, 3), (2, 2), (1, 1)] {
        if cell_res
            .iter()
            .zip(&info)
            .any(|(&v, &(h, _, _))| h == habitat && v > 0.0)
        {
            hw.set_iv(reg_to_set, value, true, false, false);
            return Ok(true);
        }
    }
    hw.set_iv(reg_to_set, 0, true, false, false);
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

/// `Inst_LookAhead` (`use_ft` false) and `Inst_LookFT` (true).
fn look_ahead(hw: &mut Hw, org: &mut OrgState, w: W<'_>, use_ft: bool) -> R {
    let (cell, facing) = if hw.use_avatar != 0 {
        (w.get_av_cell_id(0), w.get_av_facing(0))
    } else {
        (w.get_cell_id(), w.get_faced_dir())
    };
    go_look(hw, org, w, facing, cell, use_ft)
}

/// `Inst_LookAround` / `Inst_LookAroundFT`: look ahead, left or right by
/// the fifth modified register.
fn look_around(hw: &mut Hw, org: &mut OrgState, w: W<'_>, use_ft: bool) -> R {
    let hab_reg = hw.find_modified_register(R_BX);
    let dist_reg = hw.find_modified_next_register(hab_reg);
    let st_reg = hw.find_modified_next_register(dist_reg);
    let id_reg = hw.find_modified_next_register(st_reg);
    let dir_reg = hw.find_modified_next_register(id_reg);

    // QUIRK: `abs(dir) % 3` is compiled as an unsigned remainder, so
    // INT_MIN gives 2 (right).
    let mut search_dir = (hw.reg(dir_reg).unsigned_abs() % 3) as i32;
    mark_from(hw, dir_reg);

    let cfg = w.config();
    if cfg.look_disable == 5 {
        let org_type = cfg.look_disable_type;
        let is_target_type = (org_type == 0 && !is_prey_ft(org))
            || (org_type == 1 && is_prey_ft(org))
            || org_type == 2;
        if is_target_type {
            let rand = w.rng().get_int(i32::MAX);
            search_dir = rand % 3;
        }
    }
    if search_dir == 1 {
        search_dir = -1;
    } else if search_dir == 2 {
        search_dir = 1;
    }
    let mut facing = facing(hw, w).wrapping_add(search_dir);
    if facing == -1 {
        facing = 7;
    } else if facing == 9 {
        facing = 1;
    } else if facing == 8 {
        facing = 0;
    }
    let cell = if hw.use_avatar != 0 {
        w.get_av_cell_id(0)
    } else {
        w.get_cell_id()
    };
    go_look(hw, org, w, facing, cell, use_ft)
}

/// `GoLook(ctx, look_dir, cell_id, use_ft)`: fails without neighbours
/// (always in the test CPU); otherwise looks through the sensor.
fn go_look(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    look_dir: i32,
    cell_id: i32,
    use_ft: bool,
) -> R {
    if (hw.use_avatar == 0 && w.get_num_neighbors() == 0)
        || (hw.use_avatar != 0 && w.get_av_num_neighbors(0) == 0)
        || (is_pred_ft(org) && w.config().pred_confusion == 4)
    {
        return Ok(false);
    }
    let habitat = hw.find_modified_register(R_BX);
    let distance = hw.find_modified_next_register(habitat);
    let search_type = hw.find_modified_next_register(distance);
    let id_sought = hw.find_modified_next_register(search_type);
    let count = hw.find_modified_next_register(id_sought);
    let value = hw.find_modified_next_register(count);
    let group = hw.find_modified_next_register(value);
    let ft = hw.find_modified_next_register(group);
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

    // InitLooking
    let reg_init = LookInit {
        habitat: hw.reg(habitat),
        distance: hw.reg(distance),
        search_type: hw.reg(search_type),
        id_sought: hw.reg(id_sought),
    };
    let ins = [habitat, distance, search_type, id_sought];
    let any_sensor = ins.iter().any(|&r| hw.regv(r).from_sensor);
    let any_message = ins.iter().any(|&r| hw.regv(r).from_message);
    hw.from_sensor = any_sensor || hw.from_sensor;
    hw.from_message = any_message || hw.from_message;
    let mut results = hw
        .sensor
        .set_looking(org, w, &reg_init, look_dir, cell_id, use_ft)?;
    look_results(hw, org, w, &regs, &mut results);
    Ok(true)
}

/// `LookResults(ctx, regs, results)`: write a look's results to the
/// registers (with the configured disabling and tracking).
fn look_results(hw: &mut Hw, org: &mut OrgState, w: W<'_>, regs: &LookRegs, results: &mut LookOut) {
    let cfg = w.config();
    let mut over = false;
    if cfg.look_disable_combo > 0 && results.report_type == 1 {
        let combo = cfg.look_disable_combo;
        let applies = match cfg.look_disable_type {
            2 => true,
            1 => org.forage_target > -2,
            0 => org.forage_target <= -2,
            _ => false,
        };
        if applies {
            let r = &*results;
            let pred_hab = r.habitat == -2 || r.habitat == 5;
            if (combo == 1 && r.habitat == 0)
                || (combo == 2 && pred_hab && (r.search_type == 1 || r.search_type == 2))
                || (combo == 3 && r.habitat == -2 && (r.search_type == -1 || r.search_type == -2))
                || (combo == 2 && pred_hab && r.forage <= -2)
                || (combo == 3 && r.habitat == -2 && r.forage > -2)
            {
                over = true;
            }
        }
    }
    if over {
        results.report_type = 0;
        results.id_sought = -1;
    }
    if cfg.track_look_settings != 0 {
        let s = format!(
            "{},{},{},{},{},{}",
            org.forage_target,
            results.report_type,
            results.habitat,
            results.distance,
            results.search_type,
            results.id_sought
        );
        w.try_write_look_data(&s);
    }
    let set = |hw: &mut Hw, r: usize, v: i32| hw.set_iv(r, v, true, true, false);
    if results.report_type == 0 {
        set(hw, regs.habitat, results.habitat);
        set(hw, regs.distance, -1);
        set(hw, regs.search_type, results.search_type);
        set(hw, regs.id_sought, results.id_sought);
        set(hw, regs.count, 0);
        set(hw, regs.value, 0);
        set(hw, regs.group, -9);
        set(hw, regs.ft, -9);
    } else if results.report_type == 1 {
        set(hw, regs.habitat, results.habitat);
        set(hw, regs.distance, results.distance);
        set(hw, regs.search_type, results.search_type);
        set(hw, regs.id_sought, results.id_sought);
        set(hw, regs.count, results.count);
        set(hw, regs.value, results.value);
        set(hw, regs.group, results.group);
        set(hw, regs.ft, results.forage);
    }
    let cfg = w.config();
    if cfg.look_disable > 5 {
        let org_type = cfg.look_disable_type;
        let target_reg = cfg.look_disable;
        let is_target_type = (org_type == 0 && !is_prey_ft(org))
            || (org_type == 1 && is_prey_ft(org))
            || org_type == 2;
        if is_target_type {
            let randsign = if w.rng().get_uint_range(0, 2) != 0 {
                -1
            } else {
                1
            };
            let rand = w.rng().get_int(i32::MAX).wrapping_mul(randsign);
            let r = match target_reg {
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
                hw.set_iv(r, rand, true, false, false);
            }
        }
    }
    if w.config().track_look_output != 0 {
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
        .map(|r| hw.reg(r).to_string());
        let s = format!(
            "{},{},{}",
            org.forage_target,
            results.report_type,
            vals.join(",")
        );
        w.try_write_look_output(&s);
    }
}

// ----------------------------------------------------------------------
// Forage targets
// ----------------------------------------------------------------------

/// `cOrganism::SetForageTarget(ctx, forage_target, inject)`. The
/// `MAX_PRED` / `MAX_PREY` culls read the stats creature counts (`Err`
/// where the interface cannot give them).
pub fn org_set_forage_target(
    org: &mut OrgState,
    w: W<'_>,
    forage_target: i32,
    inject: bool,
) -> Result<(), String> {
    let (max_pred, max_prey) = (w.config().max_pred, w.config().max_prey);
    if org.parent_ft <= -2 && max_pred != 0 && w.stats_num_total_pred_creatures()? >= max_pred {
        w.kill_rand_pred(org);
    } else if forage_target > -2 && max_prey != 0 && w.stats_num_prey_creatures()? >= max_prey {
        w.kill_rand_prey(org);
    }
    let pps = w.config().pred_prey_switch;
    if pps == -2 || pps > -1 {
        let cur = org.forage_target;
        if forage_target == -2 && cur > -2 {
            if !inject {
                w.dec_num_prey_organisms();
            }
            w.inc_num_pred_organisms();
        } else if forage_target == -2 && cur < -2 {
            if !inject {
                w.dec_num_top_pred_organisms();
            }
            w.inc_num_pred_organisms();
        } else if forage_target < -2 && cur > -2 {
            if !inject {
                w.dec_num_prey_organisms();
            }
            w.inc_num_top_pred_organisms();
        } else if forage_target < -2 && cur == -2 {
            if !inject {
                w.dec_num_pred_organisms();
            }
            w.inc_num_top_pred_organisms();
        } else if forage_target > -2 && cur == -2 {
            w.inc_num_prey_organisms();
            if !inject {
                w.dec_num_pred_organisms();
            }
        } else if forage_target > -2 && cur < -2 {
            w.inc_num_prey_organisms();
            if !inject {
                w.dec_num_top_pred_organisms();
            }
        }
    }
    org.forage_target = forage_target;
    if org.show_ft == -1 {
        org.show_ft = org.forage_target;
    }
    Ok(())
}

/// `cOrganism::CopyParentFT(ctx)`
pub(crate) fn copy_parent_ft(org: &mut OrgState, w: W<'_>) -> Result<(), String> {
    let pps = w.config().pred_prey_switch;
    let mut copy_ft = true;
    if (pps <= 0 || pps == 2) && org.parent_ft > -2 && org.forage_target < -1 {
        copy_ft = false;
    }
    if copy_ft {
        let ft = org.parent_ft;
        org_set_forage_target(org, w, ft, false)?;
    }
    Ok(())
}

/// The `MAX_PRED` cull of `MakePred` / `MakeTopPred`.
fn max_pred_cull(org: &mut OrgState, w: W<'_>) -> Result<(), String> {
    let max_pred = w.config().max_pred;
    if max_pred != 0 && w.stats_num_total_pred_creatures()? >= max_pred {
        w.kill_rand_pred(org);
    }
    Ok(())
}

/// `MakePred(ctx)`
fn make_pred(hw: &Hw, org: &mut OrgState, w: W<'_>) -> Result<(), String> {
    if is_prey_ft(org) {
        max_pred_cull(org, w)?;
        if hw.use_avatar != 0 && w.get_av_cell_id(0) != -1 {
            w.switch_pred_prey(0);
        }
        org_set_forage_target(org, w, -2, false)?;
    }
    Ok(())
}

/// `MakeTopPred(ctx)`
fn make_top_pred(hw: &Hw, org: &mut OrgState, w: W<'_>) -> Result<(), String> {
    if is_prey_ft(org) {
        max_pred_cull(org, w)?;
        if hw.use_avatar != 0 && w.get_av_cell_id(0) != -1 {
            w.switch_pred_prey(0);
        }
        org_set_forage_target(org, w, -3, false)?;
    } else if is_pred_ft(org) {
        org_set_forage_target(org, w, -3, false)?;
    }
    Ok(())
}

/// The environment's target ids in order, and how many are resource
/// targets (not -1, -2 or -3).
pub(crate) fn target_ids(w: &dyn OrgInterface) -> (Vec<i32>, i32) {
    let ids: Vec<i32> = w.environment().target_ids().iter().copied().collect();
    let num_fts = ids
        .iter()
        .filter(|&&t| t != -1 && t != -2 && t != -3)
        .count() as i32;
    (ids, num_fts)
}

/// `Inst_SetForageTarget`: switch to the forage target in ?BX? (folded
/// onto the valid targets); ?BX? = the target set.
fn inst_set_forage_target(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let reg = hw.find_modified_register(R_BX);
    let mut prop_target = hw.reg(reg);
    mark_from(hw, reg);
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
        let (ids, num_fts) = target_ids(w);
        if num_fts == 0 {
            if w.environment().is_target_id(-1) {
                prop_target = -1;
            }
        } else {
            // QUIRK: the walk counts from the start of the whole set,
            // including the -3/-2/-1 entries that num_fts leaves out.
            // QUIRK: `abs(prop_target) % num_fts` is compiled as an
            // unsigned remainder, so INT_MIN folds as 2^31.
            let ft_num = (prop_target.unsigned_abs() % num_fts.unsigned_abs()) as i32;
            prop_target = nth_target(&ids, ft_num, "Inst_SetForageTarget")?;
        }
    }
    if prop_target == -2 && old_target > -2 {
        make_pred(hw, org, w)?;
    } else if prop_target == -3 && old_target > -2 {
        make_top_pred(hw, org, w)?;
    } else if hw.use_avatar != 0
        && prop_target > -2
        && old_target <= -2
        && w.get_av_cell_id(0) != -1
    {
        w.switch_pred_prey(0);
        org_set_forage_target(org, w, prop_target, false)?;
    } else {
        org_set_forage_target(org, w, prop_target, false)?;
    }
    org.has_set_ft = true;
    let r = hw.find_modified_register(R_BX);
    hw.set_ivp(r, prop_target);
    Ok(true)
}

/// `Inst_SetRandForageTargetOnce`: a random resource forage target, once.
fn inst_set_rand_forage_target_once(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if org.has_set_ft {
        return Ok(false);
    }
    let (ids, num_fts) = target_ids(w);
    let mut prop_target = w.rng().get_uint(num_fts as u32) as i32;
    if num_fts == 0 {
        if w.environment().is_target_id(-1) {
            prop_target = -1;
        }
    } else {
        let ft_num = prop_target.wrapping_abs() % num_fts;
        prop_target = nth_target(&ids, ft_num, "Inst_SetRandForageTargetOnce")?;
    }
    org_set_forage_target(org, w, prop_target, false)?;
    org.has_set_ft = true;
    let r = hw.find_modified_register(R_BX);
    hw.set_ivp(r, prop_target);
    Ok(true)
}

/// `Inst_SetRandPFTOnce`: forage target 0, 1 or 2 at random, once. With
/// `MAX_PREY_BT`, when at least that many living organisms (the running one
/// included) already have the target, a random one of them dies first.
fn inst_set_rand_pft_once(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if org.has_set_ft {
        return Ok(false);
    }
    let mut prop_target = 2;
    if w.rng().p(0.5) {
        prop_target = 0;
        if w.rng().p(0.5) {
            prop_target = 1;
        }
    }
    // The C++ walks the live organisms whatever MAX_PREY_BT is; only the
    // kill depends on it.
    let max_prey_bt = w.config().max_prey_bt;
    if max_prey_bt != 0 {
        let orgs: Vec<(OrgUid, i32)> = w
            .live_org_forage_targets()?
            .into_iter()
            .filter(|&(_, _, ft)| ft.unwrap_or(org.forage_target) == prop_target)
            .map(|(uid, cell, _)| (uid, cell))
            .collect();
        let in_use = orgs.len() as i32;
        if in_use >= max_prey_bt {
            let pick = w.rng().get_uint_range(0, in_use as u32) as usize;
            // `orgs[pick]->Die(ctx)`
            match orgs.get(pick) {
                // The test organism is none of the live organisms.
                Some(&(uid, cell)) if w.test_mode() => w.kill_population_org(uid, cell)?,
                Some(&(uid, _)) if uid == org.uid => org.die(w),
                Some(&(_, cell)) => w.kill_cell_id(org, cell),
                None => {
                    return Err(unsupported(
                        &format!(
                            "MAX_PREY_BT={max_prey_bt} with no organism of forage target \
                             {prop_target} (undefined in C++: orgs[0] of an empty array)"
                        ),
                        "Inst_SetRandPFTOnce",
                    ))
                }
            }
        }
    }
    org_set_forage_target(org, w, prop_target, false)?;
    org.has_set_ft = true;
    let r = hw.find_modified_register(R_BX);
    hw.set_ivp(r, prop_target);
    Ok(true)
}

/// `Inst_ShowForageTarget`: a mimic shows the forage target in ?BX?.
fn inst_show_forage_target(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let reg = hw.find_modified_register(R_BX);
    let mut prop_target = hw.reg(reg);
    mark_from(hw, reg);
    if org.forage_target != 1 {
        return Ok(false);
    }
    if !w.environment().is_target_id(prop_target) {
        let (ids, _) = target_ids(w);
        let num_fts = ids.len() as i32;
        if num_fts == 0 {
            return Err(unsupported(
                "no forage targets (the C++ divides by zero)",
                "Inst_ShowForageTarget",
            ));
        }
        // QUIRK: an unsigned remainder, as in `Inst_SetForageTarget`.
        let ft_num = (prop_target.unsigned_abs() % num_fts.unsigned_abs()) as i32;
        prop_target = nth_target(&ids, ft_num, "Inst_ShowForageTarget")?;
    }
    org.show_ft = prop_target;
    let r = hw.find_modified_register(R_BX);
    hw.set_ivp(r, prop_target);
    Ok(true)
}

/// `Inst_GetLocOrgDensity` / `Inst_GetFacedOrgDensity`: fail without
/// neighbours (always in the test CPU); otherwise ?BX? = the living prey
/// and the next register = the living predators (other than the organism
/// itself) in the cells up to five steps away, clipped to the world's
/// edges (`faced`: only the half ahead, a quadrant for a diagonal facing).
/// With avatars the prey and predator avatars are counted instead.
fn org_density(hw: &mut Hw, org: &OrgState, w: W<'_>, faced: bool) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let worldx = w.config().world_x;
    let worldy = w.config().world_y;
    let (num_neighbors, own_cell) = if ua == 0 {
        (w.get_num_neighbors(), w.get_cell_id())
    } else {
        (w.get_av_num_neighbors(0), w.get_av_cell_id(0))
    };
    let org_x = own_cell % worldx;
    let org_y = own_cell / worldx;
    if num_neighbors == 0 {
        return Ok(false);
    }

    let mut max_x = org_x + 5;
    let mut min_x = org_x - 5;
    let mut max_y = org_y + 5;
    let mut min_y = org_y - 5;
    let mut facing = -1;
    let mut diagonal = false;
    if faced {
        facing = if ua != 0 {
            w.get_av_facing(0)
        } else {
            w.get_faced_dir()
        };
        match facing {
            0 => max_y = org_y,
            4 => min_y = org_y,
            2 => min_x = org_x,
            6 => max_x = org_x,
            _ => diagonal = true,
        }
    }
    if max_x >= worldx - 1 {
        max_x = worldx - 1;
    }
    if min_x <= 0 {
        min_x = 0;
    }
    if max_y >= worldy - 1 {
        max_y = worldy - 1;
    }
    if min_y <= 0 {
        min_y = 0;
    }

    let x_dist = max_x - min_x + 1;
    let y_dist = max_y - min_y + 1;
    let ul = min_y * worldx + min_x;
    let mut prey_count = 0;
    let mut pred_count = 0;
    for i in 0..x_dist {
        for j in 0..y_dist {
            let cellid = ul + i + j * worldx;
            if diagonal {
                let x_offset = (cellid % worldx) - org_x;
                let y_offset = (cellid / worldx) - org_y;
                let skip = match facing {
                    1 => y_offset > x_offset,
                    5 => x_offset > y_offset,
                    3 => {
                        (x_offset < 0 && (x_offset.abs() > y_offset.abs() || x_offset == y_offset))
                            || (y_offset < 0 && y_offset.abs() > x_offset.abs())
                    }
                    7 => {
                        (x_offset > 0 && (x_offset.abs() > y_offset.abs() || x_offset == y_offset))
                            || (y_offset > 0 && y_offset.abs() > x_offset.abs())
                    }
                    _ => false,
                };
                if skip {
                    continue;
                }
            }
            if ua == 0 {
                if cellid == own_cell {
                    continue;
                }
                // `IsOccupied() && !GetOrganism()->IsDead()`
                let mut occupant = None;
                w.with_organism_in_cell(cellid, &mut |o: &mut Organism| {
                    occupant = Some((o.state.is_dead, is_prey_ft(&o.state)));
                });
                match occupant {
                    Some((false, true)) => prey_count += 1,
                    Some((false, false)) => pred_count += 1,
                    _ => {}
                }
            } else {
                let (num_pred, num_prey) = w.cell_av_counts(cellid);
                prey_count += num_prey;
                pred_count += num_pred;
                if cellid == own_cell {
                    if is_prey_ft(org) {
                        prey_count -= 1;
                    } else {
                        pred_count -= 1;
                    }
                }
            }
        }
    }

    let reg_used = hw.find_modified_register(R_BX);
    hw.set_iv(reg_used, prey_count, true, false, false);
    let r2 = hw.find_modified_next_register(reg_used);
    hw.set_iv(r2, pred_count, true, false, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Collecting and depositing
// ----------------------------------------------------------------------

/// `DoActualCollect(ctx, bin_used, unit)` (`fake`: `FakeActualCollect`,
/// which takes the resource without storing it).
fn do_actual_collect(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    bin_used: i32,
    unit: bool,
    fake: bool,
    method: &str,
) -> Result<bool, String> {
    let info = res_info(w);
    let size = info.len();
    let idx = usize::try_from(bin_used)
        .ok()
        .filter(|&i| i < size)
        .ok_or_else(|| {
            unsupported(
                &format!("collecting resource {bin_used}, not in the library (undefined in C++)"),
                method,
            )
        })?;
    let cell_res = here_res_val(hw, org, w, bin_used);
    let mut res_change = vec![0.0; size];
    let total = org.rbins_total();
    let cfg = w.config();
    let max = cfg.max_total_stored;
    let has_max = max > 0.0;
    let mut res_consumed = if unit {
        let threshold = info[idx].1;
        if cell_res >= threshold {
            threshold
        } else {
            return Ok(false);
        }
    } else {
        cell_res * cfg.absorb_resource_fraction
    };
    if has_max && res_consumed + total >= max {
        res_consumed = max - total;
    }
    res_change[idx] = -res_consumed;
    if res_consumed > 0.0 {
        if !fake {
            org.add_to_rbin(bin_used, res_consumed);
        }
        update_res(hw, org, w, &res_change);
        return Ok(true);
    }
    Ok(false)
}

/// The edible resource `MULTI_ABSORB_TYPE` picks (1: first, 2: last) with
/// at least a threshold here, and the level read last; `None` for another
/// absorb type.
fn find_edible(hw: &Hw, org: &mut OrgState, w: W<'_>) -> Option<(i32, f64)> {
    let info = res_info(w);
    let edible = |h: i32| h == 0 || h > 5;
    let mut cell_res = 0.0;
    let order: Vec<usize> = match w.config().multi_absorb_type {
        1 => (0..info.len()).collect(),
        2 => (0..info.len()).rev().collect(),
        _ => return None,
    };
    for i in order {
        if edible(info[i].0) {
            cell_res = here_res_val(hw, org, w, i as i32);
            if cell_res >= info[i].1 {
                return Some((i as i32, cell_res));
            }
        }
    }
    Some((-1, cell_res))
}

/// `Inst_CollectEdible` / `Inst_Nop2CollectEdible` (`fake`): collect a
/// unit of the first (or last) edible resource here. Another
/// `MULTI_ABSORB_TYPE` reports a driver error (`Feedback().Error`, to
/// stdout) and collects nothing.
fn collect_edible(hw: &mut Hw, org: &mut OrgState, w: W<'_>, fake: bool) -> R {
    let method = if fake {
        "Inst_Nop2CollectEdible"
    } else {
        "Inst_CollectEdible"
    };
    let Some((res_id, _)) = find_edible(hw, org, w) else {
        w.cout("error: Instruction collect-edible only operational for MULTI_ABSORB_TYPE 1 or 2\n");
        return Ok(false);
    };
    if res_id != -1 {
        return do_actual_collect(hw, org, w, res_id, true, fake, method);
    }
    Ok(false)
}

/// `Inst_NopCollectEdible`: whether `collect-edible` would collect.
fn inst_nop_collect_edible(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let Some((res_id, cell_res)) = find_edible(hw, org, w) else {
        return Ok(false);
    };
    if res_id != -1 {
        let total = org.rbins_total();
        let max = w.config().max_total_stored;
        let has_max = max > 0.0;
        let threshold = w
            .environment()
            .resource_lib()
            .resource(res_id as usize)
            .threshold;
        let mut res_consumed = if cell_res >= threshold {
            threshold
        } else {
            return Ok(false);
        };
        if has_max && res_consumed + total >= max {
            res_consumed = max - total;
        }
        if res_consumed > 0.0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `Inst_CollectSpecific`: collect the `COLLECT_SPECIFIC_RESOURCE`; ?BX? =
/// the amount gained, the next register = the amount stored.
fn inst_collect_specific(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let resource = w.config().collect_specific_resource;
    let res_before = org.rbin(resource);
    let success = do_actual_collect(hw, org, w, resource, false, false, "Inst_CollectSpecific")?;
    let res_after = org.rbin(resource);
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, (res_after - res_before) as i32, true, false, false);
    let r2 = hw.find_modified_next_register(out_reg);
    hw.set_iv(r2, res_after as i32, true, false, false);
    Ok(success)
}

/// The first den (habitat 4) here above its threshold.
fn on_deposit_den(hw: &Hw, org: &mut OrgState, w: W<'_>) -> bool {
    let info = res_info(w);
    for (i, &(habitat, threshold, _)) in info.iter().enumerate() {
        if habitat == 4 && here_res_val(hw, org, w, i as i32) > threshold {
            return true;
        }
    }
    false
}

/// Deposit `amount` of stored resource `id` into this cell's resource
/// `id` (the body of the deposit loops).
fn deposit(
    hw: &Hw,
    org: &mut OrgState,
    w: W<'_>,
    id: i32,
    amount: i32,
    method: &str,
) -> Result<(), String> {
    let size = w.environment().resource_lib().size() as usize;
    org.add_to_rbin(id, f64::from(amount.wrapping_neg()));
    let mut res_change = vec![0.0; size];
    match usize::try_from(id).ok().and_then(|i| res_change.get_mut(i)) {
        Some(v) => *v = f64::from(amount),
        None => {
            return Err(unsupported(
                &format!("depositing resource {id}, not in the library (undefined in C++)"),
                method,
            ))
        }
    }
    update_res(hw, org, w, &res_change);
    Ok(())
}

/// `Inst_DepositResource` / `Inst_NopDepositResource` (`nop`): deposit
/// |next register| of the stored resource ?BX? on a den.
fn deposit_resource(hw: &mut Hw, org: &mut OrgState, w: W<'_>, nop: bool) -> R {
    let method = if nop {
        "Inst_NopDepositResource"
    } else {
        "Inst_DepositResource"
    };
    let amount_reg = hw.find_modified_next_register(R_BX);
    let raw_amount = hw.reg(amount_reg);
    let mut resource_amount = raw_amount.wrapping_abs();
    let id_reg = hw.find_modified_register(R_BX);
    let mut resource_id = hw.reg(id_reg);
    let num_bins = org.rbins().len() as i32;
    if num_bins == 0 {
        return Err(unsupported(
            "no resource bins (the C++ divides by zero)",
            method,
        ));
    }
    resource_id %= num_bins;
    let stored_res = bin_at(org.rbins(), resource_id, method)?;
    // QUIRK: the reference binary converts `abs(amount)` to double as an
    // unsigned value (`ucvtf`), so INT_MIN compares as 2^31 and is cut
    // down to the stored amount.
    if f64::from(raw_amount.unsigned_abs()) > stored_res {
        resource_amount = stored_res as i32;
    }
    let mut success = false;
    if stored_res >= f64::from(resource_amount) && resource_amount > 0 && on_deposit_den(hw, org, w)
    {
        if !nop {
            deposit(hw, org, w, resource_id, resource_amount, method)?;
        }
        success = true;
    }
    if nop {
        return Ok(success);
    }
    let res_after = org.rbin(resource_id);
    let out_reg = hw.find_modified_register(R_BX);
    if success {
        hw.set_iv(out_reg, res_after as i32, true, false, false);
    } else if resource_amount > 0 {
        hw.set_iv(out_reg, -1, true, false, false);
    }
    Ok(success)
}

/// `Inst_DepositSpecific` / `Inst_NopDepositSpecific` (`nop`): deposit ?BX?
/// of the `COLLECT_SPECIFIC_RESOURCE` on a den.
fn deposit_specific(hw: &mut Hw, org: &mut OrgState, w: W<'_>, nop: bool) -> R {
    let method = if nop {
        "Inst_NopDepositSpecific"
    } else {
        "Inst_DepositSpecific"
    };
    let r = hw.find_modified_register(R_BX);
    let mut resource_amount = hw.reg(r);
    let spec_res = w.config().collect_specific_resource;
    let stored_spec = bin_at(org.rbins(), spec_res, method)?;
    if f64::from(resource_amount) > stored_spec {
        resource_amount = stored_spec as i32;
    }
    let mut success = false;
    if stored_spec >= f64::from(resource_amount)
        && resource_amount > 0
        && on_deposit_den(hw, org, w)
    {
        if !nop {
            deposit(hw, org, w, spec_res, resource_amount, method)?;
        }
        success = true;
    }
    if nop {
        return Ok(success);
    }
    let res_after = org.rbin(spec_res);
    let out_reg = hw.find_modified_register(R_BX);
    if success {
        hw.set_iv(out_reg, res_after as i32, true, false, false);
    } else if resource_amount > 0 {
        hw.set_iv(out_reg, -1, true, false, false);
    }
    Ok(success)
}

/// The three `DepositAllAsSpecific` variants.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DepositAll {
    /// `Inst_DepositAllAsSpecific`
    Real,
    /// `Inst_NopDepositAllAsSpecific`: only tests.
    Nop,
    /// `Inst_Nop2DepositAllAsSpecific`: empties the bins, deposits nothing.
    Nop2,
}

/// `Inst_DepositAllAsSpecific` and its nop variants: on a den, empty every
/// bin into this cell's `COLLECT_SPECIFIC_RESOURCE`; ?BX? = 1 or -1.
fn deposit_all_as_specific(hw: &mut Hw, org: &mut OrgState, w: W<'_>, kind: DepositAll) -> R {
    let method = match kind {
        DepositAll::Real => "Inst_DepositAllAsSpecific",
        DepositAll::Nop => "Inst_NopDepositAllAsSpecific",
        DepositAll::Nop2 => "Inst_Nop2DepositAllAsSpecific",
    };
    let spec_res = w.config().collect_specific_resource;
    let size = w.environment().resource_lib().size();
    let mut success = false;
    if on_deposit_den(hw, org, w) {
        let mut total_deposit = 0.0;
        for j in 0..size {
            let resource_amount = bin_at(org.rbins(), j, method)?;
            if kind != DepositAll::Nop {
                org.add_to_rbin(j, -resource_amount);
            }
            // QUIRK: the nop2 variant drains the bins without adding them up.
            if kind != DepositAll::Nop2 {
                total_deposit += resource_amount;
            }
        }
        if total_deposit > 0.0 {
            if kind != DepositAll::Nop {
                let mut res_change = vec![0.0; size as usize];
                match usize::try_from(spec_res)
                    .ok()
                    .and_then(|i| res_change.get_mut(i))
                {
                    Some(v) => *v = total_deposit,
                    None => {
                        return Err(unsupported(
                            "COLLECT_SPECIFIC_RESOURCE not in the library (undefined in C++)",
                            method,
                        ))
                    }
                }
                update_res(hw, org, w, &res_change);
                org.num_deposits = org.num_deposits.wrapping_add(1);
                org.amount_deposited += total_deposit;
            }
            success = true;
        }
    }
    if kind == DepositAll::Nop {
        return Ok(success);
    }
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, if success { 1 } else { -1 }, true, false, false);
    Ok(success)
}

/// `Inst_GetResStored`: ?BX? = the stored amount of resource |?BX?|.
fn inst_get_res_stored(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    let r = hw.find_modified_register(R_BX);
    let mut resource_id = hw.reg(r).wrapping_abs();
    let bins = org.rbins().to_vec();
    if bins.is_empty() {
        return Err(unsupported(
            "no resource bins (the C++ divides by zero)",
            "Inst_GetResStored",
        ));
    }
    resource_id %= bins.len() as i32;
    let out_reg = hw.find_modified_register(R_BX);
    let v = bin_at(&bins, resource_id, "Inst_GetResStored")?;
    hw.set_iv(out_reg, v as i32, true, false, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Messages
// ----------------------------------------------------------------------

/// `cOrganism::SendMessage(ctx, msg)`: test outputs first under
/// `CHECK_TASK_ON_SEND`, then hand the message to the faced organism.
fn org_send_message(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    msg: &mut OrgMessage,
) -> Result<bool, String> {
    if w.config().check_task_on_send {
        hw.org_do_output(org, w, Some(msg.data as i32), false)?;
    }
    w.set_sender_recv_len(org.msg_received.len());
    let sent = w.send_message(msg);
    deliver_to_self(hw, org, w)?;
    if sent {
        message_sent(org, w, msg);
        return Ok(true);
    }
    Ok(false)
}

/// `Inst_SendMessage` (`SendMessage(ctx, 0)`): label ?BX?, data the next
/// register.
fn inst_send_message(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let label_reg = hw.find_modified_register(R_BX);
    let data_reg = Hw::find_next_register(label_reg);
    let mut msg = new_message(org, w, 0);
    msg.label = hw.reg(label_reg) as u32;
    msg.data = hw.reg(data_reg) as u32;
    org_send_message(hw, org, w, &mut msg)
}

/// `Inst_RetrieveMessage`: ?BX? = label, the next register = data of the
/// oldest received message.
fn inst_retrieve_message(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let Some(msg) = org_retrieve_message(org) else {
        return Ok(false);
    };
    let label_reg = hw.find_modified_register(R_BX);
    let data_reg = Hw::find_next_register(label_reg);
    hw.set_iv(label_reg, msg.label as i32, false, false, true);
    hw.set_iv(data_reg, msg.data as i32, false, false, true);
    if w.config().net_log_retmessages != 0 {
        w.stats_log_ret_message(&msg);
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Groups
// ----------------------------------------------------------------------

/// `Inst_JoinGroup`: join the group in ?BX?.
fn inst_join_group(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let r = hw.find_modified_register(R_BX);
    let prop_group_id = hw.reg(r);
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

/// `Inst_ChangePredGroup`: by the nop, make (?AX?), leave (?BX?) or adopt
/// (?CX?) a predator group.
fn inst_change_pred_group(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if is_prey_ft(org) || w.config().use_form_groups != 1 {
        return Ok(false);
    }
    if !hw.next_is_nop() {
        return Ok(false);
    }
    let nop_reg = hw.find_modified_register(R_BX);
    match nop_reg {
        R_AX => inst_make_pred_group(hw, org, w),
        R_BX => inst_leave_pred_group(hw, org, w),
        R_CX => inst_adopt_pred_group(hw, org, w),
        _ => Ok(false),
    }
}

/// `Inst_MakePredGroup`: a predator founds (and joins) a new group.
fn inst_make_pred_group(_hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if is_prey_ft(org) || w.config().use_form_groups != 1 {
        return Ok(false);
    }
    if org.has_opinion() {
        let group = org.get_opinion().0;
        w.leave_group(org, group);
    }
    w.make_group(org);
    Ok(true)
}

/// `Inst_LeavePredGroup`: a predator joins the nomads (group -3).
fn inst_leave_pred_group(_hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const NOMAD_GROUP: i32 = -3;
    if is_prey_ft(org) || w.config().use_form_groups != 1 {
        return Ok(false);
    }
    if org.has_opinion() {
        let group = org.get_opinion().0;
        if group == NOMAD_GROUP {
            return Ok(false);
        }
        w.leave_group(org, group);
    }
    let update = w.update();
    org.set_opinion(w.config(), update, NOMAD_GROUP)?;
    w.join_group(org, NOMAD_GROUP);
    Ok(true)
}

/// `Inst_AdoptPredGroup`: try to immigrate into the (non-empty) group that
/// freshly marked the faced cell.
fn inst_adopt_pred_group(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if is_prey_ft(org) || w.config().use_form_groups != 1 {
        return Ok(false);
    }
    let ua = hw.use_avatar;
    let mut prop_group_id = w.get_faced_cell_data_territory();
    if ua == 2 {
        prop_group_id = w.get_av_faced_data_territory(0);
    }
    if prop_group_id == -1 {
        return Ok(false);
    }
    let current_update = w.update();
    let update_marked = w.get_faced_cell_data_update();
    if ua == 2 {
        // QUIRK: the avatar's marking update overwrites the group id.
        prop_group_id = w.get_av_faced_data_update(0);
    }
    let expire_window = w.config().marking_expire_date;
    if current_update > update_marked.wrapping_add(expire_window) {
        return Ok(false);
    }
    if org.has_opinion() && org.get_opinion().0 == prop_group_id {
        return Ok(false);
    }
    if w.number_of_organisms_in_group(prop_group_id) == 0 {
        return Ok(false);
    }
    w.attempt_immigrate_group(prop_group_id, org);
    Ok(true)
}

/// `Inst_GetFacedOrgID`: ?BX? = the id of the living faced organism (the
/// organism itself when it faces its own cell or its own avatar).
fn inst_get_faced_org_id(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_av(0)) {
        return Ok(false);
    }
    let info = faced_org_info(org, w, ua);
    let Some((dead, id)) = info else {
        return Err(unsupported(
            "no organism behind an occupied faced cell or avatar (null in C++)",
            "Inst_GetFacedOrgID",
        ));
    };
    if dead {
        return Ok(false);
    }
    let out_reg = hw.find_modified_register(R_BX);
    hw.set_iv(out_reg, id, true, false, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Attacks and fights
// ----------------------------------------------------------------------

/// `sAttackResult`: the unsigned bitfields `inst:1` (solo or group attack
/// instruction), `share:2` (no, yes, fake), `success:2` (0 success, 1 no
/// prey, 2 no friends, 3 chance failure) and `size:3`.
#[derive(Clone, Copy)]
struct AttackResult {
    inst: i32,
    share: i32,
    success: i32,
    size: i32,
}

impl AttackResult {
    const fn new(inst: i32, share: i32) -> Self {
        AttackResult {
            inst,
            share,
            success: 0,
            size: 0,
        }
    }

    /// `results.size = n`. QUIRK: the three-bit field keeps `n & 7`, so a
    /// pack of eight reads as 0 (a "no friends" failure) and one of nine as
    /// 1.
    fn set_size(&mut self, n: usize) {
        self.size = (n & 7) as i32;
    }
}

/// `TestAttack(ctx)`: is there prey in front, outside a refuge?
fn test_attack(hw: &Hw, org: &mut OrgState, w: W<'_>) -> bool {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return false;
    }
    if w.config().pred_prey_switch < 0 {
        return false;
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_prey_av(0)) {
        return false;
    }
    let n = if ua == 0 {
        w.get_num_neighbors()
    } else {
        w.get_av_num_neighbors(0)
    };
    if n == 0 {
        return false;
    }
    // prevent killing on refuges
    let info = res_info(w);
    for (i, &(_, _, refuge)) in info.iter().enumerate() {
        if refuge != 0
            && ((ua == 0 && w.get_faced_resource_val(org, i as i32) > 0.0)
                || (ua == 2 && w.get_av_faced_resource_val(org, i as i32, 0) > 0.0))
        {
            return false;
        }
    }
    true
}

/// `TestAttackPred(ctx)`: is there a predator in front?
fn test_attack_pred(hw: &Hw, w: W<'_>) -> bool {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return false;
    }
    if w.config().pred_prey_switch < 0 {
        return false;
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_pred_av(0)) {
        return false;
    }
    true
}

/// `TestAttackResultsOut(results)`: log the attempt (`TRACK_GROUP_ATTACK_
/// DETAILS`), count it, and count a kill when it succeeded.
fn attack_results_out(org: &mut OrgState, w: W<'_>, r: AttackResult) -> bool {
    match w.config().track_group_attack_details {
        1 => {
            let s = format!("{},{},{},{}", r.size, r.success, r.share, r.inst);
            w.try_write_group_attack_string(&s);
        }
        2 => {
            let raw_bits = (((((r.inst << 2) | r.share) << 2) | r.success) << 3) | r.size;
            w.try_write_group_attack_bits(raw_bits as u8);
        }
        _ => {}
    }
    org.phenotype.cur_attacks = org.phenotype.cur_attacks.wrapping_add(1);
    if r.success == 0 {
        org.phenotype.cur_kills = org.phenotype.cur_kills.wrapping_add(1);
        return true;
    }
    false
}

/// An organism an attack or fight acts on (a `cOrganism*`): the running
/// organism, whose state the hardware holds (`Inst_AttackPoisonFTMixedPrey`
/// may pick it, and every pack starts with it), or another living
/// organism, known by the cell it lives in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tgt {
    Me,
    Other { uid: OrgUid, cell: i32 },
}

/// The organism `uid` as a [`Tgt`].
fn tgt(org: &OrgState, w: W<'_>, uid: OrgUid) -> Option<Tgt> {
    if uid == org.uid {
        return Some(Tgt::Me);
    }
    w.seen_org(uid).map(|s| Tgt::Other {
        uid,
        cell: s.cell_id,
    })
}

/// [`tgt`] of an organism the C++ holds a pointer to; `Err` where that
/// pointer is NULL (`uid` is `None`). [`tgt`] maps the running organism to
/// [`Tgt::Me`], so a live organism the population does not hold is an
/// unreachable invariant.
fn tgt_of(org: &OrgState, w: W<'_>, uid: Option<OrgUid>, method: &str) -> Result<Tgt, String> {
    uid.and_then(|u| tgt(org, w, u)).ok_or_else(|| {
        unsupported(
            "an attack or fight target that is NULL (the C++ dereferences it)",
            method,
        )
    })
}

/// `GetNeighbor()`: the organism in the faced cell.
fn neighbor_uid(w: W<'_>) -> Option<OrgUid> {
    let faced = w.get_faced_cell_id();
    w.cell_organism_uid(faced)
}

/// Read the target's state.
fn t_get<T>(org: &OrgState, w: W<'_>, t: Tgt, f: impl Fn(&OrgState) -> T) -> Result<T, String> {
    match t {
        Tgt::Me => Ok(f(org)),
        Tgt::Other { uid, .. } => {
            let mut out = None;
            w.with_organism(uid, &mut |o: &mut Organism| out = Some(f(&o.state)));
            out.ok_or_else(|| "attack target left the population (C++: dangling cOrganism*)".into())
        }
    }
}

/// Change the target's state.
fn t_mut(
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    f: &mut dyn FnMut(&mut OrgState),
) -> Result<(), String> {
    match t {
        Tgt::Me => {
            f(org);
            Ok(())
        }
        Tgt::Other { uid, .. } => {
            if w.with_organism(uid, &mut |o: &mut Organism| f(&mut o.state)) {
                Ok(())
            } else {
                Err("attack target left the population (C++: dangling cOrganism*)".into())
            }
        }
    }
}

/// `target->UpdateMerit(ctx, new_merit)`. With `new_merit <= 0`
/// `cPopulation::UpdateMerit` kills the organism and then dereferences its
/// emptied cell (a crash), so the port stops.
fn t_update_merit(
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    new_merit: f64,
    method: &str,
) -> Result<(), String> {
    if new_merit <= 0.0 {
        return Err(unsupported(
            &format!(
                "a merit of {new_merit} after an attack (C++: cPopulation::UpdateMerit kills \
                 the organism, then dereferences its empty cell)"
            ),
            method,
        ));
    }
    match t {
        Tgt::Me => {
            w.update_merit(org, new_merit);
        }
        Tgt::Other { cell, .. } => {
            w.update_cell_merit(org, cell, new_merit);
        }
    }
    Ok(())
}

/// `target->Die(ctx)`
fn t_die(org: &mut OrgState, w: W<'_>, t: Tgt) {
    match t {
        Tgt::Me => org.die(w),
        Tgt::Other { cell, .. } => w.kill_cell_id(org, cell),
    }
}

/// `target->GetOrgInterface().GetCellID()`
fn t_cell(w: W<'_>, t: Tgt) -> i32 {
    match t {
        Tgt::Me => w.get_cell_id(),
        Tgt::Other { cell, .. } => cell,
    }
}

/// `(GetForageTarget(), IsDead(), HasOpinion() ? GetOpinion().first)`.
fn t_info(org: &OrgState, w: W<'_>, t: Tgt) -> Result<(i32, bool, Option<i32>), String> {
    t_get(org, w, t, |s| {
        let opinion = s.opinions.as_ref().and_then(|l| l.last()).map(|o| o.0);
        (s.forage_target, s.is_dead, opinion)
    })
}

/// `GetPreyTarget(ctx)`: the faced organism, or with avatars the first
/// faced prey avatar.
fn get_prey_target(hw: &Hw, org: &OrgState, w: W<'_>, method: &str) -> Result<Tgt, String> {
    let uid = if hw.use_avatar == 0 {
        neighbor_uid(w)
    } else {
        w.get_rand_faced_prey_av(0)
    };
    tgt_of(org, w, uid, method)
}

/// `TestPreyTarget(target)`: "attacking other carnivores is handled
/// differently (e.g. using fights or tolerance)".
fn t_is_prey_target(org: &OrgState, w: W<'_>, t: Tgt) -> Result<bool, String> {
    t_get(org, w, t, |s| s.forage_target > -2 && !s.is_dead)
}

/// What an attack reads of its target (`cHardwareBCR`'s attacks): another
/// organism of the population, or the attacker itself when it faces its
/// own cell (a torus one cell wide or high) or its own avatar. The values
/// are read when the target is picked; each is used before the attack
/// changes it, so for the attacker itself they are what the C++ reads
/// through the pointer at the time.
pub(crate) struct Victim {
    pub(crate) uid: OrgUid,
    pub(crate) cell_id: i32,
    pub(crate) merit: f64,
    pub(crate) cur_bonus: f64,
    pub(crate) last_reaction_count: Vec<i32>,
    pub(crate) rbins: Vec<f64>,
    pub(crate) forage_target: i32,
    pub(crate) is_dead: bool,
}

/// The living organism `uid` as a [`Victim`] (the attacker `org` itself
/// when `uid` is its own).
pub(crate) fn victim(org: &OrgState, w: W<'_>, uid: OrgUid) -> Option<Victim> {
    let snapshot = |st: &OrgState, cell_id: i32| Victim {
        uid,
        cell_id,
        merit: st.phenotype.merit.get_double(),
        cur_bonus: st.phenotype.cur_bonus,
        last_reaction_count: st.phenotype.last_reaction_count.clone(),
        rbins: st.phenotype.cur_rbins_avail.clone(),
        forage_target: st.forage_target,
        is_dead: st.is_dead,
    };
    if uid == org.uid {
        return Some(snapshot(org, w.get_cell_id()));
    }
    let cell_id = w.seen_org(uid)?.cell_id;
    let mut v = None;
    w.with_organism(uid, &mut |o: &mut Organism| {
        v = Some(snapshot(&o.state, cell_id))
    });
    v
}

/// Apply `f` to the victim's state (`org`'s when the victim is the
/// attacker itself).
pub(crate) fn with_victim(
    org: &mut OrgState,
    w: W<'_>,
    target: &Victim,
    f: &mut dyn FnMut(&mut OrgState),
) {
    if target.uid == org.uid {
        f(org);
        return;
    }
    w.with_organism(target.uid, &mut |o: &mut Organism| f(&mut o.state));
}

/// `target->Die(ctx)` of a [`Victim`].
pub(crate) fn victim_die(org: &mut OrgState, w: W<'_>, target: &Victim) {
    if target.uid == org.uid {
        org.die(w);
    } else {
        w.kill_cell_id(org, target.cell_id);
    }
}

/// `TestPreyTarget(target)` of a [`Victim`].
pub(crate) fn test_prey_target(target: &Victim) -> bool {
    target.forage_target > -2 && !target.is_dead
}

/// `sAttackReg` as `SetAttackReg(reg)` fills it.
#[derive(Clone, Copy)]
struct AttackReg {
    success_reg: usize,
    bonus_reg: usize,
    bin_reg: usize,
}

/// `SetAttackReg(reg)`
fn set_attack_reg(hw: &mut Hw) -> AttackReg {
    let success_reg = hw.find_modified_register(R_BX);
    let bonus_reg = hw.find_modified_next_register(success_reg);
    let bin_reg = hw.find_modified_next_register(bonus_reg);
    AttackReg {
        success_reg,
        bonus_reg,
        bin_reg,
    }
}

/// `setInternalValue(reg, value, true)`
fn set_env(hw: &mut Hw, reg: usize, value: i32) {
    hw.set_iv(reg, value, true, false, false);
}

/// `TestAttackChance(ctx, target, reg, odds)` (`odds` -1: `PRED_ODDS`): a
/// failed attack injures the target.
fn test_attack_chance(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    odds: f64,
    method: &str,
) -> R {
    let odds = if odds == -1.0 {
        w.config().pred_odds
    } else {
        odds
    };
    let min_prey = w.config().min_prey;
    if w.rng().get_double() >= odds || (min_prey > 0 && w.stats_num_prey_creatures()? <= min_prey) {
        t_injure(org, w, t, method)?;
        set_env(hw, reg.success_reg, -1);
        set_env(hw, reg.bonus_reg, -1);
        if w.config().use_resource_bins {
            set_env(hw, reg.bin_reg, -1);
        }
        return Ok(false);
    }
    Ok(true)
}

/// `InjureOrg(ctx, target)`: a failed attack costs the target
/// `PRED_INJURY` of its merit, reactions, bonus and resource bins.
fn t_injure(org: &mut OrgState, w: W<'_>, t: Tgt, method: &str) -> Result<(), String> {
    let injury = w.config().pred_injury;
    if injury == 0.0 {
        return Ok(());
    }
    if w.config().merit_inc_apply_immediate {
        let target_merit = t_get(org, w, t, |s| s.phenotype.merit.get_double())?;
        // `target_merit -= target_merit * injury`, one fused rounding
        t_update_merit(
            org,
            w,
            t,
            (-target_merit).mul_add(injury, target_merit),
            method,
        )?;
    }
    let use_bins = w.config().use_resource_bins;
    t_mut(org, w, t, &mut |st: &mut OrgState| {
        injure_state(st, injury, use_bins)
    })
}

/// The state half of `InjureOrg`.
fn injure_state(st: &mut OrgState, injury: f64, use_bins: bool) {
    // QUIRK: the current counts are set from the last update's.
    let target_reactions = st.phenotype.last_reaction_count.clone();
    for (i, &r) in target_reactions.iter().enumerate() {
        if let Some(c) = st.phenotype.cur_reaction_count.get_mut(i) {
            *c = r.wrapping_sub((f64::from(r) * injury) as i32);
        }
    }
    let target_bonus = st.phenotype.cur_bonus;
    st.phenotype.cur_bonus = (-target_bonus).mul_add(injury, target_bonus);
    if use_bins {
        let target_bins = st.phenotype.cur_rbins_avail.clone();
        for (i, b) in target_bins.iter().enumerate() {
            st.add_to_rbin(i as i32, -(b * injury));
        }
    }
}

/// `InjureOrg(ctx, target)` on a [`Victim`] (`cHardwareBCR::injureOrg`).
pub(crate) fn injure_org(
    org: &mut OrgState,
    w: W<'_>,
    target: &Victim,
    method: &str,
) -> Result<(), String> {
    let injury = w.config().pred_injury;
    if injury == 0.0 {
        return Ok(());
    }
    if w.config().merit_inc_apply_immediate {
        let target_merit = (-target.merit).mul_add(injury, target.merit);
        if target_merit <= 0.0 {
            return Err(format!(
                "unsupported: a merit of {target_merit} after an attack (C++: \
                 cPopulation::UpdateMerit kills the organism, then dereferences its empty \
                 cell; {method})"
            ));
        }
        w.update_cell_merit(org, target.cell_id, target_merit);
    }
    let use_bins = w.config().use_resource_bins;
    with_victim(org, w, target, &mut |st: &mut OrgState| {
        injure_state(st, injury, use_bins)
    });
    Ok(())
}

/// `GetRBins()[COLLECT_SPECIFIC_RESOURCE]` of the attacker into the bin
/// register.
fn set_spec_bin(
    hw: &mut Hw,
    org: &OrgState,
    w: W<'_>,
    reg: AttackReg,
    method: &str,
) -> Result<(), String> {
    let spec = w.config().collect_specific_resource;
    let spec_bin = bin_at(&org.phenotype.cur_rbins_avail, spec, method)? as i32;
    set_env(hw, reg.bin_reg, spec_bin);
    Ok(())
}

/// `ApplyKilledPreyMerit(ctx, target, effic)`: "add prey's merit to
/// predator's--this will result in immediately applying merit increases".
fn apply_killed_prey_merit(
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    effic: f64,
    method: &str,
) -> Result<(), String> {
    if w.config().merit_inc_apply_immediate {
        let target_merit = t_get(org, w, t, |s| s.phenotype.merit.get_double())?;
        let attacker_merit = target_merit.mul_add(effic, org.phenotype.merit.get_double());
        t_update_merit(org, w, Tgt::Me, attacker_merit, method)?;
    }
    Ok(())
}

/// `ApplyKilledPreyReactions(target)`: "add on the victims reaction counts
/// to your own, this will allow you to pass any reaction tests".
fn apply_killed_prey_reactions(org: &mut OrgState, w: W<'_>, t: Tgt) -> Result<(), String> {
    let target_reactions = t_get(org, w, t, |s| s.phenotype.last_reaction_count.clone())?;
    for (i, s) in org
        .phenotype
        .cur_stolen_reaction_count
        .iter_mut()
        .enumerate()
    {
        *s = s.wrapping_add(target_reactions.get(i).copied().unwrap_or(0));
    }
    Ok(())
}

/// `ApplyKilledPreyBonus(target, reg, effic)`
fn apply_killed_prey_bonus(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    effic: f64,
) -> Result<(), String> {
    let target_bonus = t_get(org, w, t, |s| s.phenotype.cur_bonus)?;
    org.phenotype.cur_bonus = target_bonus.mul_add(effic, org.phenotype.cur_bonus);
    set_env(hw, reg.bonus_reg, target_bonus as i32);
    Ok(())
}

/// `ApplyKilledPreyResBins(target, reg, effic)`: the target loses what the
/// attacker gains only for a positive efficiency.
fn apply_killed_prey_res_bins(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    effic: f64,
    method: &str,
) -> Result<(), String> {
    if !w.config().use_resource_bins {
        return Ok(());
    }
    let target_bins = t_get(org, w, t, |s| s.phenotype.cur_rbins_avail.clone())?;
    for (i, &b) in target_bins.iter().enumerate() {
        org.add_to_rbin(i as i32, b * effic);
        if effic > 0.0 {
            t_mut(org, w, t, &mut |s: &mut OrgState| {
                s.add_to_rbin(i as i32, -(b * effic))
            })?;
        }
    }
    set_spec_bin(hw, org, w, reg, method)
}

/// `ApplySharedKilledPreyMerit`, `ApplySharedKilledPreyBonus` and
/// `ApplySharedKilledPreyResBins` for the pack member `member`: it gains
/// `share` of the prey's merit, bonus and bins (the prey loses all of the
/// bins' share each time); the registers get the share of the bonus and
/// the attacker's specific bin.
#[allow(clippy::too_many_arguments)]
fn apply_shared_killed_prey(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    effic: f64,
    member: Tgt,
    share: f64,
    method: &str,
) -> Result<(), String> {
    if w.config().merit_inc_apply_immediate {
        let target_merit = t_get(org, w, t, |s| s.phenotype.merit.get_double())? * share;
        let member_merit = t_get(org, w, member, |s| s.phenotype.merit.get_double())?;
        t_update_merit(
            org,
            w,
            member,
            target_merit.mul_add(effic, member_merit),
            method,
        )?;
    }
    let target_bonus = t_get(org, w, t, |s| s.phenotype.cur_bonus)? * share;
    t_mut(org, w, member, &mut |s: &mut OrgState| {
        s.phenotype.cur_bonus = target_bonus.mul_add(effic, s.phenotype.cur_bonus);
    })?;
    set_env(hw, reg.bonus_reg, target_bonus as i32);
    if w.config().use_resource_bins {
        let target_bins = t_get(org, w, t, |s| s.phenotype.cur_rbins_avail.clone())?;
        for (i, &b) in target_bins.iter().enumerate() {
            let bin_amt = b * effic;
            t_mut(org, w, member, &mut |s: &mut OrgState| {
                s.add_to_rbin(i as i32, bin_amt * share)
            })?;
            t_mut(org, w, t, &mut |s: &mut OrgState| {
                s.add_to_rbin(i as i32, -bin_amt)
            })?;
        }
        set_spec_bin(hw, org, w, reg, method)?;
    }
    Ok(())
}

/// `PRED_EFFICIENCY`, squared for a top predator.
fn attack_efficiency(org: &OrgState, w: W<'_>) -> f64 {
    let mut effic = w.config().pred_efficiency;
    if org.forage_target < -2 {
        effic *= effic;
    }
    effic
}

/// `ExecuteAttack(ctx, target, reg, odds)`: on success the attacker takes
/// the prey's merit, reactions, resource bins and bonus, the prey dies and
/// the attacker becomes a predator.
#[allow(clippy::too_many_arguments)]
fn execute_attack(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    odds: f64,
    method: &str,
) -> R {
    if !test_attack_chance(hw, org, w, t, reg, odds, method)? {
        return Ok(false);
    }
    let effic = attack_efficiency(org, w);
    apply_killed_prey_merit(org, w, t, effic, method)?;
    apply_killed_prey_reactions(org, w, t)?;
    // keep returns in same order as legacy code (important if reg
    // assignments are shared)
    apply_killed_prey_res_bins(hw, org, w, t, reg, effic, method)?;
    set_env(hw, reg.success_reg, 1);
    apply_killed_prey_bonus(hw, org, w, t, reg, effic)?;

    // kill first -- could end up being killed by inject clone or MAX_PRED
    // if parent was pred
    t_die(org, w, t);
    make_pred(hw, org, w)?;
    try_prey_clone(org, w)?;
    Ok(true)
}

/// `ExecuteShareAttack(ctx, target, reg, pack, odds)`: the pack shares
/// the prey ("reactions can't be shared").
#[allow(clippy::too_many_arguments)]
fn execute_share_attack(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    pack: &[Tgt],
    odds: f64,
    method: &str,
) -> R {
    if !test_attack_chance(hw, org, w, t, reg, odds, method)? {
        return Ok(false);
    }
    let effic = attack_efficiency(org, w);
    apply_killed_prey_reactions(org, w, t)?;
    let share = 1.0 / pack.len() as f64;
    for &member in pack {
        apply_shared_killed_prey(hw, org, w, t, reg, effic, member, share, method)?;
    }
    t_die(org, w, t);
    make_pred(hw, org, w)?;
    try_prey_clone(org, w)?;
    set_env(hw, reg.success_reg, 1);
    Ok(true)
}

/// `ExecuteFakeShareAttack(ctx, target, reg, share, odds)`: the attacker
/// alone takes its share of the prey.
#[allow(clippy::too_many_arguments)]
fn execute_fake_share_attack(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    share: f64,
    odds: f64,
    method: &str,
) -> R {
    if !test_attack_chance(hw, org, w, t, reg, odds, method)? {
        return Ok(false);
    }
    let effic = attack_efficiency(org, w);
    apply_killed_prey_reactions(org, w, t)?;
    apply_shared_killed_prey(hw, org, w, t, reg, effic, Tgt::Me, share, method)?;
    t_die(org, w, t);
    make_pred(hw, org, w)?;
    try_prey_clone(org, w)?;
    set_env(hw, reg.success_reg, 1);
    Ok(true)
}

/// `ExecutePoisonPreyAttack(ctx, target, reg, odds)`: poisonous prey
/// (forage target 2) costs merit (`PRED_EFFICIENCY_POISON`) and bonus
/// instead. `None` when the attack failed, else whether prey clones were
/// injected after the kill.
fn execute_poison_prey_attack(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    t: Tgt,
    reg: AttackReg,
    method: &str,
) -> Result<Option<bool>, String> {
    // poison affects merit
    if !test_attack_chance(hw, org, w, t, reg, -1.0, method)? {
        return Ok(None);
    }
    let poisonous = t_get(org, w, t, |s| s.forage_target == 2)?;
    let mut effic = if poisonous {
        w.config().pred_efficiency_poison
    } else {
        w.config().pred_efficiency
    };
    if org.forage_target < -2 {
        effic *= effic;
    }
    // apply poison, if any
    if poisonous && w.config().merit_inc_apply_immediate {
        let target_merit = t_get(org, w, t, |s| s.phenotype.merit.get_double())?;
        let attacker_merit = (-target_merit).mul_add(effic, org.phenotype.merit.get_double());
        // A merit poisoned to zero or below stops here (the C++ crashes in
        // `cPopulation::UpdateMerit` before `to_die` could kill the
        // attacker).
        t_update_merit(org, w, Tgt::Me, attacker_merit, method)?;
    } else {
        apply_killed_prey_merit(org, w, t, effic, method)?;
    }
    if !poisonous {
        apply_killed_prey_reactions(org, w, t)?;
    }
    if poisonous {
        effic = -effic;
    }
    apply_killed_prey_bonus(hw, org, w, t, reg, effic)?;
    apply_killed_prey_res_bins(hw, org, w, t, reg, effic, method)?;
    t_die(org, w, t);
    make_pred(hw, org, w)?;
    let cloned = try_prey_clone(org, w)?;
    set_env(hw, reg.success_reg, 1);
    Ok(Some(cloned))
}

/// `TryPreyClone(ctx)`: at or below `-MIN_PREY` prey, inject clones of up
/// to two random prey ("can't dump a lot b/c could end up filling world
/// with just clones"). Returns whether it injected any.
pub(crate) fn try_prey_clone(org: &mut OrgState, w: W<'_>) -> Result<bool, String> {
    let min_prey = w.config().min_prey;
    if min_prey >= 0 {
        return Ok(false);
    }
    let num_prey = w.stats_num_prey_creatures()?;
    let floor = min_prey.wrapping_abs();
    if num_prey > floor {
        return Ok(false);
    }
    let num_clones = floor.wrapping_sub(num_prey).min(2);
    for _ in 0..num_clones {
        w.inject_prey_clone(org);
    }
    Ok(num_clones > 0)
}

/// `m_organism->GetPhenotype().IncAttackedPreyFTData(ft)`: the count of
/// the target's forage target in `GetAttackPreyFTList()` (an id missing
/// from the list is used as the index itself; out of range the C++
/// writes out of bounds, and the port stops).
fn inc_attacked_prey_ft_data(
    org: &mut OrgState,
    w: W<'_>,
    target_ft: i32,
    method: &str,
) -> Result<(), String> {
    let target_list = w.environment().attack_prey_ft_list().to_vec();
    let ft = target_ft.max(-3);
    let index = target_list
        .iter()
        .position(|&t| t == ft)
        .map_or(ft, |i| i as i32);
    let counts = org.phenotype.cur_killed_targets.len();
    let size = if counts == 0 {
        target_list.len()
    } else {
        counts
    };
    if index < 0 || index as usize >= size {
        return Err(unsupported(
            &format!(
                "forage target {target_ft} outside the attack-prey target list (C++: \
                 out-of-bounds write in cPhenotype::IncAttackedPreyFTData)"
            ),
            method,
        ));
    }
    org.phenotype
        .inc_attacked_prey_ft_data(&target_list, target_ft);
    Ok(())
}

/// `target->GetForageTarget()` after `ExecutePoisonPreyAttack` killed the
/// target: the attacker's own when it attacked itself; for another
/// organism, deleted by then (a use after free), the value it had, unless
/// prey clones injected since may have reused its memory.
fn ft_after_kill(
    org: &OrgState,
    t: Tgt,
    ft_before: i32,
    cloned: bool,
    method: &str,
) -> Result<i32, String> {
    match t {
        Tgt::Me => Ok(org.forage_target),
        Tgt::Other { .. } if !cloned => Ok(ft_before),
        Tgt::Other { .. } => Err(unsupported(
            "the forage target of the killed prey after prey clones were injected (C++: read \
             after delete, IncAttackedPreyFTData(target->GetForageTarget()))",
            method,
        )),
    }
}

/// `UpdateGroupAttackStats(inst, results, get_size)`: count the attack by
/// the size of the predator pack around the attacker (`get_size`) or the
/// size the attack found.
fn update_group_attack_stats(
    hw: &Hw,
    org: &mut OrgState,
    w: W<'_>,
    inst: &str,
    results: AttackResult,
    get_size: bool,
    method: &str,
) -> Result<(), String> {
    let has_opinion = w.config().use_form_groups != 0 && org.has_opinion();
    let mut gr_pack_size = 0;
    let mut fr_pack_size;
    if get_size {
        if has_opinion {
            gr_pack_size = pred_pack(hw, org, w, true, method)?.len() as i32;
        }
        fr_pack_size = pred_pack(hw, org, w, false, method)?.len() as i32;
    } else if has_opinion {
        gr_pack_size = results.size;
        fr_pack_size = pred_pack(hw, org, w, false, method)?.len() as i32;
    } else {
        fr_pack_size = results.size;
    }
    if gr_pack_size > 8 {
        gr_pack_size = 9;
    }
    if fr_pack_size > 8 {
        fr_pack_size = 9;
    }
    let inst_set = hw.base.inst_set.inst_set_name().to_string();
    let Some(idx) = w.group_attack_inst_index(&inst_set, inst) else {
        return Err(format!(
            "{inst} is not a group attack instruction of {inst_set} (C++: assert in \
             cHardwareExperimental::UpdateGroupAttackStats)"
        ));
    };
    let ph = &mut org.phenotype;
    let rows = if org.forage_target == -2 {
        &mut ph.cur_group_attack_count
    } else if org.forage_target < -2 {
        &mut ph.cur_top_pred_group_attack_count
    } else {
        return Ok(());
    };
    let mut inc = |col: i32| {
        if let Some(c) = rows
            .get_mut(idx)
            .and_then(|r| r.get_mut(col.max(0) as usize))
        {
            *c += 1;
        }
    };
    inc(fr_pack_size - 1);
    if has_opinion {
        inc(gr_pack_size + 10 - 1);
    }
    Ok(())
}

/// `GetPredGroupAttackNeighbors()` (`same_group` false) or
/// `GetPredSameGroupAttackNeighbors()`: the attacker, then the living
/// predators around it (of its group): in the neighbouring cells, or with
/// avatars the predator avatars of the cells around its avatar.
fn pred_pack(
    hw: &Hw,
    org: &mut OrgState,
    w: W<'_>,
    same_group: bool,
    method: &str,
) -> Result<Vec<Tgt>, String> {
    let opinion = if same_group {
        Some(org.get_opinion().0)
    } else {
        None
    };
    let mut candidates: Vec<OrgUid> = Vec::new();
    if hw.use_avatar == 0 {
        for cell in w.get_neighborhood_cell_ids() {
            if let Some(uid) = w.cell_organism_uid(cell) {
                candidates.push(uid);
            }
        }
    } else {
        for cell in w.get_av_neighborhood_cell_ids(0) {
            let (num_pred, num_prey) = w.cell_av_counts(cell);
            if num_pred == 0 {
                continue;
            }
            // `GetCellInputAVs()`: the predator list (after the prey in
            // `GetCellAVs()`).
            let avs = w.get_cell_avs(cell, 0);
            candidates.extend(avs.iter().skip(num_prey.max(0) as usize));
        }
    }
    let mut pack = vec![Tgt::Me];
    for uid in candidates {
        let t = tgt_of(org, w, Some(uid), method)?;
        let (ft, dead, op) = t_info(org, w, t)?;
        if dead || ft > -2 {
            continue;
        }
        match opinion {
            Some(want) if op != Some(want) => {}
            _ => pack.push(t),
        }
    }
    Ok(pack)
}

/// `Inst_AttackPrey`: attack the faced prey (a random faced prey avatar)
/// and take its merit, bonus and reactions.
fn inst_attack_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPrey";
    let mut r = AttackResult::new(0, 0);
    if !test_attack(hw, org, w) {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let target = get_prey_target(hw, org, w, M)?;
    if !t_is_prey_target(org, w, target)? {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let reg = set_attack_reg(hw);
    if !execute_attack(hw, org, w, target, reg, -1.0, M)? {
        r.success = 3;
    } else {
        update_group_attack_stats(hw, org, w, "attack-prey", r, true, M)?;
    }
    Ok(attack_results_out(org, w, r))
}

/// `Inst_AttackPreyArea`: the more prey around (the attacker included),
/// the less likely the attack succeeds ("0 friend = 0% confusion, 1 friend
/// = 10%, 10 friends = 100%").
fn inst_attack_prey_area(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPreyArea";
    let mut r = AttackResult::new(0, 0);
    if !test_attack(hw, org, w) {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let mut prey_count = 0;
    if hw.use_avatar == 0 {
        if is_prey_ft(org) {
            prey_count += 1; // self
        }
        for cell in w.get_neighborhood_cell_ids() {
            if let Some(uid) = w.cell_organism_uid(cell) {
                let t = tgt_of(org, w, Some(uid), M)?;
                let (ft, dead, _) = t_info(org, w, t)?;
                if !dead && ft > -2 {
                    prey_count += 1;
                }
            }
        }
    } else {
        let av_cell = w.get_av_cell_id(0);
        if av_cell < 0 {
            return Err(unsupported(
                "counting prey around an organism without an avatar (C++: GetCell(-1))",
                M,
            ));
        }
        prey_count += w.cell_av_counts(av_cell).1; // self cell
        for cell in w.get_av_neighborhood_cell_ids(0) {
            prey_count += w.cell_av_counts(cell).1;
        }
    }
    // `1.0 - (0.10 * prey_count)`, one fused rounding. QUIRK: twenty prey
    // give exactly -1, which `TestAttackChance` reads as `PRED_ODDS`.
    let odds = (-0.10f64).mul_add(f64::from(prey_count), 1.0);
    let target = get_prey_target(hw, org, w, M)?;
    if !t_is_prey_target(org, w, target)? {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let reg = set_attack_reg(hw);
    if !execute_attack(hw, org, w, target, reg, odds, M)? {
        r.success = 3;
    }
    Ok(attack_results_out(org, w, r))
}

/// How a pack attack divides the prey.
#[derive(Clone, Copy)]
enum Sharing {
    /// The attacker takes all (`ExecuteAttack`).
    No,
    /// The whole pack shares (`ExecuteShareAttack`).
    Share,
    /// The attacker takes only its share (`ExecuteFakeShareAttack`).
    Fake,
}

/// The pack attacks `Inst_AttackPreyGroup` (`same_group`, no sharing),
/// `Inst_AttackPreyShare`, `Inst_AttackPreyNoShare`,
/// `Inst_AttackPreyFakeShare`, `Inst_AttackPreyGroupShare` and
/// `Inst_AttackPreyFakeGroupShare`: they need a predator pack (of the
/// attacker's group), each friend adding `PRED_ODDS` to the odds ("1 friend
/// = 20%, 8 friends = 90%").
fn attack_prey_pack(
    hw: &mut Hw,
    org: &mut OrgState,
    w: W<'_>,
    same_group: bool,
    sharing: Sharing,
    inst: &str,
    method: &str,
) -> R {
    let share_kind = match sharing {
        Sharing::No => 0,
        Sharing::Share => 1,
        Sharing::Fake => 2,
    };
    let mut r = AttackResult::new(1, share_kind);
    if same_group && !org.has_opinion() {
        return Ok(false);
    }
    if !test_attack(hw, org, w) {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let target = get_prey_target(hw, org, w, method)?;
    if !t_is_prey_target(org, w, target)? {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let mut odds = w.config().pred_odds;
    let pack = pred_pack(hw, org, w, same_group, method)?;
    r.set_size(pack.len());
    if r.size <= 1 {
        r.success = 2;
        return Ok(attack_results_out(org, w, r));
    }
    // `odds += odds * (results.size - 1)`, one fused rounding
    odds = odds.mul_add(f64::from(r.size - 1), odds);
    let reg = set_attack_reg(hw);
    let killed = match sharing {
        Sharing::No => execute_attack(hw, org, w, target, reg, odds, method)?,
        Sharing::Share => execute_share_attack(hw, org, w, target, reg, &pack, odds, method)?,
        Sharing::Fake => {
            let share = 1.0 / f64::from(r.size);
            execute_fake_share_attack(hw, org, w, target, reg, share, odds, method)?
        }
    };
    if !killed {
        r.success = 3;
    } else {
        update_group_attack_stats(hw, org, w, inst, r, false, method)?;
    }
    Ok(attack_results_out(org, w, r))
}

/// `target != GetNeighbor()` (or with avatars `target->GetAVCellID() !=
/// GetAVFacedCellID()`), negated: is the organism `uid` the one in front?
fn is_faced_org(hw: &Hw, w: W<'_>, uid: OrgUid) -> bool {
    if hw.use_avatar == 0 {
        neighbor_uid(w) == Some(uid)
    } else {
        let faced = w.get_av_faced_cell_id(0);
        w.seen_org(uid).is_some_and(|s| s.av_cell_id == faced)
    }
}

/// `Inst_AttackSpecPrey`: attack the faced prey whose id is in ?BX? (no
/// refuge check, odds 1).
fn inst_attack_spec_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackSpecPrey";
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    let id_sought_reg = hw.find_modified_register(R_BX);
    let id_sought = hw.reg(id_sought_reg);
    mark_from(hw, id_sought_reg);
    // return false if invalid number or self
    if id_sought < 0 || id_sought == org.id {
        return Ok(false);
    }
    // if valid number, does the value represent a living organism?
    let Some(uid) = w.find_live_org(id_sought) else {
        return Ok(false);
    };
    if !is_faced_org(hw, w, uid) {
        return Ok(false);
    }
    let target = tgt_of(org, w, Some(uid), M)?;
    if !t_is_prey_target(org, w, target)? {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    execute_attack(hw, org, w, target, reg, 1.0, M)
}

/// `Inst_AttackSpecPreyChance`: `Inst_AttackSpecPrey` after `TestAttack`;
/// when the sought organism is not in front, a random faced prey is
/// attacked with random odds.
fn inst_attack_spec_prey_chance(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackSpecPreyChance";
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let id_sought_reg = hw.find_modified_register(R_BX);
    let id_sought = hw.reg(id_sought_reg);
    mark_from(hw, id_sought_reg);
    // return false if invalid number or self
    if id_sought < 0 || id_sought == org.id {
        return Ok(false);
    }
    let found = w.find_live_org(id_sought);
    let generate_rand = match found {
        None => true,
        Some(uid) => !is_faced_org(hw, w, uid),
    };
    let mut odds = 1.0;
    let target = if generate_rand {
        odds = w.rng().get_double();
        get_prey_target(hw, org, w, M)?
    } else {
        tgt_of(org, w, found, M)?
    };
    if !t_is_prey_target(org, w, target)? {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    execute_attack(hw, org, w, target, reg, odds, M)
}

/// `Inst_AttackFTPrey`: attack faced prey whose forage target is ?BX?
/// (with avatars: one faced prey avatar picked at random).
fn inst_attack_ft_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackFTPrey";
    let mut r = AttackResult::new(0, 0);
    if !test_attack(hw, org, w) {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let target_reg = hw.find_modified_register(R_BX);
    let target_org_type = hw.reg(target_reg);
    let target = if hw.use_avatar == 0 {
        let uid = neighbor_uid(w);
        let t = tgt_of(org, w, uid, M)?;
        let ft = t_get(org, w, t, |s| s.forage_target)?;
        // attacking other carnivores is handled differently (e.g. using
        // fights or tolerance)
        if target_org_type != ft || ft <= -2 {
            r.success = 1;
            return Ok(attack_results_out(org, w, r));
        }
        t
    } else {
        let av_neighbors = w.get_faced_prey_avs(0);
        let rand_index = w.rng().get_uint_range(0, av_neighbors.len() as u32) as usize;
        // QUIRK: the C++ loop breaks in its first pass, so only the avatar
        // at the random index is considered.
        let Some(&uid) = av_neighbors.get(rand_index) else {
            r.success = 1;
            return Ok(attack_results_out(org, w, r));
        };
        let t = tgt_of(org, w, Some(uid), M)?;
        if t_get(org, w, t, |s| s.forage_target)? != target_org_type {
            r.success = 1;
            return Ok(attack_results_out(org, w, r));
        }
        t
    };
    if !t_is_prey_target(org, w, target)? {
        r.success = 1;
        return Ok(attack_results_out(org, w, r));
    }
    let reg = set_attack_reg(hw);
    if !execute_attack(hw, org, w, target, reg, -1.0, M)? {
        r.success = 3;
    } else {
        update_group_attack_stats(hw, org, w, "attack-ft-prey", r, true, M)?;
    }
    Ok(attack_results_out(org, w, r))
}

/// `Inst_AttackPoisonPrey`: `ExecutePoisonPreyAttack` on the faced prey.
fn inst_attack_poison_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPoisonPrey";
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let target = get_prey_target(hw, org, w, M)?;
    let target_ft = t_get(org, w, target, |s| s.forage_target)?;
    if !t_is_prey_target(org, w, target)? {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    if execute_poison_prey_attack(hw, org, w, target, reg, M)?.is_none() {
        return Ok(false);
    }
    inc_attacked_prey_ft_data(org, w, target_ft, M)?;
    Ok(true)
}

/// The faced prey avatars from a random index round to it, the first one
/// `pick` accepts (the walks of `Inst_AttackPoisonFTPrey` and
/// `Inst_AttackPoisonFTPreyGenetic`).
fn faced_prey_av_from_random(
    org: &OrgState,
    w: W<'_>,
    method: &str,
    pick: &mut dyn FnMut(&OrgState, W<'_>, Tgt) -> Result<bool, String>,
) -> Result<Option<Tgt>, String> {
    let av_neighbors = w.get_faced_prey_avs(0);
    let n = av_neighbors.len();
    let rand_index = w.rng().get_uint_range(0, n as u32) as usize;
    for k in (rand_index..n).chain(0..rand_index.min(n)) {
        let t = tgt_of(org, w, Some(av_neighbors[k]), method)?;
        if pick(org, &mut *w, t)? {
            return Ok(Some(t));
        }
    }
    Ok(None)
}

/// The end of the poison forage-target attacks: `ExecutePoisonPreyAttack`
/// and `IncAttackedPreyFTData(target->GetForageTarget())`, read after the
/// kill.
fn finish_poison_attack(hw: &mut Hw, org: &mut OrgState, w: W<'_>, target: Tgt, method: &str) -> R {
    if !t_is_prey_target(org, w, target)? {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    let ft_before = t_get(org, w, target, |s| s.forage_target)?;
    let Some(cloned) = execute_poison_prey_attack(hw, org, w, target, reg, method)? else {
        return Ok(false);
    };
    let ft = ft_after_kill(org, target, ft_before, cloned, method)?;
    inc_attacked_prey_ft_data(org, w, ft, method)?;
    Ok(true)
}

/// `Inst_AttackPoisonFTPrey`: the poison attack on faced prey that shows
/// the forage target ?BX?.
fn inst_attack_poison_ft_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPoisonFTPrey";
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let target_reg = hw.find_modified_register(R_BX);
    let target_org_type = hw.reg(target_reg);
    let target = if hw.use_avatar == 0 {
        let uid = neighbor_uid(w);
        let t = tgt_of(org, w, uid, M)?;
        let (ft, show) = t_get(org, w, t, |s| (s.forage_target, s.show_ft))?;
        if target_org_type != show {
            return Ok(false);
        }
        // attacking other carnivores is handled differently (e.g. using
        // fights or tolerance)
        if ft <= -2 {
            let mut r = AttackResult::new(0, 0);
            r.success = 1;
            return Ok(attack_results_out(org, w, r));
        }
        t
    } else {
        let found = faced_prey_av_from_random(org, w, M, &mut |o, w, t| {
            t_get(o, w, t, |s| s.show_ft == target_org_type)
        })?;
        let Some(t) = found else {
            return Ok(false);
        };
        t
    };
    finish_poison_attack(hw, org, w, target, M)
}

/// `Inst_AttackPoisonFTPreyGenetic`: the poison attack on faced prey of the
/// forage target a following nop names (read, not skipped: nop % 4, with
/// 2 any target and 3 target 2); with `MIMIC_ODDS` a candidate is judged
/// by the forage target it shows.
fn inst_attack_poison_ft_prey_genetic(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPoisonFTPreyGenetic";
    if !test_attack(hw, org, w) {
        return Ok(false);
    }
    let mut target_org_type = -1;
    let mut accept_any_target = true;
    // If followed by a nop, use nop to set target
    if hw.next_is_nop() {
        let next = hw.ip_head().get_next_inst(&hw.memory, &hw.base.inst_set);
        target_org_type = hw.base.inst_set.nop_mod(next) % 4;
        if target_org_type == 2 {
            target_org_type = -1;
        }
        if target_org_type == 3 {
            target_org_type = 2;
        }
        if target_org_type != -1 {
            accept_any_target = false;
        }
    }
    let mimic_odds = w.config().mimic_odds;
    let target = if hw.use_avatar == 0 {
        let uid = neighbor_uid(w);
        let t = tgt_of(org, w, uid, M)?;
        let (ft, show) = t_get(org, w, t, |s| (s.forage_target, s.show_ft))?;
        let mut kind = ft;
        if w.rng().p(mimic_odds) {
            kind = show;
        }
        if target_org_type != kind && !accept_any_target {
            return Ok(false);
        }
        // attacking other carnivores is handled differently (e.g. using
        // fights or tolerance)
        if ft <= -2 {
            let mut r = AttackResult::new(0, 0);
            r.success = 1;
            return Ok(attack_results_out(org, w, r));
        }
        t
    } else {
        let found = faced_prey_av_from_random(org, w, M, &mut |o, w, t| {
            let (ft, show) = t_get(o, w, t, |s| (s.forage_target, s.show_ft))?;
            let mut kind = ft;
            if w.rng().p(mimic_odds) {
                kind = show;
            }
            Ok(kind == target_org_type || accept_any_target)
        })?;
        let Some(t) = found else {
            return Ok(false);
        };
        t
    };
    finish_poison_attack(hw, org, w, target, M)
}

/// `Inst_AttackPoisonFTMixedPrey`: the poison attack on any living
/// organism showing the forage target ?BX? (if a nop follows; else any
/// prey), found by a random walk through the live organisms. QUIRK: the
/// walk can pick the attacker itself.
fn inst_attack_poison_ft_mixed_prey(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_AttackPoisonFTMixedPrey";
    if w.stats_num_prey_creatures()? == 0 {
        return Ok(false);
    }
    let target_reg = hw.find_modified_register(R_BX);
    let mut ft_sought = 0;
    let mut rand_ft = true;
    if hw.next_is_nop() {
        ft_sought = hw.reg(target_reg);
        rand_ft = false;
    }
    if ft_sought <= -1 || !w.environment().is_target_id(ft_sought) {
        return Ok(false);
    }
    let live_orgs = w.live_org_uids();
    let n = live_orgs.len();
    let mut used_orgs = vec![false; n];
    let mut num_checked = 0;
    let mut target = None;
    while num_checked < n {
        let this_rand_idx = w.rng().get_int_range(0, n as i32) as usize;
        if !used_orgs[this_rand_idx] {
            let t = tgt_of(org, w, Some(live_orgs[this_rand_idx]), M)?;
            let (ft, show, parent_ft) =
                t_get(org, w, t, |s| (s.forage_target, s.show_ft, s.parent_ft))?;
            if (!rand_ft && (ft_sought == show || (ft == -1 && ft_sought == parent_ft)))
                || (rand_ft && ft > -2)
            {
                target = Some(t);
                break;
            }
            used_orgs[this_rand_idx] = true;
            num_checked += 1;
        }
    }
    let Some(target) = target else {
        return Ok(false);
    };
    finish_poison_attack(hw, org, w, target, M)
}

/// `Inst_AttackPred` / `Inst_KillPred` (`kill_only`): attack (or only
/// kill) the faced predator when the attacker is a predator too;
/// consuming it makes the attacker a top predator.
fn attack_pred(hw: &mut Hw, org: &mut OrgState, w: W<'_>, kill_only: bool, method: &str) -> R {
    if !test_attack_pred(hw, w) {
        return Ok(false);
    }
    let uid = if hw.use_avatar == 0 {
        neighbor_uid(w)
    } else {
        w.get_rand_faced_pred_av(0)
    };
    let target = tgt_of(org, w, uid, method)?;
    let (target_ft, target_dead, _) = t_info(org, w, target)?;
    if target_ft > -2 || is_prey_ft(org) {
        return Ok(false);
    }
    if target_dead {
        return Ok(false);
    }
    let reg = set_attack_reg(hw);
    if !test_attack_chance(hw, org, w, target, reg, -1.0, method)? {
        return Ok(false);
    }
    if kill_only {
        t_die(org, w, target);
        set_env(hw, reg.success_reg, 1);
        return Ok(true);
    }
    let effic = w.config().pred_efficiency;
    if w.config().merit_inc_apply_immediate {
        let target_merit = t_get(org, w, target, |s| s.phenotype.merit.get_double())?;
        let attacker_merit = target_merit.mul_add(effic, org.phenotype.merit.get_double());
        t_update_merit(org, w, Tgt::Me, attacker_merit, method)?;
    }
    apply_killed_prey_reactions(org, w, target)?;
    let target_bonus = t_get(org, w, target, |s| s.phenotype.cur_bonus)?;
    org.phenotype.cur_bonus = target_bonus.mul_add(effic, org.phenotype.cur_bonus);
    if w.config().use_resource_bins {
        let target_bins = t_get(org, w, target, |s| s.phenotype.cur_rbins_avail.clone())?;
        for (i, &b) in target_bins.iter().enumerate() {
            org.add_to_rbin(i as i32, b * effic);
            t_mut(org, w, target, &mut |s: &mut OrgState| {
                s.add_to_rbin(i as i32, -(b * effic))
            })?;
        }
        set_spec_bin(hw, org, w, reg, method)?;
    }
    // kill first -- could end up being killed by MAX_PRED if parent was pred
    t_die(org, w, target);
    make_top_pred(hw, org, w)?;
    set_env(hw, reg.success_reg, 1);
    set_env(hw, reg.bonus_reg, target_bonus as i32);
    Ok(true)
}

/// A fight decided by `GetDouble(1)` on the attacker's and the target's
/// `attacker` and `target` values (merit or bonus): nobody dies, the
/// target dies (`AttackFacedOrg`; ?BX? = 1) or the attacker does.
fn fight(hw: &mut Hw, org: &mut OrgState, w: W<'_>, t: Tgt, attacker: f64, target: f64) -> R {
    let attacker_win_odds = attacker / (attacker + target);
    let target_win_odds = target / (attacker + target);
    // `max(attacker_win_odds, target_win_odds)`
    let odds_someone_dies = if attacker_win_odds < target_win_odds {
        target_win_odds
    } else {
        attacker_win_odds
    };
    let odds_target_dies = (1.0 - target_win_odds) * odds_someone_dies;
    let decider = w.rng().get_double_max(1.0);
    if decider < 1.0 - odds_someone_dies {
        return Ok(true);
    }
    if decider < (1.0 - odds_someone_dies) + odds_target_dies {
        // `AttackFacedOrg(ctx, target_cell)`: the attacker itself when it
        // faces its own cell.
        let target_cell = t_cell(w, t);
        w.attack_faced_org(org, target_cell);
        let out_reg = hw.find_modified_register(R_BX);
        set_env(hw, out_reg, 1);
    } else {
        org.die(w);
    }
    Ok(true)
}

/// The organism `Inst_FightMeritOrg`, `Inst_FightBonusOrg`,
/// `Inst_GetMeritFightOdds` and `Inst_FightOrg` face: "allow only for
/// predator vs predator or prey vs prey". `None` where the C++ returns
/// false.
fn fight_target(hw: &Hw, org: &OrgState, w: W<'_>, method: &str) -> Result<Option<Tgt>, String> {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(None);
    }
    let target = if ua == 0 {
        if !w.is_neighbor_cell_occupied() {
            return Ok(None);
        }
        let uid = neighbor_uid(w);
        let t = tgt_of(org, w, uid, method)?;
        if t_get(org, w, t, |s| s.forage_target > -2)? != is_prey_ft(org) {
            return Ok(None);
        }
        t
    } else {
        if !w.faced_has_av(0) {
            return Ok(None);
        }
        let uid = if is_prey_ft(org) {
            if !w.faced_has_prey_av(0) {
                return Ok(None);
            }
            w.get_rand_faced_prey_av(0)
        } else {
            if !w.faced_has_pred_av(0) {
                return Ok(None);
            }
            w.get_rand_faced_pred_av(0)
        };
        tgt_of(org, w, uid, method)?
    };
    if t_get(org, w, target, |s| s.is_dead)? {
        return Ok(None);
    }
    Ok(Some(target))
}

/// `Inst_FightMeritOrg` (`by_bonus` false) / `Inst_FightBonusOrg`: fight
/// the faced organism of the same kind by merit or current bonus.
fn fight_org(hw: &mut Hw, org: &mut OrgState, w: W<'_>, by_bonus: bool, method: &str) -> R {
    let Some(t) = fight_target(hw, org, w, method)? else {
        return Ok(false);
    };
    let value = |s: &OrgState| {
        if by_bonus {
            s.phenotype.cur_bonus
        } else {
            s.phenotype.merit.get_double()
        }
    };
    let attacker = value(org);
    let target = t_get(org, w, t, value)?;
    fight(hw, org, w, t, attacker, target)
}

/// `Inst_GetMeritFightOdds`: ?BX? = the odds out of ten of not dying in a
/// merit fight with the faced organism.
fn inst_get_merit_fight_odds(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_GetMeritFightOdds";
    let Some(t) = fight_target(hw, org, w, M)? else {
        return Ok(false);
    };
    let attacker_merit = org.phenotype.merit.get_double();
    let target_merit = t_get(org, w, t, |s| s.phenotype.merit.get_double())?;
    let attacker_win_odds = attacker_merit / (attacker_merit + target_merit);
    let target_win_odds = target_merit / (attacker_merit + target_merit);
    let odds_someone_dies = if attacker_win_odds < target_win_odds {
        target_win_odds
    } else {
        attacker_win_odds
    };
    // my win odds are odds nobody dies or someone dies and it's the target
    // (fused, as the reference build compiles them)
    let odds_i_dont_die =
        (1.0 - target_win_odds).mul_add(odds_someone_dies, 1.0 - odds_someone_dies);
    // return odds out of 10
    let out_reg = hw.find_modified_register(R_BX);
    set_env(hw, out_reg, odds_i_dont_die.mul_add(10.0, 0.5) as i32);
    Ok(true)
}

/// `Inst_FightOrg`: kill the faced organism of the same kind outright.
fn inst_fight_org(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let Some(t) = fight_target(hw, org, w, "Inst_FightOrg")? else {
        return Ok(false);
    };
    let target_cell = t_cell(w, t);
    w.attack_faced_org(org, target_cell);
    let out_reg = hw.find_modified_register(R_BX);
    set_env(hw, out_reg, 1);
    Ok(true)
}

/// `Inst_FightPred`: a merit fight with the faced predator, predators
/// only.
fn inst_fight_pred(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    const M: &str = "Inst_FightPred";
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if w.config().pred_prey_switch < 0 {
        return Ok(false);
    }
    if (ua == 0 && !w.is_neighbor_cell_occupied()) || (ua == 2 && !w.faced_has_pred_av(0)) {
        return Ok(false);
    }
    let uid = if ua == 0 {
        neighbor_uid(w)
    } else {
        w.get_rand_faced_pred_av(0)
    };
    let t = tgt_of(org, w, uid, M)?;
    let (target_ft, target_dead, _) = t_info(org, w, t)?;
    if target_dead {
        return Ok(false);
    }
    // allow only for predator vs predator
    if target_ft > -2 || is_prey_ft(org) {
        return Ok(false);
    }
    let attacker = org.phenotype.merit.get_double();
    let target = t_get(org, w, t, |s| s.phenotype.merit.get_double())?;
    fight(hw, org, w, t, attacker, target)
}

// ----------------------------------------------------------------------
// Cell marking
// ----------------------------------------------------------------------

/// `Inst_MarkCell` / `Inst_MarkGroupCell` / `Inst_MarkPredCell` (`pred`):
/// mark this cell (or the avatar's) with ?BX?.
fn mark_cell(hw: &mut Hw, org: &mut OrgState, w: W<'_>, pred: bool) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if pred && is_prey_ft(org) {
        return Ok(false);
    }
    let r = hw.find_modified_register(R_BX);
    let marking = hw.reg(r);
    if ua == 0 {
        w.set_cell_data(org, marking);
    } else {
        let id = org.id;
        w.set_av_cell_data(org, marking, id, 0);
    }
    Ok(true)
}

/// `Inst_ReadGroupCell`: read this cell's marking when it is the
/// organism's group's.
fn inst_read_group_cell(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let ua = hw.use_avatar;
    let mut success = !(ua != 0 && ua != 2);
    if success && !org.has_opinion() {
        success = false;
    }
    if success && ua == 2 && w.get_av_data_territory(0) != org.get_opinion().0 {
        success = false;
    }
    if success && ua == 0 && w.get_cell_data_territory() != org.get_opinion().0 {
        success = false;
    }
    if success {
        let marking_reg = hw.find_modified_register(R_BX);
        let update_reg = hw.find_modified_next_register(R_BX);
        let org_reg = hw.find_modified_next_register(update_reg);
        let update = w.update();
        let (data, when, org_id, territory) = if ua == 0 {
            (
                w.get_cell_data(),
                w.get_cell_data_update(),
                w.get_cell_data_org_id(),
                w.get_cell_data_territory(),
            )
        } else {
            (
                w.get_av_data(0),
                w.get_av_data_update(0),
                w.get_av_data_org_id(0),
                w.get_av_data_territory(0),
            )
        };
        hw.set_iv(marking_reg, data, true, false, false);
        hw.set_iv(update_reg, update.wrapping_sub(when), true, false, false);
        hw.set_iv(org_reg, org_id, true, false, false);
        let group_reg = hw.find_modified_next_register(org_reg);
        hw.set_iv(group_reg, territory, true, false, false);
    }
    Ok(success)
}

/// `Inst_ReadFacedCell` / `Inst_ReadFacedPredCell` (`pred`): read the
/// faced cell's marking.
fn read_faced_cell(hw: &mut Hw, org: &mut OrgState, w: W<'_>, pred: bool) -> R {
    let ua = hw.use_avatar;
    if ua != 0 && ua != 2 {
        return Ok(false);
    }
    if pred && is_prey_ft(org) {
        return Ok(false);
    }
    let marking_reg = hw.find_modified_register(R_BX);
    // QUIRK: the update register follows ?BX?, not the marking register.
    let update_reg = hw.find_modified_next_register(R_BX);
    let org_reg = hw.find_modified_next_register(update_reg);
    let update = w.update();
    let (data, when, org_id, territory) = if ua == 0 {
        (
            w.get_faced_cell_data(),
            w.get_faced_cell_data_update(),
            w.get_faced_cell_data_org_id(),
            w.get_faced_cell_data_territory(),
        )
    } else {
        (
            w.get_av_faced_data(0),
            w.get_av_faced_data_update(0),
            w.get_av_faced_data_org_id(0),
            w.get_av_faced_data_territory(0),
        )
    };
    hw.set_iv(marking_reg, data, true, false, false);
    hw.set_iv(update_reg, update.wrapping_sub(when), true, false, false);
    hw.set_iv(org_reg, org_id, true, false, false);
    let group_reg = hw.find_modified_next_register(org_reg);
    hw.set_iv(group_reg, territory, true, false, false);
    Ok(true)
}

// ----------------------------------------------------------------------
// Teaching, guards and juveniles
// ----------------------------------------------------------------------

/// `Inst_LearnParent`: copy a teaching parent's forage target.
fn inst_learn_parent(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let mut halt = false;
    if org.parent_teacher {
        let old_target = org.forage_target;
        let prop_target = org.parent_ft;
        let pred = |t: i32| t == -2 || t == -3;
        halt = pred(prop_target) && w.config().pred_prey_switch < 0;
        if !halt {
            if hw.use_avatar != 0
                && w.get_av_cell_id(0) != -1
                && ((pred(prop_target) && !pred(old_target))
                    || (!pred(prop_target) && pred(old_target)))
            {
                w.switch_pred_prey(0);
            }
            copy_parent_ft(org, w)?;
        }
    }
    Ok(!halt)
}

/// Whether this cell holds some nest or den resource (habitat 3 or 4).
fn on_den(hw: &Hw, org: &mut OrgState, w: W<'_>) -> bool {
    let info = res_info(w);
    let mut on = false;
    for (i, &(habitat, _, _)) in info.iter().enumerate() {
        if (habitat == 3 || habitat == 4) && here_res_val(hw, org, w, i as i32) > 0.0 {
            on = true;
        }
    }
    on
}

/// `Inst_SetGuard`: an adult on a den toggles guarding (otherwise the
/// stats count a guard failure); ?BX? = guarding.
fn inst_set_guard(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let set_ok = org.phenotype.time_used >= w.config().juv_period && on_den(hw, org, w);
    if set_ok {
        org.guard = !org.guard;
        org.num_guard = org.num_guard.wrapping_add(1);
    } else {
        w.stats_inc_guard_fail();
    }
    let r = hw.find_modified_register(R_BX);
    hw.set_iv(r, i32::from(org.guard), true, false, false);
    Ok(set_ok)
}

/// `Inst_GetNumGuards` (`juvs` false) / `Inst_GetNumJuvs` (true): on a den,
/// ?BX? = the guards (juveniles) here.
fn on_den_count(hw: &mut Hw, org: &mut OrgState, w: W<'_>, juvs: bool) -> R {
    let juv_age = w.config().juv_period;
    let on = on_den(hw, org, w);
    let mut num = 0;
    let counts = |st: &OrgState| {
        if juvs {
            st.phenotype.time_used < juv_age
        } else {
            st.guard
        }
    };
    if on {
        if hw.use_avatar != 0 {
            // Every avatar in the avatar's cell (`GetCellAVs(GetAVCellID())`),
            // the organism's own included.
            let cell_id = w.get_av_cell_id(0);
            for uid in w.get_cell_avs(cell_id, 0) {
                if uid == org.uid {
                    if counts(org) {
                        num += 1;
                    }
                    continue;
                }
                let mut hit = None;
                w.with_organism(uid, &mut |o: &mut Organism| hit = Some(counts(&o.state)));
                match hit {
                    Some(true) => num += 1,
                    Some(false) => {}
                    None => {
                        return Err(unsupported(
                            "an avatar without a living organism (C++: dangling cOrganism*)",
                            if juvs {
                                "Inst_GetNumJuvs"
                            } else {
                                "Inst_GetNumGuards"
                            },
                        ))
                    }
                }
            }
        } else if counts(org) {
            num += 1;
        }
    }
    let r = hw.find_modified_register(R_BX);
    hw.set_iv(r, num, true, false, false);
    Ok(on)
}

// ----------------------------------------------------------------------
// Displays
// ----------------------------------------------------------------------

/// `cOrganism::SetSimpDisplay(display_part, value)`: create the display
/// (and switch displaying on) when there is none, then fall through the
/// C++ `switch`.
pub(crate) fn set_simp_display(org: &mut OrgState, display_part: i32, value: i32) {
    if org.org_display.is_none() {
        // QUIRK: thing_id of a new display is left uninitialised in C++.
        org.org_display = Some(OrgDisplay {
            distance: -99,
            direction: -99,
            thing_id: 0,
            value: -99,
            message: -99,
        });
        org.display = true;
    }
    let Some(d) = org.org_display.as_mut() else {
        return;
    };
    match display_part {
        0 => {
            d.distance = value;
            d.direction = value;
            d.value = value;
            d.message = value;
        }
        1 => {
            d.direction = value;
            d.value = value;
            d.message = value;
        }
        3 => {
            d.value = value;
            d.message = value;
        }
        _ => d.message = value,
    }
}

/// `Inst_ModifyDisplay`: copy registers into the display as the following
/// nops select (the C++ `switch` falls through).
fn inst_modify_display(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    let Some(mut d) = org.org_display else {
        return Ok(false);
    };
    let size = hw.memory.size();
    let mut pos = hw.ip_head().position();
    let mut message_used = false;
    for _ in 0..5 {
        pos += 1;
        if pos >= size {
            pos = 0;
        }
        let inst = hw.memory[pos];
        if !hw.base.inst_set.is_nop(inst) {
            break;
        }
        let this_nop = hw.base.inst_set.nop_mod(inst);
        if this_nop == 0 {
            d.distance = hw.reg(R_AX);
        }
        if (0..=1).contains(&this_nop) {
            d.direction = hw.reg(R_BX);
        }
        if (0..=2).contains(&this_nop) {
            d.thing_id = hw.reg(R_CX);
        }
        if (0..=3).contains(&this_nop) {
            d.value = hw.reg(R_DX);
        }
        if !message_used {
            d.message = hw.reg(this_nop as usize);
        }
        message_used = true;
    }
    org.org_display = Some(d);
    Ok(true)
}

/// `Inst_ModifySimpDisplay`: `SetSimpDisplay` from the registers the
/// following nops select.
fn inst_modify_simp_display(hw: &mut Hw, org: &mut OrgState, _w: W<'_>) -> R {
    let size = hw.memory.size();
    let mut pos = hw.ip_head().position();
    let mut message_used = false;
    for _ in 0..4 {
        pos += 1;
        if pos >= size {
            pos = 0;
        }
        let inst = hw.memory[pos];
        if !hw.base.inst_set.is_nop(inst) {
            break;
        }
        let this_nop = hw.base.inst_set.nop_mod(inst);
        if this_nop == 0 {
            set_simp_display(org, 0, hw.reg(R_AX));
        }
        if (0..=1).contains(&this_nop) {
            set_simp_display(org, 1, hw.reg(R_BX));
        }
        if (0..=2).contains(&this_nop) {
            set_simp_display(org, 2, hw.reg(R_CX));
        }
        if !message_used {
            set_simp_display(org, 3, hw.reg(this_nop as usize));
        }
        message_used = true;
    }
    Ok(true)
}

/// `Inst_ReadLastSeenDisplay` (`simp` false) and `Inst_ReadLastSimpDisplay`
/// (true): copy the last display seen into the registers the following
/// nops select.
fn read_last_display(hw: &mut Hw, simp_display: bool) -> R {
    if !hw.sensor.has_seen_display() {
        return Ok(false);
    }
    let last = *hw.sensor.last_seen_display();
    let size = hw.memory.size();
    let mut pos = hw.ip_head().position();
    let mut message_read = false;
    let count = if simp_display { 4 } else { 5 };
    for i in 0..count {
        pos += 1;
        if pos >= size {
            pos = 0;
        }
        let inst = hw.memory[pos];
        if !hw.base.inst_set.is_nop(inst) {
            if simp_display && i == 0 {
                hw.set_iv(R_AX, last.distance, true, false, false);
                hw.set_iv(R_BX, last.direction, true, false, false);
                hw.set_iv(R_CX, last.value, true, false, false);
                hw.set_iv(R_DX, last.message, true, false, false);
            }
            break;
        }
        let this_nop = hw.base.inst_set.nop_mod(inst);
        if this_nop == 0 {
            hw.set_iv(R_AX, last.distance, true, false, false);
        }
        if (0..=1).contains(&this_nop) {
            hw.set_iv(R_BX, last.direction, true, false, false);
        }
        if (0..=2).contains(&this_nop) {
            let v = if simp_display {
                last.value
            } else {
                last.thing_id
            };
            hw.set_iv(R_CX, v, true, false, false);
        }
        if !simp_display && (0..=3).contains(&this_nop) {
            hw.set_iv(R_DX, last.value, true, false, false);
        }
        if !message_read {
            hw.set_iv(this_nop as usize, last.message, true, false, false);
        }
        message_read = true;
    }
    Ok(true)
}

// ----------------------------------------------------------------------
// Tolerance
// ----------------------------------------------------------------------

/// `Inst_IncPredTolerance` (`inc`) / `Inst_DecPredTolerance`: change the
/// tolerance the nop selects; BX = the new total.
fn change_pred_tolerance(hw: &mut Hw, org: &mut OrgState, w: W<'_>, inc: bool) -> R {
    if is_prey_ft(org) {
        return Ok(false);
    }
    let cfg = w.config();
    if cfg.use_form_groups == 0 || cfg.tolerance_window <= 0 {
        return Ok(false);
    }
    if !w.has_opinion(org) {
        return Ok(false);
    }
    if !hw.next_is_nop() {
        return Ok(false);
    }
    let mut tolerance_type = 0;
    if w.config().tolerance_variations == 0 {
        let tolerance_to_modify = hw.find_modified_register(R_BX);
        tolerance_type = match tolerance_to_modify {
            R_AX => 0,
            R_BX => 1,
            R_CX => 2,
            _ => return Ok(false),
        };
    }
    if inc {
        let result = w.inc_tolerance(org, tolerance_type);
        if result == -1 {
            return Ok(false);
        }
        hw.set_iv(R_BX, result, true, false, false);
    } else {
        let result = w.dec_tolerance(org, tolerance_type);
        hw.set_ivp(R_BX, result);
    }
    Ok(true)
}

/// `Inst_GetPredTolerance`: AX / BX / CX = the predator's tolerances.
fn inst_get_pred_tolerance(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if is_prey_ft(org) {
        return Ok(false);
    }
    let cfg = w.config();
    if cfg.use_form_groups != 0 && cfg.tolerance_window > 0 && w.has_opinion(org) {
        if org.get_opinion().0 == -1 {
            return Ok(false);
        }
        w.push_tolerance_inst_exe(org, 6);
        let update = w.update();
        let cfg = w.config();
        let tolerance_immigrants = org.phenotype.calc_tolerance_immigrants(cfg, update);
        let tolerance_own = org.phenotype.calc_tolerance_offspring_own(cfg, update);
        let tolerance_others = org.phenotype.calc_tolerance_offspring_others(cfg, update);
        hw.set_iv(R_AX, tolerance_immigrants, true, false, false);
        hw.set_iv(R_BX, tolerance_own, true, false, false);
        hw.set_iv(R_CX, tolerance_others, true, false, false);
        return Ok(true);
    }
    Ok(false)
}

/// `Inst_GetPredGroupTolerance`: AX / BX / CX = the group's admission odds
/// in percent.
fn inst_get_pred_group_tolerance(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if is_prey_ft(org) || org.get_opinion().0 < 0 {
        return Ok(false);
    }
    let cfg = w.config();
    if cfg.use_form_groups != 0 && cfg.tolerance_window > 0 && w.has_opinion(org) {
        w.push_tolerance_inst_exe(org, 7);
        let group_id = org.get_opinion().0;
        if group_id == -1 {
            return Ok(false);
        }
        let mut mating_type = -1;
        if w.config().tolerance_variations == 2 {
            mating_type = if org.phenotype.mating_type == MATING_TYPE_FEMALE {
                0
            } else if org.phenotype.mating_type == MATING_TYPE_MALE {
                1
            } else {
                2
            };
        }
        let immigrant_odds = w.calc_group_odds_immigrants(org, group_id, mating_type);
        let offspring_own_odds = w.calc_group_odds_offspring_parent(org);
        let offspring_others_odds = w.calc_group_odds_offspring(org, group_id);
        hw.set_iv(
            R_AX,
            immigrant_odds.mul_add(100.0, 0.5) as i32,
            true,
            false,
            false,
        );
        hw.set_iv(
            R_BX,
            offspring_own_odds.mul_add(100.0, 0.5) as i32,
            true,
            false,
            false,
        );
        hw.set_iv(
            R_CX,
            offspring_others_odds.mul_add(100.0, 0.5) as i32,
            true,
            false,
            false,
        );
        return Ok(true);
    }
    Ok(false)
}

// ----------------------------------------------------------------------
// Donation and edit distance
// ----------------------------------------------------------------------

/// `(IsDead(), GetID())` of the `GetNeighbor()` (`use_avatar` 0) or
/// `GetRandFacedAV(ctx)` (2) of `Inst_GetFacedOrgID`; the running organism
/// answers for itself. `None` where the C++ pointer would be NULL.
pub(crate) fn faced_org_info(org: &mut OrgState, w: W<'_>, ua: i32) -> Option<(bool, i32)> {
    let info = |n: &OrgState| (n.is_dead, n.id);
    if ua == 0 {
        return with_neighbor_state(w, org, |n| info(n));
    }
    let uid = w.get_rand_faced_av(0)?;
    if uid == org.uid {
        return Some(info(org));
    }
    let mut out = None;
    w.with_organism(uid, &mut |n: &mut Organism| out = Some(info(&n.state)));
    out
}

/// `Inst_DonateSpecific`: give a unit of the `COLLECT_SPECIFIC_RESOURCE`
/// bin to the faced organism (resource bins only). The edit distance the
/// C++ then computes is never used.
fn inst_donate_specific(_hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    let cfg = w.config();
    let donates = org.phenotype.cur_num_donates;
    if donates > cfg.max_donates
        || (cfg.max_donate_edit_dist > 0 && donates > cfg.max_donate_edit_dist)
    {
        return Ok(false);
    }
    if !w.is_neighbor_cell_occupied() {
        return Ok(false);
    }
    let resource = w.config().collect_specific_resource;
    if w.config().use_resource_bins && org.rbin(resource) >= 1.0 {
        // `target->AddToRBin(resource, 1)`, `target->GetPhenotype()
        // .SetIsReceiverEdit()` (the donor's counts in between are other
        // fields); the target is the organism itself when it faces its own
        // cell.
        let got = with_neighbor_state(w, org, |o| {
            o.add_to_rbin(resource, 1.0);
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_edit = true;
        });
        if got.is_none() {
            return Err(unsupported(
                "an occupied faced cell without an organism (NULL in C++)",
                "Inst_DonateSpecific",
            ));
        }
        // `IncDonates()`, `SetIsDonorEdit()`
        org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
        org.phenotype.is_donor_cur = true;
        org.phenotype.is_donor_edit = true;
        return Ok(true);
    }
    Ok(false)
}

/// `Inst_GetFacedEditDistance`: BX = the edit distance between this genome
/// and the faced organism's.
fn inst_get_faced_edit_distance(hw: &mut Hw, org: &mut OrgState, w: W<'_>) -> R {
    if !w.is_neighbor_cell_occupied() {
        return Ok(false);
    }
    let own = org.genome().representation().clone();
    // The organism's own genome when it faces its own cell.
    let dist = with_neighbor_state(w, org, |n| {
        InstructionSequence::find_edit_distance(&own, n.genome().representation())
    });
    let Some(dist) = dist else {
        return Err(unsupported(
            "no organism in an occupied faced cell (null in C++)",
            "Inst_GetFacedEditDistance",
        ));
    };
    hw.set_iv(R_BX, dist, true, false, false);
    Ok(true)
}
