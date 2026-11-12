//! Donation, energy-sharing and reputation handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb: `DoDonate` through
//! `Inst_DonateSpecific`, and `Inst_DonateIfDonor` through
//! `Inst_ProduceString`), with the `cOrganism` reputation and string
//! helpers they call (avida-core/source/main/cOrganism.cc).
//!
//! A C++ `cOrganism* neighbor` is kept as [`Nb`]: the neighbour's id and
//! cell, read through [`OrgInterface::with_neighbor`] at the point where
//! the C++ calls `GetNeighbor()`. Later reads and writes go through
//! [`with_nb`]: to the running organism's own state when the neighbour is
//! the organism itself (it faces its own cell in a torus one cell wide or
//! high, see [`Faced`]; donor and recipient are then one organism and
//! every read sees the writes made before it, as in the C++), else through
//! [`OrgInterface::with_organism_in_cell`] (the donor may have turned back
//! in between, and ids alone are ambiguous: both offspring of a sexual
//! birth share one), provided the organism there is still the same. A
//! neighbour the world no longer holds (one a merit update to 0 just
//! killed, which the C++ goes on to use after free) is left alone.
//!
//! The kin / edit-distance / green-beard donations keep `static` counters
//! of the edit distance between donor and recipient whose only consumer is
//! commented out; they have no observable effect and are not kept.

use crate::hardware_cpu::{HardwareCpu, REG_AX, REG_BX};
use crate::hardware_cpu_insts::{arm_div, InstFn, R};
use crate::hardware_cpu_insts_msg::{new_message, org_broadcast_message, org_retrieve_message};
use crate::org_interface::{with_neighbor_state, Faced, OrgInterface};
use crate::organism::{OrgState, OrgUid};
use avida_genome::{Instruction, InstructionSequence};

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_DonateFacing" => inst_donate_facing,
        "Inst_DonateRandom" => inst_donate_random,
        "Inst_DonateKin" => inst_donate_kin,
        "Inst_DonateEditDist" => inst_donate_edit_dist,
        "Inst_GetFacedEditDistance" => inst_get_faced_edit_distance,
        "Inst_DonateGreenBeardGene" => inst_donate_green_beard_gene,
        "Inst_DonateShadedGreenBeard" => inst_donate_shaded_green_beard,
        "Inst_DonateTrueGreenBeard" => inst_donate_true_green_beard,
        "Inst_DonateThreshGreenBeard" => inst_donate_thresh_green_beard,
        "Inst_DonateQuantaThreshGreenBeard" => inst_donate_quanta_thresh_green_beard,
        "Inst_DonateGreenBeardSameLocus" => inst_donate_green_beard_same_locus,
        "Inst_DonateNULL" => inst_donate_null,
        "Inst_ReceiveDonatedEnergy" => inst_receive_donated_energy,
        "Inst_DonateEnergy" => inst_donate_energy,
        "Inst_UpdateMetabolicRate" => inst_update_metabolic_rate,
        "Inst_DonateEnergyFaced" => |h, o, w| donate_energy_faced(h, o, w, None),
        "Inst_DonateEnergyFaced1" => |h, o, w| donate_energy_faced(h, o, w, Some(1.0)),
        "Inst_DonateEnergyFaced2" => |h, o, w| donate_energy_faced(h, o, w, Some(2.0)),
        "Inst_DonateEnergyFaced5" => |h, o, w| donate_energy_faced(h, o, w, Some(5.0)),
        "Inst_DonateEnergyFaced10" => |h, o, w| donate_energy_faced(h, o, w, Some(10.0)),
        "Inst_DonateEnergyFaced20" => |h, o, w| donate_energy_faced(h, o, w, Some(20.0)),
        "Inst_DonateEnergyFaced50" => |h, o, w| donate_energy_faced(h, o, w, Some(50.0)),
        "Inst_DonateEnergyFaced100" => |h, o, w| donate_energy_faced(h, o, w, Some(100.0)),
        "Inst_RotateToMostNeedy" => inst_rotate_to_most_needy,
        "Inst_RequestEnergy" => inst_request_energy,
        "Inst_RequestEnergyFlagOn" => inst_request_energy_flag_on,
        "Inst_RequestEnergyFlagOff" => |_, o, w| {
            if w.get_cell_id() < 0 {
                return Ok(false);
            }
            o.phenotype.has_open_energy_request = false;
            Ok(true)
        },
        "Inst_IncreaseEnergyDonation" => |_, o, w| {
            let increment = w.config().energy_sharing_increment;
            o.frac_energy_donating = cmin(1.0, o.frac_energy_donating + increment);
            Ok(true)
        },
        "Inst_DecreaseEnergyDonation" => |_, o, w| {
            let increment = w.config().energy_sharing_increment;
            o.frac_energy_donating = cmax(0.0, o.frac_energy_donating - increment);
            Ok(true)
        },
        "Inst_DonateResource0" => |_, o, w| donate_resource_x(o, w, 0),
        "Inst_DonateResource1" => |_, o, w| donate_resource_x(o, w, 1),
        "Inst_DonateResource2" => |_, o, w| donate_resource_x(o, w, 2),
        "Inst_DonateSpecific" => inst_donate_specific,
        "Inst_DonateIfDonor" => inst_donate_if_donor,
        "Inst_DonateFacingRawMaterialsOtherSpecies" => |_, o, w| donate_other_species(o, w),
        "Inst_DonateFacingString" => |_, o, w| donate_facing_string(o, w),
        "Inst_DonateFacingRawMaterials" => |_, o, w| donate_facing_raw_materials(o, w),
        "Inst_Pose" => |_, o, w| {
            let rep = get_reputation(o);
            set_reputation(o, w, rep.wrapping_add(1))?;
            Ok(true)
        },
        "Inst_GetNeighborsReputation" => inst_get_neighbors_reputation,
        "Inst_GetReputation" => |h, o, _| {
            let opinion_reg = h.find_modified_register(REG_BX);
            h.set_reg(opinion_reg, get_reputation(o));
            Ok(true)
        },
        "Inst_GetAmountOfRawMaterials" => |h, o, _| {
            let raw_mat_reg = h.find_modified_register(REG_AX);
            h.set_reg(raw_mat_reg, strings_on_hand(o, 0));
            Ok(true)
        },
        "Inst_GetAmountOfOtherRawMaterials" => |h, o, _| {
            let raw_mat_reg = h.find_modified_register(REG_BX);
            h.set_reg(raw_mat_reg, strings_on_hand(o, 1));
            Ok(true)
        },
        "Inst_RotateToGreatestReputation" => |_, o, w| {
            w.rotate_to_greatest_reputation(o);
            Ok(true)
        },
        "Inst_RotateToGreatestReputationWithDifferentTag" => |_, o, w| {
            let tag = o.tag.0;
            w.rotate_to_greatest_reputation_with_different_tag(o, tag);
            Ok(true)
        },
        "Inst_RotateToGreatestReputationWithDifferentLineage" => |_, o, w| {
            let line = o.lineage_label;
            w.rotate_to_greatest_reputation_with_different_lineage(o, line);
            Ok(true)
        },
        "Inst_RotateToGreatestReputationAndDonate" => |_, o, w| {
            w.rotate_to_greatest_reputation(o);
            donate_facing_raw_materials(o, w)?;
            Ok(true)
        },
        "Inst_RotateToDifferentTag" => inst_rotate_to_different_tag,
        "Inst_IfDonor" => inst_if_donor,
        "Inst_ProduceString" => inst_produce_string,
        _ => return None,
    };
    Some(f)
}

