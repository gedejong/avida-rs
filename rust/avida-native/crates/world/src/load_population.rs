//! `cPopulation::LoadPopulation` (avida-core/source/main/cPopulation.cc at
//! 47f13dadb): replace the population by the organisms of a `.spop` file.
//!
//! Reading the file and loading its genotypes into the genotype arbiter are
//! `avida_systematics::spop::{read_population, load_population_genotypes}`;
//! this module creates and activates the organisms.
//!
//! Parasite lines inject their parasite into the host loaded before them.
//! `load_groups` gives each organism the group and forager type saved with
//! it (`group_id`, `forager_type`) before it is activated. `load_traceq`
//! queues the loaded organisms for mini traces (`crate::mini_trace`).

use avida_config::init_file::InitFile;
use avida_config::string_util::as_double;
use avida_genome::Genome;
use avida_hardware::org_set_forage_target;
use avida_hardware::organism::{OrgSource, Organism};
use avida_phenotype::Merit;
use avida_systematics::spop::{
    genotype_hints, load_population_genotypes, read_population, LoadOptions,
};
use avida_systematics::unit::{GroupMembership, GroupRef, UnitInfo};

use crate::groups::pending_key;
use crate::pop_interface::PopulationInterface;
use crate::population::position_offspring as po;
use crate::world::World;

/// The arguments of `cActionLoadPopulation` that reach `LoadPopulation`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoadPopulationArgs {
    pub cellid_offset: i32,
    pub lineage_offset: i32,
    pub load_groups: bool,
    pub load_birth_cells: bool,
    pub load_avatars: bool,
    pub load_rebirth: bool,
    pub load_parent_dat: bool,
    pub load_traceq: i32,
}

