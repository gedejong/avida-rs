//! Predator/prey bookkeeping of `cPopulation` (avida-core/source/main/
//! cPopulation.cc at 47f13dadb): the prey / predator / top-predator head
//! counts `ActivateOrganism` and `KillOrganism` keep when
//! `PRED_PREY_SWITCH` is -2 or above -1, `UpdateFTOrgStats`, and the random
//! culls `KillRandPred` / `KillRandPrey` that `cOrganism::SetForageTarget`
//! and `PositionOffspring` run when `MAX_PRED` / `MAX_PREY` is reached.
//!
//! A forage target above -2 is prey (`cOrganism::IsPreyFT`), -2 a predator
//! (`IsPredFT`) and below -2 a top predator (`IsTopPredFT`).
//!
//! Also the prey refill of a successful attack below `-MIN_PREY` prey
//! (`cPopulationInterface::InjectPreyClone`, `cPopulation::InjectPreyClone`
//! and its `InjectClone`) and `cStats::PrintBirthLocData`
//! (`TRACK_BIRTH_LOCS`).

use avida_apto::Accumulator;
use avida_hardware::organism::{transmission, OrgSource, OrgState, Organism};
use avida_systematics::unit::{GroupMembership, UnitInfo};

use crate::deme_compete::CloneOrigin;
use crate::pop_orgs::systematics_source;
use crate::population::position_offspring as po;
use crate::world::World;

/// The forager class of a forage target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FtClass {
    Prey,
    Pred,
    TopPred,
}

fn ft_class(ft: i32) -> FtClass {
    if ft > -2 {
        FtClass::Prey
    } else if ft == -2 {
        FtClass::Pred
    } else {
        FtClass::TopPred
    }
}

impl World {
    /// `PRED_PREY_SWITCH == -2 || PRED_PREY_SWITCH > -1`: the population
    /// keeps forager-class head counts and `UpdateFTOrgStats` runs.
    pub(crate) fn tracks_pred_prey(&self) -> bool {
        self.cfg.pred_prey_switch == -2 || self.cfg.pred_prey_switch > -1
    }

    /// The counting half of `ActivateOrganism`: "ft should be nearly always
    /// -1 so long as it is not being inherited".
    pub(crate) fn count_pred_prey_birth(&mut self, ft: i32) {
        if !self.tracks_pred_prey() {
            return;
        }
        match ft_class(ft) {
            FtClass::Prey => self.pop.num_prey_organisms += 1,
            FtClass::TopPred => self.pop.num_top_pred_organisms += 1,
            FtClass::Pred => self.pop.num_pred_organisms += 1,
        }
    }

    /// The counting half of `KillOrganism`, with the forage target it read
    /// before `NotifyDeath`. `None` when the dying organism is the one
    /// being stepped and its caller did not pass its state.
    pub(crate) fn count_pred_prey_death(&mut self, ft: Option<i32>) {
        if !self.tracks_pred_prey() {
            return;
        }
        let Some(ft) = ft else {
            self.set_fatal(
                "unsupported: KillOrganism of the running organism without its state \
                 while PRED_PREY_SWITCH counts forager classes",
            );
            return;
        };
        match ft_class(ft) {
            FtClass::Prey => self.pop.num_prey_organisms -= 1,
            FtClass::Pred => self.pop.num_pred_organisms -= 1,
            FtClass::TopPred => self.pop.num_top_pred_organisms -= 1,
        }
    }

    /// `(GetForageTarget(), GetParentFT())` of the organism in `cell`;
    /// `me` stands in for the organism `me_cell` belongs to, which is out
    /// of its slot while it runs.
    fn ft_and_parent_ft(&self, cell: i32, me_cell: i32, me: &OrgState) -> Option<(i32, i32)> {
        if cell == me_cell {
            return Some((me.forage_target, me.parent_ft));
        }
        self.pop
            .orgs
            .get(usize::try_from(cell).ok()?)?
            .as_ref()
            .map(|o| (o.state.forage_target, o.state.parent_ft))
    }