/// `std::min(a, b)`: `a` unless `b < a` (NaN parity).
fn cmin(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

/// `std::max(a, b)`: `a` unless `a < b` (NaN parity).
fn cmax(a: f64, b: f64) -> f64 {
    if a < b {
        b
    } else {
        a
    }
}

/// A neighbouring organism: its id (`GetID()`), the organism itself and
/// the cell it lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Nb {
    pub(crate) id: i32,
    pub(crate) uid: OrgUid,
    pub(crate) cell: i32,
}

/// `m_organism->GetNeighbor()`: the organism in the faced cell, which is
/// the running organism `org` itself when it faces its own cell
/// ([`Faced::SelfOrg`]).
pub(crate) fn neighbor(world: &mut dyn OrgInterface, org: &OrgState) -> Option<Nb> {
    let mut ids = None;
    let (id, uid) = match world.with_neighbor(&mut |o| ids = Some((o.state.id, o.state.uid))) {
        Faced::Empty => return None,
        Faced::SelfOrg => (org.id, org.uid),
        Faced::Other => ids?,
    };
    let cell = world.get_faced_cell_id();
    Some(Nb { id, uid, cell })
}

/// Run `f` on the state of the neighbour `nb`: `org` itself when `nb` is
/// the running organism (organisms of a population have distinct uids),
/// else the organism living in `nb.cell`; `None` when the world no longer
/// has it.
pub(crate) fn with_nb<T>(
    world: &mut dyn OrgInterface,
    org: &mut OrgState,
    nb: Nb,
    f: impl FnOnce(&mut OrgState) -> T,
) -> Option<T> {
    if nb.uid == org.uid {
        return Some(f(org));
    }
    let mut f = Some(f);
    let mut out = None;
    world.with_organism_in_cell(nb.cell, &mut |o| {
        if o.state.uid == nb.uid {
            if let Some(f) = f.take() {
                out = Some(f(&mut o.state));
            }
        }
    });
    out
}

/// `GetCurNumDonates() > MAX_DONATES`: the donation instructions fail.
fn over_donation_limit(org: &OrgState, world: &dyn OrgInterface) -> bool {
    org.phenotype.cur_num_donates > world.config().max_donates
}

/// The test [`find_random_neighbor`] applies to each faced organism (with
/// the running organism, which may be that organism).
type Accept<'a> =
    dyn FnMut(&mut dyn OrgInterface, &mut OrgState, Option<Nb>) -> Result<bool, String> + 'a;

/// The search shared by the kin / edit-distance / green-beard donations:
/// face a random neighbour (`GetInt(num_neighbors)` turns, drawn even with
/// no neighbours), turn on until `accept` takes the faced organism or every
/// neighbour was tried, then turn back. Returns the accepted neighbour.
fn find_random_neighbor(
    world: &mut dyn OrgInterface,
    org: &mut OrgState,
    accept: &mut Accept<'_>,
) -> Result<Option<Nb>, String> {
    let num_neighbors = world.get_num_neighbors();
    let mut neighbor_id = world.rng().get_int(num_neighbors);
    for _ in 0..neighbor_id {
        world.rotate(1);
    }
    let max_id = neighbor_id.wrapping_add(num_neighbors);
    let mut found = None;
    while neighbor_id < max_id {
        let nb = neighbor(world, org);
        if accept(world, org, nb)? {
            found = nb;
            break;
        }
        world.rotate(1);
        neighbor_id += 1;
    }
    for _ in 0..neighbor_id {
        world.rotate(-1);
    }
    Ok(found)
}

/// The neighbour's genome (`neighbor->GetGenome()`, the initial genome).
fn nb_genome(
    world: &mut dyn OrgInterface,
    org: &mut OrgState,
    nb: Nb,
) -> Option<InstructionSequence> {
    with_nb(world, org, nb, |o| {
        o.initial_genome.representation().clone()
    })
}

/// Whether `inst` occurs anywhere in `seq`.
fn genome_has(seq: &InstructionSequence, inst: Instruction) -> bool {
    seq.iter().any(|&i| i == inst)
}

/// The instruction under the IP (`getIP().GetInst()`).
fn ip_inst(hw: &HardwareCpu) -> Instruction {
    hw.ip_head().get_inst(&hw.memory)
}

/// The opcode of the last instruction named `name`, 0 when there is none
/// (the `for (i < num_inst) if (GetName(i) == name) inst_number = i` scan).
fn last_inst_number(hw: &HardwareCpu, name: &str, extra: bool) -> Option<i32> {
    let mut out = None;
    for i in 0..hw.base.inst_set.size() {
        if hw.base.inst_set.name(i) == name && extra {
            out = Some(i);
        }
    }
    out
}