impl World {
    /// `cPopulation::LoadPopulation(filename, ctx, cellid_offset,
    /// lineage_offset, load_groups, load_birth_cells, load_avatars,
    /// load_rebirth, load_parent_dat, traceq)`. `Ok(false)` is the C++
    /// `return false` (the file could not be opened).
    pub fn load_population(
        &mut self,
        filename: &str,
        args: &LoadPopulationArgs,
    ) -> Result<bool, String> {
        let input = InitFile::new(filename, &self.working_dir, None, None);
        for line in input.feedback().render().lines() {
            // `ctx.Driver().Feedback()` prints with `printf`.
            self.action_state.stdout.push_str(line);
            self.action_state.stdout.push('\n');
        }
        if !input.was_opened() {
            return Ok(false);
        }

        // Clear out the population, unless an offset is being used
        if args.cellid_offset == 0 {
            for i in 0..self.pop.cells.len() as i32 {
                self.kill_organism(i, None);
            }
        }

        // First, we read in all the genotypes and store them in an array
        let opts = LoadOptions {
            load_groups: args.load_groups,
            load_birth_cells: args.load_birth_cells,
            load_avatars: args.load_avatars,
            load_rebirth: args.load_rebirth,
            load_parent_dat: args.load_parent_dat,
            use_avatars: self.cfg.use_avatars != 0,
        };
        let mut loaded = read_population(&input, filename, &opts);
        let role = self
            .systematics
            .arbiter_for_role("genotype")
            .ok_or("LoadPopulation: no genotype arbiter")?;
        {
            let arbiter = self
                .systematics
                .genotypes_mut()
                .ok_or("LoadPopulation: no genotype arbiter")?;
            load_population_genotypes(arbiter, &mut loaded)?;
        }

        // Process genotypes, inject into organisms as necessary
        let placements = loaded.placements(args.cellid_offset, args.lineage_offset, &opts)?;
        for p in placements {
            let tmp = &loaded.genotypes[p.genotype];
            let cell_i = p.cell_i;
            let cell_id = p.cell_id;
            if cell_id < 0 || cell_id as usize >= self.pop.cells.len() {
                return Err(format!(
                    "LoadPopulation: cell {cell_id} is outside the world (C++: out-of-bounds access)"
                ));
            }
            let handle = tmp.group.ok_or("LoadPopulation: genotype was not loaded")?;
            let g = GroupRef { role, handle };
            let bg_id = self
                .systematics
                .genotypes()
                .and_then(|a| a.genotype(handle))
                .map_or(-1, |x| x.id());

            let genome =
                Genome::from_string(&self.systematics.property(g, "genome").string_value());
            let src = OrgSource::new(
                tmp.source.transmission_type as i32,
                &tmp.source.arguments,
                tmp.source.external,
            );
            let mut org = Organism::new(self, &genome, -1, src)?;

            // Setup the phenotype...
            let update = self.stats.update();
            org.state
                .phenotype
                .setup_inject(&self.cfg, genome.representation().size(), update);

            // Classify this new organism
            let mut membership = GroupMembership::new();
            {
                let unit = UnitInfo {
                    source: &tmp.source,
                    genome: org.state.genome(),
                    generation: org.state.phenotype.generation,
                };
                let hints = genotype_hints(bg_id);
                self.systematics
                    .classify_new_unit(&unit, Some(&hints), &mut membership);
            }

            // Coalescense Clade Setup
            org.state.cclade_id = -1;

            if self.cfg.energy_enabled {
                org.state.phenotype.merit = self.injected_energy_merit(&org.state.phenotype);
            } else {
                // Set the phenotype merit from the save file
                let mut merit = as_double(tmp.prop("merit"));
                if (args.load_rebirth || args.load_parent_dat)
                    && self.cfg.inherit_merit != 0
                    && tmp.props.contains_key("parent_merit")
                {
                    merit = *tmp
                        .parent_merit
                        .get(cell_i)
                        .ok_or("LoadPopulation: parent_merit list too short")?;
                }
                if merit > 0.0 {
                    org.state.phenotype.merit = Merit::new(merit);
                } else {
                    // `new_organism->GetTestMerit(ctx)`
                    let genotype = self
                        .systematics
                        .genotype_of(&membership)
                        .ok_or("loaded organism has no genotype")?;
                    let metrics = self
                        .genome_test_metrics(genotype)
                        .ok_or("loaded genotype has no genome")?;
                    if let Some(f) = self.fatal.borrow_mut().take() {
                        return Err(f);
                    }
                    org.state.phenotype.merit = Merit::new(metrics.merit);
                }

                if tmp.offsets.len() > cell_i && !args.load_rebirth {
                    // Adjust initial merit to account for organism execution at
                    // the time the population was saved
                    let gest_time = as_double(tmp.prop("gest_time"));
                    let gest_remain = gest_time - f64::from(tmp.offsets[cell_i]);
                    if gest_remain > 0.0 && gest_time > 0.0 {
                        let new_merit =
                            org.state.phenotype.merit.get_double() * (gest_time / gest_remain);
                        org.state.phenotype.merit = Merit::new(new_merit);
                    }
                }
            }

            org.state.lineage_label = p.lineage_label;

            // Prep the cell..
            if self.cfg.birth_method == po::FULL_SOUP_ELDEST && self.is_occupied(cell_id) {
                // Have to manually take this cell out of the reaper Queue.
                if let Some(pos) = self.pop.reaper_queue.iter().position(|&x| x == cell_id) {
                    self.pop.reaper_queue.remove(pos);
                }
            }

            // Setup the child's mutation rates. Since this organism is being
            // injected and has no parent, we should always take the rate
            // from the environment.
            org.state
                .mut_rates
                .copy_from(&self.pop.cells[cell_id as usize].mut_rates);

            // handle parasite
            if tmp.is_parasite() {
                if !self.is_occupied(cell_id) {
                    eprintln!("Loaded parasite before or without host!");
                    return Err(
                        "C++ abort: Loaded parasite before or without host! (cPopulation::LoadPopulation)"
                            .to_string(),
                    );
                }
                // QUIRK: the organism built for the parasite is never
                // deleted, so it stays in the genotype it was classified
                // into; the parasite is a new unit whose label is the
                // source arguments, which `LoadPopulation` set to the file
                // name.
                // Its groups are never told it left.
                drop(membership);
                drop(org);
                let label = tmp.source.arguments.clone();
                self.inject_parasite_into_cell(&label, genome.representation(), cell_id);
                continue;
            }

            // Activate the organism in the population...
            let parent_ft = tmp.parent_ft.get(cell_i).copied();
            let parent_teacher = tmp.parent_teacher.get(cell_i).copied();
            let parent_merit = tmp.parent_merit.get(cell_i).copied();
            let has_parent_merit = tmp.props.contains_key("parent_merit");
            let parent_list = |v: Option<i32>, name: &str| {
                v.ok_or_else(|| format!("LoadPopulation: {name} list too short"))
            };
            let org_survived = if !args.load_rebirth {
                let c = cell_id as usize;
                let org_survived = if args.load_groups {
                    // Set up group id and forager type (if loaded)
                    let group_id = loaded_list_entry(&tmp.group_ids, cell_i, "group_id")?;
                    let forager_type =
                        loaded_list_entry(&tmp.forager_types, cell_i, "forager_type")?;
                    let update = self.stats.update();
                    org.state.set_opinion(&self.cfg, update, group_id)?;
                    // `JoinGroup(new_organism, group_id)` before the
                    // organism has a cell: it is a member under a pending
                    // key until `ActivateOrganism` has emptied its cell.
                    let key = pending_key(0);
                    self.join_group(&mut [(key, &mut org.state)], key, group_id);
                    org.state.phenotype.birth_cell_id = cell_id;
                    org.state.phenotype.birth_group_id = group_id;
                    org.state.phenotype.birth_forager_type = forager_type;
                    org.state.parent_group = group_id;
                    org.state.parent_ft = forager_type;
                    if !tmp.parent_merit.is_empty() {
                        org.state.p_merit = parent_merit.ok_or(
                            "LoadPopulation: parent_merit list too short (C++: out-of-bounds access)",
                        )?;
                    }
                    self.pop.groups.activating = Some(key);
                    let org_survived =
                        self.activate_organism(org, membership, cell_id, false, true, None);
                    self.pop.groups.activating = None;
                    if org_survived {
                        self.loaded_org_set_forage_target(cell_id, forager_type)?;
                    }
                    org_survived
                } else {
                    self.activate_organism(org, membership, cell_id, true, true, None)
                };
                if args.load_parent_dat {
                    let ft = parent_list(parent_ft, "parent_ft")?;
                    let teacher =
                        parent_teacher.ok_or("LoadPopulation: parent_is_teach list too short")?;
                    if let Some(o) = self.pop.orgs[c].as_mut() {
                        o.state.parent_ft = ft;
                        o.state.parent_teacher = teacher;
                        if has_parent_merit {
                            o.state.p_merit = parent_merit
                                .ok_or("LoadPopulation: parent_merit list too short")?;
                        }
                    }
                }
                org_survived
            } else {
                org.state.parent_ft = parent_list(parent_ft, "parent_ft")?;
                org.state.parent_teacher =
                    parent_teacher.ok_or("LoadPopulation: parent_is_teach list too short")?;
                if let Some(m) = parent_merit {
                    org.state.p_merit = m;
                }
                org.state.phenotype.birth_cell_id = cell_id;
                self.activate_organism(org, membership, cell_id, false, true, None)
            };

            if org_survived && self.cfg.use_avatars != 0 && !self.cfg.neural_networking {
                let avatar_cell = if tmp.avatar_cells.is_empty() {
                    -1
                } else {
                    *tmp.avatar_cells.get(cell_i).ok_or(
                        "LoadPopulation: avatar_cell list too short (C++: out-of-bounds access)",
                    )?
                };
                if avatar_cell != -1 {
                    self.add_pred_prey_av_of_cell_at(cell_id, avatar_cell);
                    if let Some(o) = self.pop.orgs[cell_id as usize].as_mut() {
                        o.state.phenotype.av_birth_cell_id = avatar_cell;
                    }
                }
            }
            if org_survived {
                self.try_write_birth_loc_data_of_cell(cell_id);
            }
            if org_survived && args.load_traceq != 0 {
                self.mini_trace.print_genomes = args.load_traceq == 2;
                self.setup_mini_trace_in_cell(cell_id);
            }
        }
        self.pop.sync_events = true;
        Ok(true)
    }

    /// `new_organism->SetForageTarget(ctx, forager_type, true)` for the
    /// organism just activated in `cell_id`.
    fn loaded_org_set_forage_target(
        &mut self,
        cell_id: i32,
        forager_type: i32,
    ) -> Result<(), String> {
        let c = cell_id as usize;
        let Some(mut org) = self.pop.orgs[c].take() else {
            return Ok(());
        };
        let r = {
            let mut iface = PopulationInterface { w: self, cell_id };
            org_set_forage_target(&mut org.state, &mut iface, forager_type, true)
        };
        self.pop.orgs[c] = Some(org);
        r
    }
}

/// `tmp.<list>[cell_i]` of a list read with `load_groups`, or -1 when the
/// file has none. A list shorter than the organism count is indexed past
/// its end in the C++ (the size check is an `assert`).
fn loaded_list_entry(list: &[i32], cell_i: usize, name: &str) -> Result<i32, String> {
    if list.is_empty() {
        return Ok(-1);
    }
    list.get(cell_i)
        .copied()
        .ok_or_else(|| format!("LoadPopulation: {name} list too short (C++: out-of-bounds access)"))
}