    /// The `TriedIdx` draw of `KillRandPred` / `KillRandPrey`: pick a live
    /// organism (by cell) satisfying `wanted`, other than `me_cell`'s.
    ///
    /// QUIRK: an entry that is the caller itself and satisfies `wanted` is
    /// neither taken nor swapped out, so it can be drawn again; and one more
    /// `GetUInt` is drawn after a pick unless only one entry is left.
    fn rand_live_org_where(
        &mut self,
        me_cell: i32,
        me: &OrgState,
        wanted: impl Fn(i32, i32) -> bool,
    ) -> Option<i32> {
        let mut tried: Vec<i32> = self.pop.live_org_list.clone();
        let mut list_size = tried.len();
        let mut chosen: Option<i32> = None;
        let mut idx = self.rng.get_uint(list_size as u32) as usize;
        while chosen.is_none() {
            let Some(&cell_at) = tried.get(idx) else {
                // The C++ reads past its array here.
                self.set_fatal("KillRandPred/KillRandPrey ran out of candidates");
                return None;
            };
            let take = self
                .ft_and_parent_ft(cell_at, me_cell, me)
                .is_some_and(|(ft, parent_ft)| wanted(ft, parent_ft));
            if take {
                if cell_at != me_cell {
                    chosen = Some(cell_at);
                }
            } else {
                list_size -= 1;
                tried.swap(idx, list_size);
            }
            if list_size == 1 {
                break;
            }
            if list_size == 0 {
                // The C++ would swap with index -1 next.
                self.set_fatal("KillRandPred/KillRandPrey emptied its candidate list");
                return None;
            }
            idx = self.rng.get_uint(list_size as u32) as usize;
        }
        chosen
    }

    /// `cPopulation::KillRandPred(ctx, org)`: kill a random live organism
    /// that is not prey (or whose parent was a predator); `me` is `org`,
    /// living in `me_cell`.
    /// The victim is never `org` itself.
    pub(crate) fn kill_rand_pred(&mut self, me_cell: i32, me: &OrgState) {
        // exclude prey
        let victim =
            self.rand_live_org_where(me_cell, me, |ft, parent_ft| parent_ft <= -2 || ft <= -2);
        if let Some(cell) = victim {
            self.kill_organism(cell, None);
        }
    }

    /// `cPopulation::KillRandPrey(ctx, org)`
    pub(crate) fn kill_rand_prey(&mut self, me_cell: i32, me: &OrgState) {
        // exclude predators and juvenilles with predatory parents (include
        // juvs with non-predatory parents)
        let victim = self.rand_live_org_where(me_cell, me, |ft, parent_ft| {
            ft > -1 || (ft == -1 && parent_ft > -2)
        });
        if let Some(cell) = victim {
            self.kill_organism(cell, None);
        }
    }