/// `GetTestCPUInstCount()[i]`; 0 outside the array (undefined in C++).
fn test_cpu_count(org: &OrgState, i: i32) -> i32 {
    usize::try_from(i)
        .ok()
        .and_then(|i| org.phenotype.test_cpu_inst_count.get(i).copied())
        .unwrap_or(0)
}

/// `DoDonate(ctx, to_org)`: give `MERIT_GIVEN` of the own merit (floored at
/// 0) and add `MERIT_RECEIVED` to the recipient's.
fn do_donate(org: &mut OrgState, world: &mut dyn OrgInterface, to: Nb) {
    let merit_given = world.config().merit_given;
    let merit_received = world.config().merit_received;
    let mut cur_merit = org.phenotype.merit.get_double();
    cur_merit -= merit_given;
    if cur_merit < 0.0 {
        cur_merit = 0.0;
    }
    world.update_merit(org, cur_merit);
    org.phenotype.is_energy_donor = true;

    let Some(mut other_merit) = with_nb(world, org, to, |o| o.phenotype.merit.get_double()) else {
        return;
    };
    other_merit += merit_received;
    world.update_cell_merit(org, to.cell, other_merit);
    with_nb(world, org, to, |o| o.phenotype.is_energy_receiver = true);
}

/// `ConvertEnergyToMerit(GetStoredEnergy() * GetEnergyUsageRatio())`
fn energy_merit(org: &OrgState, cfg: &avida_config::config::Config) -> f64 {
    let p = &org.phenotype;
    p.convert_energy_to_merit(cfg, p.energy_store * p.execution_ratio)
}

/// `DoEnergyDonate(ctx, to_org)`: move `GetFracEnergyDonating()` of the
/// stored energy straight into the recipient's store; both merits follow.
fn do_energy_donate(org: &mut OrgState, world: &mut dyn OrgInterface, to: Nb) {
    let cfg = world.config().clone();
    let frac_energy_given = org.frac_energy_donating;
    let cur_energy = org.phenotype.energy_store;
    let energy_given = cur_energy * frac_energy_given;

    org.phenotype.reduce_energy(&cfg, energy_given);
    org.phenotype.total_energy_donated += energy_given;
    let sender_merit = energy_merit(org, &cfg);
    world.update_merit(org, sender_merit);
    org.phenotype.is_energy_donor = true;

    let Some(receiver_merit) = with_nb(world, org, to, |o| {
        o.phenotype.reduce_energy(&cfg, -energy_given);
        o.phenotype.total_energy_received += energy_given;
        energy_merit(o, &cfg)
    }) else {
        return;
    };
    world.update_cell_merit(org, to.cell, receiver_merit);
    with_nb(world, org, to, |o| o.phenotype.is_energy_receiver = true);
}

/// `DoEnergyDonateAmount(ctx, to_org, amount)`: put up to `amount` of the
/// stored energy, less `RESOURCE_SHARING_LOSS`, into the recipient's
/// incoming buffer (and its store under `ENERGY_SHARING_METHOD` 1).
fn do_energy_donate_amount(org: &mut OrgState, world: &mut dyn OrgInterface, to: Nb, amount: f64) {
    let cfg = world.config().clone();
    let losspct = cfg.resource_sharing_loss;
    let update_metabolic = cfg.energy_sharing_update_metabolic;
    let energy_given = cmin(org.phenotype.energy_store, amount);

    org.phenotype.reduce_energy(&cfg, energy_given);
    org.phenotype.is_energy_donor = true;
    org.phenotype.total_energy_donated += energy_given;
    org.phenotype.num_energy_donations = org.phenotype.num_energy_donations.wrapping_add(1);
    let deme_id = world.get_deme_id();
    world.deme_increase_energy_donated(deme_id, energy_given);

    if update_metabolic {
        let sender_merit = energy_merit(org, &cfg);
        world.update_merit(org, sender_merit);
    }

    let energy_received = energy_given * (1.0 - losspct);
    if with_nb(world, org, to, |o| {
        o.phenotype.receive_donated_energy(energy_received)
    })
    .is_none()
    {
        return;
    }
    let to_deme = world.cell_deme_id(to.cell);
    world.deme_increase_energy_received(to_deme, energy_received);

    if cfg.energy_sharing_method == 1 {
        let receiver_merit = with_nb(world, org, to, |o| {
            o.phenotype.apply_donated_energy(&cfg);
            energy_merit(o, &cfg)
        });
        if update_metabolic {
            if let Some(m) = receiver_merit {
                world.update_cell_merit(org, to.cell, m);
            }
        }
    }
}

/// `DoEnergyDonatePercent(ctx, to_org, frac)`
fn do_energy_donate_percent(org: &mut OrgState, world: &mut dyn OrgInterface, to: Nb, frac: f64) {
    let amount = org.phenotype.energy_store * frac;
    do_energy_donate_amount(org, world, to, amount);
}

/// `Inst_DonateFacing`: share energy with the faced organism.
fn inst_donate_facing(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_rand = true;
    if let Some(nb) = neighbor(world, org) {
        do_energy_donate(org, world, nb);
        with_nb(world, org, nb, |o| o.phenotype.is_receiver = true);
    }
    Ok(true)
}

/// `Inst_DonateRandom`: donate merit to a random neighbour.
fn inst_donate_random(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_rand = true;

    let num_neighbors = world.get_num_neighbors();
    let neighbor_id = world.rng().get_int(num_neighbors);
    for _ in 0..neighbor_id {
        world.rotate(1);
    }
    let nb = neighbor(world, org);
    for _ in 0..neighbor_id {
        world.rotate(-1);
    }
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_rand = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateKin`: donate merit to a random neighbour, or, with
/// `MAX_DONATE_KIN_DIST` set, to the first neighbour whose genotype is at
/// most that far away (`Systematics::SexualAncestry::GetPhyloDistance`).
/// QUIRK: an organism without a genotype (in the test CPU) fails the
/// instruction after the random turn and does not turn back.
fn inst_donate_kin(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_kin = true;

    // Find the target as the first Kin found in the neighborhood.
    let num_neighbors = world.get_num_neighbors();

    // Turn to face a random neighbor
    let mut neighbor_id = world.rng().get_int(num_neighbors);
    for _ in 0..neighbor_id {
        world.rotate(1);
    }
    let mut nb = neighbor(world, org);

    // If there is no max distance, just take the random neighbor we're facing.
    let max_dist = world.config().max_donate_kin_dist;
    if max_dist != -1 {
        let max_id = neighbor_id.wrapping_add(num_neighbors);
        let mut found = false;
        if !world.has_genotype() {
            return Ok(false);
        }
        while neighbor_id < max_id {
            nb = neighbor(world, org);
            if let Some(n) = nb {
                if world.phylo_distance_to(n.cell)? <= max_dist {
                    found = true;
                    break;
                }
            }
            world.rotate(1);
            neighbor_id += 1;
        }
        if !found {
            nb = None;
        }
    }

    // Put the facing back where it was.
    for _ in 0..neighbor_id {
        world.rotate(-1);
    }

    // Donate only if we have found a close enough relative...
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_kin = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateEditDist`: donate merit to a random neighbour, or, with
/// `MAX_DONATE_EDIT_DIST` set, to the first one within that edit distance.
fn inst_donate_edit_dist(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_edit = true;

    let max_dist = world.config().max_donate_edit_dist;
    let nb = if max_dist == -1 {
        let num_neighbors = world.get_num_neighbors();
        let neighbor_id = world.rng().get_int(num_neighbors);
        for _ in 0..neighbor_id {
            world.rotate(1);
        }
        let nb = neighbor(world, org);
        for _ in 0..neighbor_id {
            world.rotate(-1);
        }
        nb
    } else {
        let own = org.initial_genome.representation().clone();
        find_random_neighbor(world, org, &mut |w, donor, nb| {
            let mut edit_dist = max_dist.wrapping_add(1);
            if let Some(nb) = nb {
                if let Some(seq) = nb_genome(w, donor, nb) {
                    edit_dist = InstructionSequence::find_edit_distance(&own, &seq);
                }
            }
            Ok(edit_dist <= max_dist)
        })?
    };
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_edit = true;
        });
    }
    Ok(true)
}

/// `Inst_GetFacedEditDistance`: ?BX? = edit distance to the faced genome.
fn inst_get_faced_edit_distance(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if !world.is_neighbor_cell_occupied() {
        return Ok(false);
    }
    let Some(seq) = with_neighbor_state(world, org, |o| o.initial_genome.representation().clone())
    else {
        return Err("unsupported: get-faced-edit-distance facing an occupied cell without a neighbour organism (C++: cHardwareCPU.cc:cHardwareCPU::Inst_GetFacedEditDistance)".to_string());
    };
    let reg = hw.find_modified_register(REG_BX);
    let d = InstructionSequence::find_edit_distance(org.initial_genome.representation(), &seq);
    hw.set_reg(reg, d);
    Ok(true)
}