    /// `cPopulationInterface::InjectPreyClone(ctx, gen_id)` for the running
    /// organism `me` (of cell `me_cell`, genotype `gen_id`): draw live
    /// organisms (the `TriedIdx` walk of `KillRandPred`) until one is prey
    /// past its juvenile stage, of another genotype and no clone itself,
    /// and inject a clone of it (`cPopulation::InjectPreyClone`).
    ///
    /// QUIRK: one more `GetUInt` is drawn after a pick unless one entry is
    /// left, and the last entry left is never tried. Where the C++ walks
    /// out of its array (no live organisms, or none left to try) the port
    /// stops.
    pub(crate) fn inject_prey_clone(
        &mut self,
        me_cell: i32,
        me: &mut OrgState,
        gen_id: i32,
    ) -> Result<(), String> {
        let mut tried: Vec<i32> = self.pop.live_org_list.clone();
        let mut list_size = tried.len();
        let mut org_to_clone: Option<i32> = None;
        let mut idx = self.rng.get_uint(list_size as u32) as usize;
        while org_to_clone.is_none() {
            let Some(&cell_at) = tried.get(idx) else {
                return Err(
                    "InjectPreyClone with no live organism to try (C++: reads past \
                            cPopulationInterface::InjectPreyClone's TriedIdx)"
                        .to_string(),
                );
            };
            // exclude pred and juvs & the genotype of the org that was being
            // killed and triggered this cloning
            if self.prey_clone_source_ok(cell_at, me_cell, me, gen_id)? {
                // only clone orgs that can reproduce on their own
                org_to_clone = Some(cell_at);
            } else {
                list_size -= 1;
                tried.swap(idx, list_size);
            }
            if list_size == 1 {
                break;
            }
            if list_size == 0 {
                return Err(
                    "InjectPreyClone found no organism to clone (C++: TriedIdx.Swap(idx, \
                            -1), out of bounds, in cPopulationInterface::InjectPreyClone)"
                        .to_string(),
                );
            }
            idx = self.rng.get_uint(list_size as u32) as usize;
        }
        let Some(src_cell) = org_to_clone else {
            return Ok(());
        };

        // `cPopulation::InjectPreyClone(ctx, org_to_clone)`
        let src_state = if self.runs_in(src_cell) {
            me.clone()
        } else {
            match self.pop.orgs[src_cell as usize].as_ref() {
                Some(o) => o.state.clone(),
                None => return Err("InjectPreyClone of an empty cell".to_string()),
            }
        };
        let target_cell = if self.runs_in(src_cell) {
            self.position_offspring(src_cell, me, false)
        } else {
            let mut held = src_state.clone();
            self.position_offspring(src_cell, &mut held, false)
        };
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Err(f);
        }
        if target_cell < 0 || target_cell as usize >= self.pop.cells.len() {
            return Err(format!(
                "InjectPreyClone into cell {target_cell} (C++: cPopulation::PositionOffspring \
                 returned no cell)"
            ));
        }
        let src = OrgSource::new(transmission::DUPLICATION, "", false);
        self.inject_clone_running(target_cell, &src_state, src, me)
    }

    /// Whether the live organism in `cell` may be cloned by
    /// `InjectPreyClone`: `GetForageTarget() > -1`, its genotype is not
    /// `gen_id`, and it is not a clone itself (`me` stands in for the
    /// running organism of `me_cell`).
    fn prey_clone_source_ok(
        &self,
        cell: i32,
        me_cell: i32,
        me: &OrgState,
        gen_id: i32,
    ) -> Result<bool, String> {
        let (ft, is_clone) = if cell == me_cell && self.runs_in(cell) {
            (me.forage_target, me.phenotype.is_clone)
        } else {
            match self.pop.orgs.get(cell as usize).and_then(|o| o.as_ref()) {
                Some(o) => (o.state.forage_target, o.state.phenotype.is_clone),
                None => {
                    return Err(format!(
                        "live organism list names the empty cell {cell} (InjectPreyClone)"
                    ))
                }
            }
        };
        if ft <= -1 {
            return Ok(false);
        }
        let genotype = self
            .pop
            .membership
            .get(cell as usize)
            .and_then(|m| self.systematics.genotype_of(m))
            .and_then(|g| self.systematics.group_id(g));
        let Some(genotype) = genotype else {
            return Err(
                "InjectPreyClone of an organism without a genotype (C++: NULL \
                        SystematicsGroup(\"genotype\"))"
                    .to_string(),
            );
        };
        Ok(genotype != gen_id && !is_clone)
    }

    /// Whether `cell` is the cell of the organism being run, which is out of
    /// its slot while it runs (and still lives there).
    fn runs_in(&self, cell: i32) -> bool {
        self.pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed)
    }

    /// `cPopulation::InjectClone(cell_id, orig_org, src)` from inside the
    /// step of the organism `running` (the clone may replace it), with its
    /// avatar block (`PlaceAvatar` next to `orig_org`, `TryWriteBirthLocData`,
    /// `AddPredPreyAV`, `AVATAR_BIRTH_FACING`).
    fn inject_clone_running(
        &mut self,
        cell_id: i32,
        orig_org: &OrgState,
        src: OrgSource,
        running: &mut OrgState,
    ) -> Result<(), String> {
        let c = cell_id as usize;
        let origin = CloneOrigin::of(orig_org);
        let mut org = Organism::new(self, &origin.genome, origin.generation, src)?;

        // Classify the new organism
        let mut membership = GroupMembership::new();
        {
            let source = systematics_source(&org.state.src);
            let unit = UnitInfo {
                source: &source,
                genome: org.state.genome(),
                generation: org.state.phenotype.generation,
            };
            self.systematics
                .classify_new_unit(&unit, None, &mut membership);
        }

        // Setup the phenotype...
        let update = self.stats.update();
        org.state
            .phenotype
            .setup_clone(&self.cfg, &origin.phenotype, update, &mut self.rng);

        // Prep the cell..
        if self.cfg.birth_method == po::FULL_SOUP_ELDEST && self.is_occupied(cell_id) {
            // Have to manually take this cell out of the reaper Queue.
            if let Some(pos) = self.pop.reaper_queue.iter().position(|&x| x == cell_id) {
                self.pop.reaper_queue.remove(pos);
            }
        }

        // Setup the mutation rate based on the population cell...
        if self.cfg.mut_rate_source == 1 {
            // Update the mutation rates of each child from the environment....
            org.state.mut_rates.copy_from(&self.pop.cells[c].mut_rates);
        } else {
            // Update the mutation rates of each child from its parent.
            org.state.mut_rates.copy_from(&origin.mut_rates);
        }

        // since this is a clone, we want some of the parent data from the
        // genome source (the source is not the parent, the source's parent
        // is)
        if self.cfg.use_form_groups != 0 {
            org.state.parent_group = origin.parent_group;
        }
        if origin.parent_teacher {
            org.state.parent_teacher = true;
        }
        org.state.parent_ft = origin.parent_ft;

        // Activate the organism in the population...
        let org_survived =
            self.activate_organism(org, membership, cell_id, true, true, Some(running));
        // only assign an avatar cell if the org lived through birth
        self.place_offspring_avatar(cell_id, org_survived, orig_org.uid);
        if let Some(f) = self.fatal.borrow_mut().take() {
            return Err(f);
        }
        Ok(())
    }

    /// `TryWriteBirthLocData(GetOrgIndex())` → `cStats::PrintBirthLocData`
    /// for the organism in `cell`: with `TRACK_BIRTH_LOCS`, a `birthlocs`
    /// row of the update, its id, its parent's forage target and its
    /// (avatar) birth cell.
    pub(crate) fn try_write_birth_loc_data_of_cell(&mut self, cell: i32) {
        if self.cfg.track_birth_locs == 0 {
            return;
        }
        let row = self
            .pop
            .orgs
            .get(cell as usize)
            .and_then(|o| o.as_ref())
            .map(|o| self.birth_loc_row(&o.state));
        match row {
            Some(row) => self.write_birth_loc_row(&row),
            None => self.set_fatal(format!(
                "TryWriteBirthLocData for the empty cell {cell} (C++: NULL organism)"
            )),
        }
    }

    /// [`World::try_write_birth_loc_data_of_cell`] for an organism held out
    /// of its cell.
    pub(crate) fn try_write_birth_loc_data(&mut self, org: &OrgState) {
        if self.cfg.track_birth_locs == 0 {
            return;
        }
        let row = self.birth_loc_row(org);
        self.write_birth_loc_row(&row);
    }

    /// `update,id,parent_ft,x,y` as `PrintBirthLocData` writes it.
    fn birth_loc_row(&self, org: &OrgState) -> String {
        let worldx = self.cfg.world_x;
        let loc = if self.cfg.use_avatars != 0 {
            org.phenotype.av_birth_cell_id
        } else {
            org.phenotype.birth_cell_id
        };
        format!(
            "{},{},{},{},{}",
            self.stats.update(),
            org.id,
            org.parent_ft,
            loc.wrapping_rem(worldx),
            loc.wrapping_div(worldx)
        )
    }

    fn write_birth_loc_row(&mut self, row: &str) {
        let df = match self.out.static_with_path("birthlocs") {
            Ok(df) => df,
            Err(e) => {
                self.set_fatal(e);
                return;
            }
        };
        if !df.header_done() {
            df.write_comment("Update,OrgID,ParentFT,BirthCellX,BirthCellY");
            df.flush_comments();
            df.endl();
        }
        let _ = writeln!(df.ofstream(), "{row}");
        df.flush();
    }

    /// The `MAX_PREY` cull at the end of `PositionOffspring`: "for juvs with
    /// non-predatory parents...".
    pub(crate) fn position_offspring_max_prey(&mut self, parent_cell: i32, parent: &OrgState) {
        let max_prey = self.cfg.max_prey;
        if max_prey != 0 && self.pop.num_prey_organisms >= max_prey && parent.forage_target > -2 {
            self.kill_rand_prey(parent_cell, parent);
        }
    }

    /// `cPopulation::UpdateFTOrgStats(ctx)`: per-organism sums for prey,
    /// predators and top predators.
    pub fn update_ft_org_stats(&mut self) {
        let stats = &mut self.stats;
        // Clear out organism sums...
        stats.sum_prey_fitness.clear();
        stats.sum_prey_gestation.clear();
        stats.sum_prey_merit.clear();
        stats.sum_prey_creature_age.clear();
        stats.sum_prey_generation.clear();

        stats.sum_pred_fitness.clear();
        stats.sum_pred_gestation.clear();
        stats.sum_pred_merit.clear();
        stats.sum_pred_creature_age.clear();
        stats.sum_pred_generation.clear();

        stats.sum_tpred_fitness.clear();
        stats.sum_tpred_gestation.clear();
        stats.sum_tpred_merit.clear();
        stats.sum_tpred_creature_age.clear();
        stats.sum_tpred_generation.clear();

        stats.sum_attacks.clear();
        stats.sum_kills.clear();

        stats.zero_ft_inst();
        stats.zero_group_attack_inst();

        for &c in &self.pop.live_org_list {
            let Some(org) = self.pop.orgs[c as usize].as_ref() else {
                continue;
            };
            let phenotype = &org.state.phenotype;
            let cur_merit = phenotype.merit.get_double();
            let cur_fitness = phenotype.fitness;
            let inst_set = org.state.genome().inst_set().to_string();
            let class = ft_class(org.state.forage_target);
            match class {
                FtClass::Prey => {
                    stats.sum_prey_fitness.add(cur_fitness);
                    stats
                        .sum_prey_gestation
                        .add(f64::from(phenotype.gestation_time));
                    stats.sum_prey_merit.add(cur_merit);
                    stats.sum_prey_creature_age.add(f64::from(phenotype.age));
                    stats
                        .sum_prey_generation
                        .add(f64::from(phenotype.generation));
                }
                FtClass::Pred => {
                    stats.sum_pred_fitness.add(cur_fitness);
                    stats
                        .sum_pred_gestation
                        .add(f64::from(phenotype.gestation_time));
                    stats.sum_pred_merit.add(cur_merit);
                    stats.sum_pred_creature_age.add(f64::from(phenotype.age));
                    stats
                        .sum_pred_generation
                        .add(f64::from(phenotype.generation));
                    stats.sum_attacks.add(f64::from(phenotype.last_attacks));
                    stats.sum_kills.add(f64::from(phenotype.last_kills));
                }
                FtClass::TopPred => {
                    stats.sum_tpred_fitness.add(cur_fitness);
                    stats
                        .sum_tpred_gestation
                        .add(f64::from(phenotype.gestation_time));
                    stats.sum_tpred_merit.add(cur_merit);
                    stats.sum_tpred_creature_age.add(f64::from(phenotype.age));
                    stats
                        .sum_tpred_generation
                        .add(f64::from(phenotype.generation));
                    stats.sum_attacks.add(f64::from(phenotype.last_attacks));
                    stats.sum_kills.add(f64::from(phenotype.last_kills));
                }
            }
            let exe = match class {
                FtClass::Prey => stats.inst_prey_exe_counts_for_inst_set(&inst_set),
                FtClass::Pred => stats.inst_pred_exe_counts_for_inst_set(&inst_set),
                FtClass::TopPred => stats.inst_top_pred_exe_counts_for_inst_set(&inst_set),
            };
            add_counts(exe, &phenotype.last_inst_count);
            let from_sensor = match class {
                FtClass::Prey => stats.inst_prey_from_sensor_exe_counts_for_inst_set(&inst_set),
                FtClass::Pred => stats.inst_pred_from_sensor_exe_counts_for_inst_set(&inst_set),
                FtClass::TopPred => {
                    stats.inst_top_pred_from_sensor_exe_counts_for_inst_set(&inst_set)
                }
            };
            add_counts(from_sensor, &phenotype.last_from_sensor_count);
            if class != FtClass::Prey {
                let att_inst: Vec<String> = stats.group_attack_insts(&inst_set).to_vec();
                let per_attack = if class == FtClass::Pred {
                    &phenotype.last_group_attack_count
                } else {
                    &phenotype.last_top_pred_group_attack_count
                };
                for (k, inst) in att_inst.iter().enumerate() {
                    let counts = stats.exec_counts_for_group_attack_inst(&inst_set, inst);
                    if let Some(row) = per_attack.get(k) {
                        add_counts(counts, row);
                    }
                }
            }
        }
    }
}

/// `for (j < src.GetSize()) dst[j].Add(src[j])`. The C++ sizes `dst` when
/// the instruction set is loaded; the port sizes it on first use, to the
/// same length.
fn add_counts(dst: &mut Vec<Accumulator<i32>>, src: &[i32]) {
    if dst.len() < src.len() {
        dst.resize_with(src.len(), Default::default);
    }
    for (j, &v) in src.iter().enumerate() {
        dst[j].add(v);
    }
}