/// `Inst_DonateGreenBeardGene`: donate merit to the first neighbour whose
/// genome holds this instruction.
fn inst_donate_green_beard_gene(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_gbg = true;
    let me = ip_inst(hw);
    let nb = find_random_neighbor(world, org, &mut |w, donor, nb| {
        Ok(match nb {
            Some(nb) => nb_genome(w, donor, nb).is_some_and(|seq| genome_has(&seq, me)),
            None => false,
        })
    })?;
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_gbg = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateShadedGreenBeard`: donate merit to the first neighbour that
/// executed `donate-shadedgb` in the test CPU as often as this organism.
fn inst_donate_shaded_green_beard(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    let has_counts = !org.phenotype.test_cpu_inst_count.is_empty();
    let inst_number = last_inst_number(hw, "donate-shadedgb", has_counts);
    let shade_of_gb = inst_number.map_or(0, |i| test_cpu_count(org, i));
    let inst_number = inst_number.unwrap_or(0);

    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_shadedgb = true;
    org.phenotype.num_shaded_gb_donations = org.phenotype.num_shaded_gb_donations.wrapping_add(1);

    let nb = find_random_neighbor(world, org, &mut |w, donor, nb| {
        let Some(nb) = nb else {
            return Ok(false);
        };
        let shade = with_nb(w, donor, nb, |o| {
            if o.phenotype.test_cpu_inst_count.is_empty() {
                0
            } else {
                test_cpu_count(o, inst_number)
            }
        });
        Ok(shade == Some(shade_of_gb))
    })?;
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_shadedgb = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateTrueGreenBeard`: donate merit to the first neighbour that
/// executed `donate-tgb` in the test CPU.
///
/// The C++ dereferences the faced organism without a NULL check and indexes
/// its test CPU instruction counts without a size check; an empty faced
/// cell, or a neighbour without counts (one that has not divided under
/// `PRECALC_PHENOTYPE` 4), during the search is the `Err`.
fn inst_donate_true_green_beard(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_truegb = true;
    let inst_number = last_inst_number(hw, "donate-tgb", true).unwrap_or(0);
    let nb = find_random_neighbor(world, org, &mut |w, donor, nb| {
        let Some(nb) = nb else {
            return Err("unsupported: donate-tgb facing an empty cell (C++: cHardwareCPU.cc:cHardwareCPU::Inst_DonateTrueGreenBeard dereferences NULL)".to_string());
        };
        let count = with_nb(w, donor, nb, |o| {
            usize::try_from(inst_number)
                .ok()
                .and_then(|i| o.phenotype.test_cpu_inst_count.get(i).copied())
        });
        match count {
            Some(Some(c)) => Ok(c != 0),
            Some(None) => Err(
                "unsupported: donate-tgb reading the test CPU instruction counts \
                 of a neighbour that has none (C++: cHardwareCPU.cc:\
                 cHardwareCPU::Inst_DonateTrueGreenBeard indexes an empty array)"
                    .to_string(),
            ),
            None => Ok(false),
        }
    })?;
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_truegb = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateThreshGreenBeard`: donate merit to the first neighbour that
/// executed `donate-threshgb` at least `MIN_GB_DONATE_THRESHOLD` times in
/// the test CPU and carries this instruction.
fn inst_donate_thresh_green_beard(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_threshgb = true;
    org.phenotype.num_thresh_gb_donations = org.phenotype.num_thresh_gb_donations.wrapping_add(1);

    let has_counts = !org.phenotype.test_cpu_inst_count.is_empty();
    let inst_number = last_inst_number(hw, "donate-threshgb", has_counts).unwrap_or(0);
    let min_thresh = world.config().min_gb_donate_threshold;
    let me = ip_inst(hw);
    let nb = find_random_neighbor(world, org, &mut |w, donor, nb| {
        let Some(nb) = nb else {
            return Ok(false);
        };
        let found = with_nb(w, donor, nb, |o| {
            let thresh = if o.phenotype.test_cpu_inst_count.is_empty() {
                0
            } else {
                test_cpu_count(o, inst_number)
            };
            thresh >= min_thresh && genome_has(o.initial_genome.representation(), me)
        });
        Ok(found == Some(true))
    })?;
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_threshgb = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateQuantaThreshGreenBeard`: like the threshold green beard,
/// with the threshold the own last-life donation count rounded down to a
/// multiple of `DONATE_THRESH_QUANTA` (a 0 quantum, a division by zero in
/// C++, gives threshold 0 as on the arm64 reference build).
fn inst_donate_quanta_thresh_green_beard(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_quanta_threshgb = true;
    org.phenotype.num_quanta_thresh_gb_donations =
        org.phenotype.num_quanta_thresh_gb_donations.wrapping_add(1);

    let num_neighbors = world.get_num_neighbors();
    let mut neighbor_id = world.rng().get_int(num_neighbors);
    for _ in 0..neighbor_id {
        world.rotate(1);
    }
    let max_id = neighbor_id.wrapping_add(num_neighbors);
    let donate_quanta = world.config().donate_thresh_quanta;
    let quanta_donate_thresh = arm_div(
        org.phenotype.num_quanta_thresh_gb_donations_last,
        donate_quanta,
    )
    .wrapping_mul(donate_quanta);
    let me = ip_inst(hw);
    let mut found = None;
    while neighbor_id < max_id {
        if let Some(nb) = neighbor(world, org) {
            let ok = with_nb(world, org, nb, |o| {
                o.phenotype.num_quanta_thresh_gb_donations_last >= quanta_donate_thresh
                    && genome_has(o.initial_genome.representation(), me)
            });
            if ok == Some(true) {
                found = Some(nb);
                break;
            }
        }
        world.rotate(1);
        neighbor_id += 1;
    }
    for _ in 0..neighbor_id {
        world.rotate(-1);
    }
    if let Some(nb) = found {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_quanta_threshgb = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateGreenBeardSameLocus`: donate merit to the first neighbour
/// that donated from the same genome position last life and carries this
/// instruction there.
fn inst_donate_green_beard_same_locus(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    let donate_locus = hw.ip_head().position();
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype
        .set_is_donor_position(donate_locus.max(0) as usize);
    org.phenotype.num_donations_locus = org.phenotype.num_donations_locus.wrapping_add(1);
    let me = ip_inst(hw);
    let nb = find_random_neighbor(world, org, &mut |w, donor, nb| {
        let Some(nb) = nb else {
            return Ok(false);
        };
        let ok = with_nb(w, donor, nb, |o| {
            let seq = o.initial_genome.representation();
            o.phenotype.is_donor_position_last(donate_locus as usize)
                && seq.size() > donate_locus
                && seq.get(donate_locus) == me
        });
        Ok(ok == Some(true))
    })?;
    if let Some(nb) = nb {
        do_donate(org, world, nb);
        with_nb(world, org, nb, |o| {
            o.phenotype.is_receiver = true;
            o.phenotype.is_receiver_gb_same_locus = true;
        });
    }
    Ok(true)
}

/// `Inst_DonateNULL`: give up `MERIT_GIVEN` merit to no one (not floored).
fn inst_donate_null(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if over_donation_limit(org, world) {
        return Ok(false);
    }
    org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
    org.phenotype.is_donor_cur = true;
    org.phenotype.is_donor_null = true;
    let merit_given = world.config().merit_given;
    let mut cur_merit = org.phenotype.merit.get_double();
    cur_merit -= merit_given;
    world.update_merit(org, cur_merit);
    Ok(true)
}

/// `Inst_ReceiveDonatedEnergy`: move buffered donated energy into the store.
fn inst_receive_donated_energy(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    if org.phenotype.energy_received_buffer > 0.0 {
        let cfg = world.config().clone();
        org.phenotype.apply_donated_energy(&cfg);
        if cfg.energy_sharing_update_metabolic {
            let m = energy_merit(org, &cfg);
            world.update_merit(org, m);
        }
    }
    Ok(true)
}

/// `Inst_DonateEnergy`: answer the oldest received message (an energy
/// request) by donating to its sender, if the sender is still in its cell.
fn inst_donate_energy(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let Some(msg) = org_retrieve_message(org) else {
        return Ok(false);
    };
    let receiver_id = world.cell_organism_id(msg.sender_cell);
    if receiver_id < 0 || receiver_id != msg.sender_org_id {
        return Ok(false);
    }
    let Some(uid) = world.cell_organism_uid(msg.sender_cell) else {
        return Ok(false);
    };
    let to = Nb {
        id: receiver_id,
        uid,
        cell: msg.sender_cell,
    };
    let frac = org.frac_energy_donating;
    do_energy_donate_percent(org, world, to, frac);
    Ok(true)
}

/// `Inst_UpdateMetabolicRate`: merit from the stored energy.
fn inst_update_metabolic_rate(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let m = energy_merit(org, world.config());
    world.update_merit(org, m);
    Ok(true)
}

/// `Inst_DonateEnergyFaced` (`amount` `None`: `GetFracEnergyDonating()` of
/// the store) and `Inst_DonateEnergyFaced<N>` (`N` units): donate to a live
/// faced organism with an open energy request (any, under
/// `ENERGY_SHARING_METHOD` 1).
fn donate_energy_faced(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    amount: Option<f64>,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let push = world.config().energy_sharing_method == 1;
    if let Some(nb) = neighbor(world, org) {
        let wants = with_nb(world, org, nb, |o| {
            !o.is_dead && (o.phenotype.has_open_energy_request || push)
        });
        if wants == Some(true) {
            match amount {
                None => {
                    let frac = org.frac_energy_donating;
                    do_energy_donate_percent(org, world, nb, frac);
                }
                Some(a) => do_energy_donate_amount(org, world, nb, a),
            }
        }
    }
    Ok(true)
}

/// `Inst_RotateToMostNeedy`
///
/// QUIRK: `min_energy` is never lowered, so this turns to the last
/// qualifying neighbour with energy below `ENERGY_CAP`, and by one turn
/// less than its index (the scan turns before it looks).
fn inst_rotate_to_most_needy(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let min_energy = world.config().energy_cap;
    let push = world.config().energy_sharing_method == 1;
    let mut num_rotations = 0;
    for i in 0..world.get_num_neighbors() {
        world.rotate(1);
        let Some(nb) = neighbor(world, org) else {
            continue;
        };
        let energy = with_nb(world, org, nb, |o| {
            (!o.is_dead && (o.phenotype.has_open_energy_request || push))
                .then_some(o.phenotype.energy_store)
        })
        .flatten();
        if let Some(neighbor_energy) = energy {
            if neighbor_energy > 0.0 && neighbor_energy < min_energy {
                num_rotations = i;
            }
        }
    }
    for _ in 0..num_rotations {
        world.rotate(1);
    }
    Ok(true)
}

/// `Inst_RequestEnergy`: broadcast an energy request over
/// `ENERGY_REQUEST_RADIUS`.
fn inst_request_energy(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    let mut msg = new_message(org, world, 0);
    let radius = world.config().energy_request_radius;
    org_broadcast_message(hw, org, world, &mut msg, radius)?;
    org.phenotype.is_energy_requestor = true;
    org.phenotype.num_energy_requests = org.phenotype.num_energy_requests.wrapping_add(1);
    Ok(true)
}

/// `Inst_RequestEnergyFlagOn`
fn inst_request_energy_flag_on(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if world.get_cell_id() < 0 {
        return Ok(false);
    }
    org.phenotype.is_energy_requestor = true;
    org.phenotype.num_energy_requests = org.phenotype.num_energy_requests.wrapping_add(1);
    org.phenotype.has_open_energy_request = true;
    Ok(true)
}

/// `DonateResourceX(ctx, res_id)`: give 10% of resource `res_id` in this
/// cell to the faced cell (`DoResourceDonatePercent` /
/// `DoResourceDonateAmount`), less `RESOURCE_SHARING_LOSS`.
fn donate_resource_x(org: &mut OrgState, world: &mut dyn OrgInterface, res_id: usize) -> R {
    let pct = 0.1;
    let current_cell = world.get_cell_id();
    if current_cell == -1 {
        return Ok(false);
    }
    let faced_cell = world.get_faced_cell_id();
    if faced_cell == -1 {
        return Ok(false);
    }
    // DoResourceDonatePercent
    let resources = world.get_resources(org);
    let Some(&have) = resources.get(res_id) else {
        return Ok(true);
    };
    let amount = cmax(0.0, pct * have);
    // DoResourceDonateAmount
    let src_resources = world.get_resources(org);
    let dest_resources = world.get_cell_resources(org, faced_cell);
    let donation = cmin(amount, src_resources.get(res_id).copied().unwrap_or(0.0));
    let decay = world.config().resource_sharing_loss;
    let mut src_change = vec![0.0; src_resources.len()];
    let mut dest_change = vec![0.0; dest_resources.len()];
    if let Some(v) = src_change.get_mut(res_id) {
        *v = -donation;
    }
    if let Some(v) = dest_change.get_mut(res_id) {
        *v = (1.0 - decay) * donation;
    }
    world.update_resources(org, &src_change);
    world.update_cell_resources(org, &dest_change, faced_cell);
    Ok(true)
}

/// `Inst_DonateSpecific`: hand one unit of the `COLLECT_SPECIFIC_RESOURCE`
/// bin to the faced organism (resource bins only).
fn inst_donate_specific(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let cfg = world.config();
    let n = org.phenotype.cur_num_donates;
    if n > cfg.max_donates || (cfg.max_donate_edit_dist > 0 && n > cfg.max_donate_edit_dist) {
        return Ok(false);
    }
    if !world.is_neighbor_cell_occupied() {
        return Ok(false);
    }
    let resource = world.config().collect_specific_resource;
    if world.config().use_resource_bins {
        let res_before = org.rbin(resource);
        if res_before >= 1.0 {
            // `target->AddToRBin(resource, 1)` and, after the donor's
            // counts (independent fields), `SetIsReceiverEdit()`.
            let got = with_neighbor_state(world, org, |o| {
                o.add_to_rbin(resource, 1.0);
                o.phenotype.is_receiver = true;
                o.phenotype.is_receiver_edit = true;
            });
            if got.is_none() {
                return Err("unsupported: donate-specific facing an occupied cell without a neighbour organism (C++: cHardwareCPU.cc:cHardwareCPU::Inst_DonateSpecific)".to_string());
            }
            org.phenotype.cur_num_donates = org.phenotype.cur_num_donates.wrapping_add(1);
            org.phenotype.is_donor_cur = true;
            org.phenotype.is_donor_edit = true;
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------
// Reputation and raw-material strings
// ---------------------------------------------------------------------

/// `cOrganism::GetReputation()`: the newest opinion, 0 without one.
pub(crate) fn get_reputation(org: &mut OrgState) -> i32 {
    if org.has_opinion() {
        org.get_opinion().0
    } else {
        0
    }
}

/// `cOrganism::SetReputation(rep)`: `SetOpinion(rep)`.
fn set_reputation(
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    rep: i32,
) -> Result<(), String> {
    let update = world.update();
    org.set_opinion(world.config(), update, rep)
}

/// `neighbor->GetReputation()`
fn nb_reputation(world: &mut dyn OrgInterface, org: &mut OrgState, nb: Nb) -> i32 {
    with_nb(world, org, nb, get_reputation).unwrap_or(0)
}

/// `neighbor->SetReputation(rep)`
fn nb_set_reputation(
    world: &mut dyn OrgInterface,
    org: &mut OrgState,
    nb: Nb,
    rep: i32,
) -> Result<(), String> {
    let bsize = world.config().opinion_buffer_size;
    let update = world.update();
    with_nb(world, org, nb, |o| o.set_opinion_bsize(bsize, update, rep)).unwrap_or(Ok(()))
}

/// `ComputeReputation()`: the `AUTO_REPUTATION` bookkeeping after a
/// donation to the faced organism.
fn compute_reputation(org: &mut OrgState, world: &mut dyn OrgInterface) -> Result<(), String> {
    let nb = neighbor(world, org);
    let mode = world.config().auto_reputation;
    let need_nb = || {
        nb.ok_or_else(|| "unsupported: reputation update without a faced organism (C++: cHardwareCPU.cc:cHardwareCPU::ComputeReputation dereferences NULL)".to_string())
    };
    match mode {
        1 => {
            let my_rep = get_reputation(org);
            set_reputation(org, world, my_rep.wrapping_add(1))?;
            let nb = need_nb()?;
            if nb_reputation(world, org, nb) == 0 {
                nb_set_reputation(world, org, nb, -1)?;
            }
        }
        2 => {
            let my_rep = get_reputation(org);
            set_reputation(org, world, my_rep.wrapping_add(1))?;
            let nb = need_nb()?;
            let rep = nb_reputation(world, org, nb);
            nb_set_reputation(world, org, nb, rep.wrapping_sub(1))?;
        }
        3 => {
            set_reputation(org, world, 1)?;
            let nb = need_nb()?;
            if nb_reputation(world, org, nb) == 0 {
                nb_set_reputation(world, org, nb, -1)?;
            }
        }
        4 => {
            let my_rep = get_reputation(org);
            set_reputation(org, world, my_rep.wrapping_add(1))?;
        }
        _ => {}
    }
    Ok(())
}

/// `(*m_string_map)[tag]`: the entry, inserted as zeros when missing
/// (`std::map::operator[]`).
fn string_entry(map: &mut Vec<(i32, i32, i32)>, tag: i32) -> &mut (i32, i32, i32) {
    let i = match map.binary_search_by_key(&tag, |e| e.0) {
        Ok(i) => i,
        Err(i) => {
            map.insert(i, (tag, 0, 0));
            i
        }
    };
    &mut map[i]
}

/// `GetNumberStringsOnHand(i)`: 0 before `InitStringMap`.
pub(crate) fn strings_on_hand(org: &mut OrgState, i: i32) -> i32 {
    match org.string_map.as_mut() {
        None => 0,
        Some(m) => string_entry(m, i).2,
    }
}

/// `CanReceiveString(tag, amount)`: below `STRING_AMOUNT_CAP` (-1: no cap).
/// `None` before `InitStringMap` (a NULL dereference in C++).
fn can_receive_string(org: &mut OrgState, tag: i32, cap: i32) -> Option<bool> {
    let m = org.string_map.as_mut()?;
    let e = string_entry(m, tag);
    Some(cap == -1 || e.2 < cap)
}

/// `AddOtherRawMaterials(amount, donor_id)`
fn add_other_raw_materials(org: &mut OrgState, amount: i32, donor_id: i32) {
    org.other_raw_materials = org.other_raw_materials.wrapping_add(amount);
    org.donor_list.insert(donor_id);
    org.num_donate_received = org.num_donate_received.wrapping_add(amount);
    org.amount_donate_received = org.amount_donate_received.wrapping_add(1);
}

/// `InitStringMap()`: one entry per task-library match string.
fn init_string_map(org: &mut OrgState, world: &dyn OrgInterface) {
    if org.string_map.is_none() {
        let n = world.environment().tasklib().0.get_match_strings().len();
        org.string_map = Some((0..n as i32).map(|i| (i, 0, 0)).collect());
    }
}

/// The NULL-map dereference shared by the string donations.
fn null_string_map(inst: &str) -> String {
    format!("unsupported: {inst} before the string map exists (C++: cOrganism.cc string-map dereference of NULL)")
}

/// `Inst_DonateFacingString`: give `ALT_COST` of the own string (tagged by
/// lineage) to the faced organism, unless `DONATION_FAILURE_PERCENT` hits.
fn donate_facing_string(org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let Some(nb) = neighbor(world, org) else {
        return Ok(true);
    };
    let cost = world.config().alt_cost;
    let cap = world.config().string_amount_cap;
    let my_string = org.lineage_label;
    if strings_on_hand(org, my_string) < cost {
        return Ok(true);
    }
    let can = with_nb(world, org, nb, |o| can_receive_string(o, my_string, cap)).flatten();
    match can {
        None => return Err(null_string_map("donate-facing-string")),
        Some(false) => return Ok(true),
        Some(true) => {}
    }
    let prob_fail = world.config().donation_failure_percent as u32;
    let rand_num = world.rng().get_uint_range(0, 100);
    if rand_num < prob_fail {
        return Ok(true);
    }
    // DonateString
    let Some(m) = org.string_map.as_mut() else {
        return Err(null_string_map("donate-facing-string"));
    };
    let e = string_entry(m, my_string);
    if e.2 >= cost {
        e.2 = e.2.wrapping_sub(cost);
    }
    let my_id = org.id;
    let my_lineage = org.lineage_label;
    with_nb(world, org, nb, |o| {
        add_other_raw_materials(o, cost, my_id);
        // ReceiveString
        if let Some(m) = o.string_map.as_mut() {
            let e = string_entry(m, my_string);
            if cap == -1 || e.2 < cap {
                e.2 = e.2.wrapping_add(1);
                o.donor_list.insert(my_id);
                o.num_donate_received = o.num_donate_received.wrapping_add(cost);
                o.amount_donate_received = o.amount_donate_received.wrapping_add(1);
            }
        }
        o.donating_lineages.insert(my_lineage);
    });
    org.num_donate = org.num_donate.wrapping_add(1);
    compute_reputation(org, world)?;
    Ok(true)
}

/// `Inst_DonateFacingRawMaterials`: give `ALT_COST` raw materials to the
/// faced organism, unless `DONATION_FAILURE_PERCENT` hits; with
/// `ROTATE_ON_DONATE` the recipient turns to face the donor.
fn donate_facing_raw_materials(org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let Some(nb) = neighbor(world, org) else {
        return Ok(true);
    };
    let cost = world.config().alt_cost;
    // SubtractSelfRawMaterials
    if cost > org.self_raw_materials {
        return Ok(true);
    }
    org.self_raw_materials = org.self_raw_materials.wrapping_sub(cost);
    let prob_fail = world.config().donation_failure_percent as u32;
    let rand_num = world.rng().get_uint_range(0, 100);
    if rand_num < prob_fail {
        return Ok(true);
    }
    let my_id = org.id;
    let my_lineage = org.lineage_label;
    with_nb(world, org, nb, |o| {
        add_other_raw_materials(o, cost, my_id);
        o.donating_lineages.insert(my_lineage);
    });
    if world.config().rotate_on_donate != 0 {
        let me = world.get_cell_id();
        world.turn_cell_to_face(nb.cell, me);
    }
    org.num_donate = org.num_donate.wrapping_add(1);
    compute_reputation(org, world)?;
    Ok(true)
}

/// `Inst_DonateFacingRawMaterialsOtherSpecies`: donate a string to a faced
/// organism of another lineage.
fn donate_other_species(org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    if let Some(nb) = neighbor(world, org) {
        let spec_neighbor = with_nb(world, org, nb, |o| o.lineage_label);
        if spec_neighbor.is_some_and(|s| s != org.lineage_label) {
            donate_facing_string(org, world)?;
        }
    }
    Ok(true)
}

/// `Inst_DonateIfDonor`: return a donation to a faced former donor.
fn inst_donate_if_donor(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    if let Some(nb) = neighbor(world, org) {
        if org.donor_list.contains(&nb.id) {
            world.stats_inc_donate_to_donor();
            donate_other_species(org, world)?;
        }
    }
    Ok(true)
}

/// `Inst_GetNeighborsReputation`: ?AX? = the faced organism's reputation
/// (the register is untouched without one; its nop is read either way).
fn inst_get_neighbors_reputation(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    // The reference binary passes `FindModifiedRegister(REG_AX)` as an
    // argument to `avd_cpu_inst_get_neighbors_reputation`, so the trailing
    // nop is consumed before the helper's neighbour check (47f13dadb read it
    // only with a neighbour).
    let raw_mat_reg = hw.find_modified_register(REG_AX);
    if let Some(nb) = neighbor(world, org) {
        let rep = nb_reputation(world, org, nb);
        hw.set_reg(raw_mat_reg, rep);
    }
    Ok(true)
}

/// `Inst_RotateToDifferentTag`: from a random neighbour on, turn to the
/// first one with another tag (staying there).
fn inst_rotate_to_different_tag(
    _: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let num_neighbors = world.get_num_neighbors();
    let mut neighbor_id = world.rng().get_int(num_neighbors);
    for _ in 0..neighbor_id {
        world.rotate(1);
    }
    let max_id = neighbor_id.wrapping_add(num_neighbors);
    let my_tag = org.tag.0;
    while neighbor_id < max_id {
        if let Some(nb) = neighbor(world, org) {
            if with_nb(world, org, nb, |o| o.tag.0).is_some_and(|t| t != my_tag) {
                break;
            }
        }
        world.rotate(1);
        neighbor_id += 1;
    }
    Ok(true)
}

/// `Inst_IfDonor`: execute the next instruction only if the faced organism
/// has donated to this one.
fn inst_if_donor(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let donor = neighbor(world, org).is_some_and(|nb| org.donor_list.contains(&nb.id));
    if !donor {
        hw.ip_advance();
    }
    Ok(true)
}

/// `cOrganism::MatchOutputBuffer(s)`: positions where `s` ('0' / '1')
/// matches the output buffer, newest output first.
fn match_output_buffer(org: &OrgState, s: &str) -> i32 {
    let mut num_matched = 0;
    for (j, c) in s.bytes().enumerate() {
        let v = org.output_buf.get(j as i32);
        if (c == b'0' && v == 0) || (c == b'1' && v == 1) {
            num_matched += 1;
        }
    }
    num_matched
}

/// `Inst_ProduceString`: when the output buffer spells a whole match
/// string, produce it, gain a raw material and blank the buffer.
fn inst_produce_string(_: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let mut max_num = 0;
    let mut max_string = -1;
    let mut string_size = 0;
    init_string_map(org, world);
    let temp_strings = world.environment().tasklib().0.get_match_strings();
    if let Some(s) = temp_strings.first() {
        string_size = s.len() as i32;
    }
    for (i, s) in temp_strings.iter().enumerate() {
        let num = match_output_buffer(org, s);
        if num > max_num {
            max_num = num;
            max_string = i as i32;
        }
    }
    if world.config().specialists != 0 && org.lineage_label != max_string {
        max_num = 0;
    }
    if max_num == string_size {
        // ProduceString
        let cap = world.config().string_amount_cap;
        let mut val = false;
        if let Some(m) = org.string_map.as_mut() {
            let e = string_entry(m, max_string);
            if cap == -1 || e.2 < cap {
                e.1 = e.1.wrapping_add(1);
                e.2 = e.2.wrapping_add(1);
                val = true;
            }
        }
        // AddSelfRawMaterials(1)
        if org.self_raw_materials < 10 {
            org.self_raw_materials = org.self_raw_materials.wrapping_add(1);
        }
        if val {
            // SetOutputNegative1
            for _ in 0..org.output_buf.capacity() {
                org.output_buf.add(-1);
            }
            org.output_buf.clear();
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_entries_insert_in_tag_order() {
        let mut m = vec![(0, 1, 1), (2, 0, 3)];
        string_entry(&mut m, 1).2 = 5;
        string_entry(&mut m, -1);
        assert_eq!(m, vec![(-1, 0, 0), (0, 1, 1), (1, 0, 5), (2, 0, 3)]);
        assert_eq!(string_entry(&mut m, 2).2, 3);
    }

    #[test]
    fn std_min_max_keep_the_first_argument_on_ties_and_nan() {
        assert_eq!(cmin(1.0, 2.0), 1.0);
        assert!(cmin(f64::NAN, 1.0).is_nan());
        assert_eq!(cmin(1.0, f64::NAN), 1.0);
        assert_eq!(cmax(0.0, -1.0), 0.0);
        assert_eq!(cmax(-1.0, 0.0), 0.0);
    }
}
