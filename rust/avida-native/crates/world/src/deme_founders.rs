//! Founder-based deme seeding and the genotype germlines of `cPopulation`
//! and `cDeme` (avida-core/source/main/cPopulation.cc and cDeme.cc at
//! 47f13dadb): `SeedDeme(source_deme, target_deme)` with
//! `DEMES_SEED_METHOD 1`, `SeedDeme_InjectDemeFounder`,
//! `ReplaceDemeFlaggedGermline` (`DEMES_USE_GERMLINE 3`), the founder and
//! germline half of `Inject`, and `cDeme::AddFounder`, `ClearFounders` and
//! `ReplaceGermline(GroupPtr)`.
//!
//! Founder seeding marks every organism of both demes running
//! (`SetRunning(true)`), so `KillOrganism` takes them out of their cells
//! without deleting them; their units stay in their genotypes until the
//! new founders are injected, and only then are they deleted. The port
//! keeps those organisms in [`SeedLimbo`].
//!
//! Stops with `unsupported:` (each is undefined behaviour or a crash in
//! the reference binary): seeding a deme from itself (`CompeteDemes`
//! re-seeding a deme that won once), where the C++ uses the old organisms
//! after deleting them; deme replication with `DEMES_USE_GERMLINE 2`, see
//! [`crate::pop_demes`].

use avida_genome::{Genome, InstructionSequence};
use avida_hardware::organism::OrgUid;
use avida_hardware::Organism;
use avida_systematics::unit::{GroupMembership, GroupRef};

use crate::driver::verbosity;
use crate::stat_tools::DoubleSum;
use crate::world::World;

/// `Avida::INVALID_CONFIG`, the status `Driver().Abort` exits with.
const INVALID_CONFIG: i32 = 2;

/// Organisms founder seeding marked running, and those `KillOrganism`
/// has since taken out of their cells but not deleted.
#[derive(Debug, Default)]
pub struct SeedLimbo {
    /// The organisms marked running (`SetRunning(true)`).
    marked: Vec<OrgUid>,
    /// Killed marked organisms with their systematics groups, in kill
    /// order.
    killed: Vec<(Box<Organism>, GroupMembership)>,
    /// Killed organisms the C++ never deletes (the source founders
    /// `DEMES_DIVIDE_METHOD 4` kills): their units never leave their
    /// genotypes.
    leaked: Vec<(Box<Organism>, GroupMembership)>,
    /// Set while `ProcessStepSpeculative` checks for implicit deme
    /// replication, before it looks at the organism it just ran.
    pub(crate) speculative_step: bool,
}

impl SeedLimbo {
    /// The end of `KillOrganism` for a cell's occupant: `None` when the
    /// organism is marked running and was kept here (`SetToDelete`),
    /// otherwise the organism and its groups back for `delete`.
    pub(crate) fn keep(
        &mut self,
        org: Box<Organism>,
        membership: GroupMembership,
    ) -> Option<(Box<Organism>, GroupMembership)> {
        if self.marked.contains(&org.state.uid) {
            self.killed.push((org, membership));
            None
        } else {
            Some((org, membership))
        }
    }

    fn take_killed(&mut self, uid: OrgUid) -> Option<(Box<Organism>, GroupMembership)> {
        let pos = self.killed.iter().position(|(o, _)| o.state.uid == uid)?;
        Some(self.killed.remove(pos))
    }
}

/// A founder chosen from the source deme: what `SeedDeme` later reads
/// through its `cOrganism*` (`KillOrganism` changes none of it).
#[derive(Debug, Clone, Copy)]
struct Founder {
    uid: OrgUid,
    cell: i32,
    /// `SystematicsGroup("genotype")`
    genotype: GroupRef,
    /// `GetPhenotype().GetGeneration()`
    generation: i32,
    /// `GetPhenotype().GetPermanentGermlinePropensity()`
    propensity: f64,
    /// `GetLineageLabel()`
    lineage: i32,
}

/// `source_founders` and `target_founders`.
type FounderLists = (Vec<Founder>, Vec<Founder>);

impl World {
    fn founder_at(&self, cell: i32) -> Result<Founder, String> {
        let c = cell as usize;
        let org = self.pop.orgs[c]
            .as_ref()
            .ok_or("deme founder cell is empty")?;
        let genotype = self
            .systematics
            .genotype_of(&self.pop.membership[c])
            .ok_or("deme founder has no genotype")?;
        Ok(Founder {
            uid: org.state.uid,
            cell,
            genotype,
            generation: org.state.phenotype.generation,
            propensity: org.state.phenotype.permanent_germline_propensity,
            lineage: org.state.lineage_label,
        })
    }

    /// The occupied cells of deme `d`, in cell order.
    fn deme_occupied_cells(&self, d: usize) -> Vec<i32> {
        self.pop.demes[d]
            .cell_ids
            .iter()
            .copied()
            .filter(|&c| self.pop.orgs[c as usize].is_some())
            .collect()
    }

    /// `cell_array[source_deme.GetCellID(random.GetUInt(size))]` until the
    /// cell is occupied.
    fn random_occupied_cell(&mut self, d: usize) -> Result<i32, String> {
        if self.deme_occupied_cells(d).is_empty() {
            return Err("picking an organism from an empty deme (C++: loops forever)".into());
        }
        let size = self.pop.demes[d].size() as u32;
        loop {
            let cellid = self.pop.demes[d].cell_id(self.rng.get_uint(size) as i32);
            if self.pop.orgs[cellid as usize].is_some() {
                return Ok(cellid);
            }
        }
    }

    // ------------------------------------------------------------------
    // cDeme founder and germline lists
    // ------------------------------------------------------------------

    /// `cDeme::AddFounder(bg, phenotype)`: of the copied phenotype only the
    /// generation and the permanent germline propensity are ever read.
    pub(crate) fn deme_add_founder(
        &mut self,
        d: usize,
        genotype: GroupRef,
        generation: i32,
        propensity: f64,
    ) {
        let id = self.systematics.group_id(genotype).unwrap_or(-1);
        let deme = &mut self.pop.demes[d];
        deme.founder_genotype_ids.push(id);
        deme.founder_generations.push(generation);
        deme.founder_germline_propensities.push(propensity);
        self.systematics.add_passive_reference(genotype);
    }

    /// `cDeme::ClearFounders()`
    pub(crate) fn deme_clear_founders(&mut self, d: usize) -> Result<(), String> {
        let ids = std::mem::take(&mut self.pop.demes[d].founder_genotype_ids);
        self.pop.demes[d].founder_generations.clear();
        self.pop.demes[d].founder_germline_propensities.clear();
        let role = self.systematics.arbiter_for_role("genotype");
        // check for unused genotypes, now that we're done with these
        for id in ids {
            let g = role
                .and_then(|r| self.systematics.group(r, id))
                .ok_or("deme founder genotype not found (C++: assertion, then NULL dereference)")?;
            self.systematics.remove_passive_reference(g);
        }
        Ok(())
    }

    /// `cDeme::ReplaceGermline(GroupPtr bg)`
    fn deme_replace_germline_genotype(&mut self, d: usize, genotype: GroupRef) {
        let id = self.systematics.group_id(genotype).unwrap_or(-1);
        // same genotype, no changes
        if self.pop.demes[d].germline_genotype_id == id {
            return;
        }
        // first, save and put a hold on new germline genotype
        let prev = std::mem::replace(&mut self.pop.demes[d].germline_genotype_id, id);
        self.systematics.add_passive_reference(genotype);
        // next, if we previously were saving a germline genotype, free it
        let role = self.systematics.arbiter_for_role("genotype");
        if let Some(pg) = role.and_then(|r| self.systematics.group(r, prev)) {
            self.systematics.remove_passive_reference(pg);
        }
    }

    /// The germline half of `Inject(...)` (it runs for every injection,
    /// even with a single deme).
    pub(crate) fn inject_germline(&mut self, cell_id: i32) {
        let germline = self.cfg.demes_use_germline;
        if germline == 0 {
            return;
        }
        let c = cell_id as usize;
        let d = self.deme_of(cell_id);
        match self.cfg.demes_seed_method {
            0 => {
                if germline == 1 && self.pop.demes[d].germline.is_empty() {
                    if let Some(genome) =
                        self.pop.orgs[c].as_ref().map(|o| o.state.genome().clone())
                    {
                        self.pop.demes[d].germline.push(genome);
                    }
                }
            }
            1 => {
                let Some((generation, propensity)) = self.pop.orgs[c].as_ref().map(|o| {
                    let ph = &o.state.phenotype;
                    (ph.generation, ph.permanent_germline_propensity)
                }) else {
                    return;
                };
                let Some(genotype) = self.systematics.genotype_of(&self.pop.membership[c]) else {
                    self.set_fatal("injected organism has no genotype");
                    return;
                };
                if germline == 2 {
                    //find the genotype we just created from the genome, and save it
                    self.deme_replace_germline_genotype(d, genotype);
                } else {
                    // not germlines, save org as founder (with the phenotype
                    // as it is before the propensity below is set)
                    self.deme_add_founder(d, genotype, generation, propensity);
                }
                // QUIRK: set unconditionally, so the default of -1 replaces
                // DEMES_DEFAULT_GERMLINE_PROPENSITY; the `>= 0` branch sets
                // the same value again.
                let fgp = self.cfg.demes_founder_germline_propensity;
                if let Some(org) = self.pop.orgs[c].as_mut() {
                    org.state.phenotype.permanent_germline_propensity = fgp;
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Germline mutation and founder injection
    // ------------------------------------------------------------------

    /// The `GERMLINE_COPY_MUT`, `GERMLINE_INS_MUT` and `GERMLINE_DEL_MUT`
    /// mutations `ReplaceDeme`, `SeedDeme_InjectDemeFounder` and
    /// `ReplaceDemeFlaggedGermline` apply to a germ genome. With `memory`
    /// (`ReplaceDemeFlaggedGermline`), a site the copy pass does not
    /// mutate takes the founder's in-memory instruction where it has one.
    pub(crate) fn mutate_germline(
        &mut self,
        genome: &mut Genome,
        memory: Option<&InstructionSequence>,
    ) -> Result<(), String> {
        let World { hw, rng, cfg, .. } = self;
        let instset = hw
            .registry()
            .get_inst_set(genome.inst_set())
            .ok_or("germline instruction set not found")?;
        let seq = genome.representation_mut();
        if cfg.germline_copy_mut > 0.0 {
            for i in 0..seq.size() {
                if rng.p(cfg.germline_copy_mut) {
                    seq.set(i, instset.random_inst(rng));
                } else if let Some(m) = memory.filter(|m| m.size() > i) {
                    // this line copies the mutations accured as a result of
                    // performing tasks to the new genome
                    seq.set(i, m.get(i));
                }
            }
        }
        if cfg.germline_ins_mut > 0.0 && rng.p(cfg.germline_ins_mut) {
            let mut_line = rng.get_uint((seq.size() + 1) as u32) as i32;
            seq.insert(mut_line, instset.random_inst(rng));
        }
        if cfg.germline_del_mut > 0.0 && rng.p(cfg.germline_del_mut) {
            let mut_line = rng.get_uint(seq.size() as u32) as i32;
            seq.remove(mut_line, 1);
        }
        Ok(())
    }

    /// `SeedDeme_InjectDemeFounder(cell_id, bg, ctx, phenotype,
    /// lineage_label, reset)`; `phenotype` is the founder's generation and
    /// permanent germline propensity.
    fn seed_deme_inject_founder(
        &mut self,
        cell_id: i32,
        genotype: GroupRef,
        phenotype: Option<(i32, f64)>,
        lineage_label: i32,
        reset: bool,
    ) -> Result<(), String> {
        let mut genome =
            Genome::from_string(&self.systematics.property(genotype, "genome").string_value());
        // Mutate the genome?
        if self.cfg.demes_mut_orgs_on_replication == 1 && !reset {
            self.mutate_germline(&mut genome, None)?;
        }
        self.inject_genome(
            cell_id,
            crate::pop_demes::duplication(""),
            &genome,
            lineage_label,
            true,
        )?;
        // At this point, the cell had better be occupied...
        let fgp = self.cfg.demes_founder_germline_propensity;
        let org = self.pop.orgs[cell_id as usize]
            .as_mut()
            .ok_or("injected deme founder is not in its cell")?;
        if let Some((generation, propensity)) = phenotype {
            // For now, just copy the generation...
            org.state.phenotype.generation = generation;
            // and germline propensity.
            org.state.phenotype.permanent_germline_propensity = propensity;
            if fgp >= 0.0 {
                org.state.phenotype.permanent_germline_propensity = fgp;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // SeedDeme(source_deme, target_deme) with DEMES_SEED_METHOD 1
    // ------------------------------------------------------------------

    /// The founder lists (`source_founders`, `target_founders`) of
    /// `DEMES_ORGANISM_SELECTION`; `None` when the run was aborted.
    fn select_deme_founders(&mut self, source: usize) -> Result<Option<FounderLists>, String> {
        let replicate_size = self.cfg.demes_replicate_size;
        let selection = self.cfg.demes_organism_selection;
        let mut source_founders = Vec::new();
        let mut target_founders = Vec::new();
        match selection {
            // Random w/ replacement (meaning, we don't prevent the same
            // genotype from being selected more than once).
            0 => {
                let mut founders = Vec::new();
                while (founders.len() as i64) < i64::from(replicate_size) {
                    let cellid = self.random_occupied_cell(source)?;
                    founders.push(self.founder_at(cellid)?);
                }
                source_founders.clone_from(&founders);
                target_founders = founders;
            }
            // Sequential selection, from the beginning.
            1 => {
                if replicate_size > self.pop.demes[source].size() {
                    return Err(
                        "DEMES_REPLICATE_SIZE exceeds the deme size (C++: out-of-bounds cell)"
                            .into(),
                    );
                }
                let mut founders = Vec::new();
                for i in 0..replicate_size {
                    let cellid = self.pop.demes[source].cell_id(i);
                    if self.pop.orgs[cellid as usize].is_some() {
                        founders.push(self.founder_at(cellid)?);
                    }
                }
                source_founders.clone_from(&founders);
                target_founders = founders;
            }
            // Random w/o replacement. Take as many for each deme as
            // DEME_REPLICATE_SIZE.
            6 => {
                //Die if not >= two organisms.
                if self.pop.demes[source].cur_org_count >= 2 {
                    // `tList::Push` adds at the front, so the list runs
                    // from the last occupied cell to the first.
                    let mut prospective = Vec::new();
                    for cell in self.deme_occupied_cells(source).into_iter().rev() {
                        prospective.push(self.founder_at(cell)?);
                    }
                    //add orgs alternately to source and target founders
                    //until we run out or each reaches DEMES_REPLICATE_SIZE.
                    let rs = i64::from(replicate_size);
                    let mut num_chosen_orgs = 0;
                    while ((target_founders.len() as i64) < rs
                        || (source_founders.len() as i64) < rs)
                        && !prospective.is_empty()
                    {
                        let chosen = self.rng.get_uint(prospective.len() as u32) as usize;
                        let f = prospective.remove(chosen);
                        if num_chosen_orgs % 2 == 0 {
                            source_founders.push(f);
                        } else {
                            target_founders.push(f);
                        }
                        num_chosen_orgs += 1;
                    }
                }
            }
            // Grab the organisms that have flagged themselves as part of
            // the germline. Ignores replicate size...
            7 => {
                let founders = self.flagged_germline_founders(source)?;
                source_founders.clone_from(&founders);
                target_founders = founders;
            }
            // Grab a random org from the set of orgs that have flagged
            // themselves as part of the germline.
            8 => {
                let potential = self.flagged_germline_founders(source)?;
                let mut founders = Vec::new();
                if !potential.is_empty() {
                    let r = self.rng.get_uint(potential.len() as u32) as usize;
                    founders.push(potential[r]);
                }
                source_founders.clone_from(&founders);
                target_founders = founders;
            }
            // Selection based on germline propensities.
            2..=5 => {
                if self.pop.demes[source].cur_org_count >= 2 {
                    if self.cfg.demes_divide_method != 0 {
                        self.abort_run(
                            "error: Germline DEMES_ORGANISM_SELECTION methods 2 and 3 can only \
                             be used with DEMES_DIVIDE_METHOD 0.\n",
                            INVALID_CONFIG,
                            "Germline DEMES_ORGANISM_SELECTION methods 2 and 3 can only be used \
                             with DEMES_DIVIDE_METHOD 0.",
                        );
                        return Ok(None);
                    }
                    let founders = self.propensity_founders(source, selection)?;
                    if let Some(&f) = founders.first() {
                        source_founders.push(f);
                    }
                    if let Some(&f) = founders.get(1) {
                        target_founders.push(f);
                    }
                }
            }
            v => {
                self.abort_run(
                    &format!("Undefined value ({v}) for DEMES_ORGANISM_SELECTION.\n"),
                    1,
                    "Undefined value for DEMES_ORGANISM_SELECTION",
                );
                return Ok(None);
            }
        }
        Ok(Some((source_founders, target_founders)))
    }

    /// The organisms of deme `d` that flagged themselves as part of the
    /// germline (`IsGermline()`), in cell order.
    fn flagged_germline_founders(&self, d: usize) -> Result<Vec<Founder>, String> {
        let mut founders = Vec::new();
        for cell in self.deme_occupied_cells(d) {
            let is_germ = self.pop.orgs[cell as usize]
                .as_ref()
                .is_some_and(|o| o.state.phenotype.is_germ_cell);
            if is_germ {
                founders.push(self.founder_at(cell)?);
            }
        }
        Ok(founders)
    }

    /// `DEMES_ORGANISM_SELECTION` 2 to 5: up to two founders picked by
    /// their permanent germline propensities. 2: weighted by propensity;
    /// 3: each organism with a nonzero propensity equally likely; 4: as 3,
    /// but no random organisms fill up; 5: as 3, but nothing is picked when
    /// fewer than two organisms have a nonzero propensity.
    fn propensity_founders(
        &mut self,
        source: usize,
        selection: i32,
    ) -> Result<Vec<Founder>, String> {
        let mut prospective: Vec<Founder> = Vec::new();
        let mut gp_sum = DoubleSum::new();
        for cell in self.deme_occupied_cells(source) {
            let f = self.founder_at(cell)?;
            if f.propensity > 0.0 {
                gp_sum.add(f.propensity);
                prospective.push(f);
            }
        }
        if self.cfg.verbosity >= verbosity::ON {
            let text = format!(
                "Germline Propensity Sum: {}\nNum prospective founders: {}\n",
                self.cout_double(gp_sum.sum()),
                prospective.len()
            );
            self.action_state.stdout.push_str(&text);
        }

        let mut founders: Vec<Founder> = Vec::new();
        if prospective.len() < 2 {
            // there were not enough orgs with nonzero germlines pick
            // additional orgs at random without replacement, unless our
            // method forbids this

            // leave the founder list empty for method 5
            if selection != 5 {
                founders.clone_from(&prospective);
                //do not add additional founders randomly for method 4
                if selection != 4 {
                    if self.deme_occupied_cells(source).len() < 2 {
                        return Err("fewer than two organisms to pick germline founders from \
                                    (C++: loops forever)"
                            .into());
                    }
                    let size = self.pop.demes[source].size() as u32;
                    while founders.len() < 2 {
                        let cellid = self.pop.demes[source].cell_id(self.rng.get_uint(size) as i32);
                        if self.pop.orgs[cellid as usize].is_some() {
                            let f = self.founder_at(cellid)?;
                            if !founders.iter().any(|x| x.uid == f.uid) {
                                founders.push(f);
                            }
                        }
                    }
                }
            }
        } else {
            // pick two orgs based on germline propensities from
            // prospective founders
            while founders.len() < 2 {
                let mut choice = if selection == 2 {
                    self.rng.get_double_max(gp_sum.sum())
                } else {
                    self.rng.get_double_max(gp_sum.count())
                };
                // find the next organism to choose
                let mut org: Option<Founder> = None;
                let mut on = 0;
                while choice > 0.0 && on < prospective.len() {
                    let o = prospective[on];
                    org = Some(o);
                    // did we already have this org?
                    let found = founders.iter().any(|x| x.uid == o.uid);
                    // if it wasn't already chosen, then we count down...
                    if !found {
                        choice -= if selection == 2 {
                            o.propensity
                        } else {
                            f64::from(u8::from(o.propensity > 0.0))
                        };
                    }
                    on += 1;
                }
                let o = org.ok_or(
                    "germline founder choice of 0 (C++: NULL dereference in cPopulation::SeedDeme)",
                )?;
                gp_sum.subtract(o.propensity, 1.0);
                founders.push(o);
            }
        }
        Ok(founders)
    }

    /// The `DEMES_SEED_METHOD 1` branch of `SeedDeme(source_deme,
    /// target_deme)` (@JEB: an updated seed deme method that maintains
    /// genotype inheritance). Returns whether the target was seeded.
    pub(crate) fn seed_deme_founders(
        &mut self,
        source: usize,
        target: usize,
    ) -> Result<bool, String> {
        if self.pop.running.is_some() {
            return Err("unsupported: founder-based deme seeding while an organism runs".into());
        }
        let Some((source_founders, target_founders)) = self.select_deme_founders(source)? else {
            return Ok(true);
        };

        // Methods that require a germline can sometimes come up short...
        if source_founders.is_empty() {
            return Ok(false);
        }
        if self.pop.seed_limbo.speculative_step && matches!(self.cfg.demes_divide_method, 0 | 1 | 4)
        {
            // `ProcessStepSpeculative` reads `cur_org->GetPhenotype().
            // GetToDelete()` after `CheckImplicitDemeRepro`. Methods 0 and 1
            // delete the source deme's organisms, the one just run among
            // them once `KillOrganism` has set its to-delete flag, so the
            // C++ reads freed memory and deletes it again (the reference
            // binary crashes there). With method 4 a founder that was the
            // organism just run is deleted after the step instead of
            // leaked, and the port does not know which organism that was.
            return Err(
                "unsupported: founder-based deme seeding from implicit replication in \
                        a speculative step with DEMES_DIVIDE_METHOD 0, 1 or 4 (C++: \
                        cPopulation::ProcessStepSpeculative uses the organism SeedDeme deleted)"
                    .into(),
            );
        }
        if source == target && !target_founders.is_empty() {
            // The old organisms are on both lists: the C++ deletes them as
            // old target organisms, then calls SetRunning (and with
            // DEMES_DIVIDE_METHOD 0 or 1 `delete`) on them again as old
            // source organisms. The reference binary crashes in SeedDeme
            // (EXC_BAD_ACCESS) with method 0; with method 4 it goes on with
            // `Die` through freed organisms.
            return Err(
                "unsupported: founder-based SeedDeme of a deme from itself (C++: \
                        cPopulation::SeedDeme uses the old organisms after deleting them)"
                    .into(),
            );
        }

        // We clear the deme, but trick cPopulation::KillOrganism to NOT
        // delete the organisms, by pretending the orgs are running. This
        // way we can still create clones of them that will track stats
        // correctly. We also need to defer adjusting the genotype or it
        // will be prematurely deleted before we are done!
        let uids = |w: &World, d: usize| -> Vec<OrgUid> {
            w.deme_occupied_cells(d)
                .into_iter()
                .filter_map(|c| w.pop.orgs[c as usize].as_ref().map(|o| o.state.uid))
                .collect()
        };
        let old_source = uids(self, source);
        let old_target = uids(self, target);
        self.pop.seed_limbo.marked.extend_from_slice(&old_source);
        self.pop.seed_limbo.marked.extend_from_slice(&old_target);

        let result =
            self.seed_deme_founders_marked(source, target, &source_founders, &target_founders);
        let divide = self.cfg.demes_divide_method;
        let successfully_seeded = !target_founders.is_empty();

        // remember to delete the old target organisms and adjust their
        // genotypes, ONLY if seeding was successful (otherwise they still
        // exist in the population!!!), and the old source organisms ONLY
        // if the source was replaced.
        let limbo = &mut self.pop.seed_limbo;
        limbo.marked.clear();
        let mut doomed = Vec::new();
        if successfully_seeded {
            doomed.extend(old_target.iter().filter_map(|&u| limbo.take_killed(u)));
        }
        if divide == 0 || divide == 1 {
            doomed.extend(old_source.iter().filter_map(|&u| limbo.take_killed(u)));
        }
        let leaked = std::mem::take(&mut limbo.killed);
        limbo.leaked.extend(leaked);
        for (org, m) in doomed {
            self.delete_organism(org, Some(m));
        }
        result.map(|()| successfully_seeded)
    }

    /// `SeedDeme` from clearing the target to the end of the source's
    /// `DEMES_DIVIDE_METHOD` handling, with the old organisms marked.
    fn seed_deme_founders_marked(
        &mut self,
        source: usize,
        target: usize,
        source_founders: &[Founder],
        target_founders: &[Founder],
    ) -> Result<(), String> {
        // Clear the target deme (if we have successfully replaced it).
        if !target_founders.is_empty() {
            self.deme_clear_founders(target)?;
            self.deme_update_stats(target);
            self.deme_kill_all(target);
        }

        // Now populate the target (and optionally the source) using
        // InjectGenotype.
        for (i, f) in target_founders.iter().enumerate() {
            let cellid = self.deme_select_injection_cell(target, i as i32)?;
            self.seed_deme_inject_founder(
                cellid,
                f.genotype,
                Some((f.generation, f.propensity)),
                f.lineage,
                false,
            )?;
            self.deme_post_injection(target, cellid);
        }
        for cell in self.deme_occupied_cells(target) {
            let f = self.founder_at(cell)?;
            self.deme_add_founder(target, f.genotype, f.generation, f.propensity);
        }

        // We either repeat this procedure in the source deme, restart the
        // source deme from its old founders, or do nothing to the source
        // deme...
        match self.cfg.demes_divide_method {
            // source deme is replaced in the same way as the target
            0 => {
                self.deme_clear_founders(source)?;
                self.deme_update_stats(source);
                self.deme_kill_all(source);
                for (i, f) in source_founders.iter().enumerate() {
                    let cellid = self.deme_select_injection_cell(source, i as i32)?;
                    let phenotype = Some((f.generation, f.propensity));
                    self.seed_deme_inject_founder(cellid, f.genotype, phenotype, f.lineage, false)?;
                    self.deme_add_founder(source, f.genotype, f.generation, f.propensity);
                    self.deme_post_injection(source, cellid);
                }
            }
            // source deme is "reset" by re-injecting founder genotypes
            1 => self.reseed_deme_from_founders(source)?,
            // source deme is left untouched
            2 => {}
            3 => self.pop.demes[source].total_res_consumed = 0.0,
            4 => {
                self.pop.demes[source].total_res_consumed = 0.0;
                for f in source_founders {
                    // `Die(ctx)`: `KillOrganism` on the founder's cell,
                    // which is already empty for a founder listed twice.
                    let occupant = self.pop.orgs[f.cell as usize].as_ref().map(|o| o.state.uid);
                    match occupant {
                        Some(u) if u == f.uid => self.kill_organism(f.cell, None),
                        None => {}
                        Some(_) => return Err("deme founder's cell has a new occupant".into()),
                    }
                }
            }
            _ => {
                self.abort_run(
                    "error: Unknown DEMES_DIVIDE_METHOD\n",
                    INVALID_CONFIG,
                    "Unknown DEMES_DIVIDE_METHOD",
                );
            }
        }
        Ok(())
    }

    /// `DEMES_DIVIDE_METHOD 1`: the source deme is "reset" by re-injecting
    /// its founder genotypes. Do not update the last founder generation,
    /// since the founders have not changed.
    fn reseed_deme_from_founders(&mut self, source: usize) -> Result<(), String> {
        self.deme_update_stats(source);
        self.deme_kill_all(source);
        // do not clear or change founder list; use it to recreate
        // ancestral state of genotypes
        let deme = &self.pop.demes[source];
        let ids = deme.founder_genotype_ids.clone();
        let generations = deme.founder_generations.clone();
        let propensities = deme.founder_germline_propensities.clone();
        let role = self.systematics.arbiter_for_role("genotype");
        for (i, &id) in ids.iter().enumerate() {
            let cellid = self.deme_select_injection_cell(source, i as i32)?;
            let g = role
                .and_then(|r| self.systematics.group(r, id))
                .ok_or("deme founder genotype not found (C++: NULL dereference)")?;
            let phenotype = Some((generations[i], propensities[i]));
            self.seed_deme_inject_founder(cellid, g, phenotype, -1, true)?;
            self.deme_post_injection(source, cellid);
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // ReplaceDemeFlaggedGermline (DEMES_USE_GERMLINE 3)
    // ------------------------------------------------------------------

    /// `ReplaceDemeFlaggedGermline(source_deme, target_deme, ctx)`. The
    /// first founder it injects ends the run: the C++ follows it with
    /// `Feedback().Error("Temporarily disabled ...")` and
    /// `Abort(INVALID_CONFIG)`.
    pub(crate) fn replace_deme_flagged_germline(
        &mut self,
        source: usize,
        target: usize,
    ) -> Result<(), String> {
        // Get list of potential founders
        let potential = self.flagged_germline_founders(source)?;
        let target_founders = if self.cfg.demes_organism_selection == 8 {
            // pick a random founder...
            if potential.is_empty() {
                return Ok(());
            }
            let r = self.rng.get_uint(potential.len() as u32) as usize;
            vec![potential[r]]
        } else {
            potential
        };
        // Stats tracking; pre-replication hook.
        self.deme_pre_replication(source);

        let (source_reset, target_reset) = match self.cfg.demes_reset_resources {
            0 => (true, true),
            1 => (false, true),
            2 => (false, false),
            v => {
                self.abort_run(
                    &format!("Undefined value {v} for DEMES_RESET_RESOURCES\n"),
                    1,
                    "Undefined value for DEMES_RESET_RESOURCES",
                );
                return Ok(());
            }
        };
        self.deme_divide_reset(target, source, target_reset, 0.0);
        self.deme_divide_reset(source, source, source_reset, 0.0);

        self.deme_clear_founders(target)?;
        self.deme_update_stats(target);
        self.deme_kill_all(target);

        if let Some(f) = target_founders.first() {
            // this is the genome we need to use. However, we only need part
            // of it...since it can include an offspring
            let memory = self.pop.orgs[f.cell as usize]
                .as_ref()
                .map(|o| o.hardware.memory().to_sequence())
                .ok_or("germline founder is not in its cell")?;
            // this is the genotype of the organism, which does not reflect
            // any point mutations that have occurred. we need to use it to
            // get the right length for the genome
            let mut genome = Genome::from_string(
                &self
                    .systematics
                    .property(f.genotype, "genome")
                    .string_value(),
            );
            self.mutate_germline(&mut genome, Some(&memory))?;
            let cellid = self.deme_select_injection_cell(target, 0)?;
            self.inject_genome(
                cellid,
                crate::pop_demes::duplication("germline"),
                &genome,
                f.lineage,
                true,
            )?;
            let c = cellid as usize;
            let propensity = {
                let org = self.pop.orgs[c]
                    .as_mut()
                    .ok_or("injected germline founder is not in its cell")?;
                // For now, just copy the generation...
                org.state.phenotype.generation = f.generation;
                org.state.phenotype.permanent_germline_propensity
            };
            let genotype = self
                .systematics
                .genotype_of(&self.pop.membership[c])
                .ok_or("injected organism has no genotype")?;
            self.deme_add_founder(target, genotype, f.generation, propensity);
            let msg = "Temporarily disabled due to bizarre incompatibility on Windows with Visual \
                       Studio 2012";
            self.abort_run(&format!("error: {msg}\n"), INVALID_CONFIG, msg);
            return Ok(());
        }

        // For source deme...
        if self.cfg.demes_divide_method == 1 {
            self.reseed_deme_from_founders(source)?;
        }
        self.pop.demes[source].total_res_consumed = 0.0;
        self.pop.demes[source].clear_shannon_information_stats();
        self.pop.demes[target].clear_shannon_information_stats();

        // do our post-replication stats tracking.
        let genotype_ids = self.deme_genotype_ids(target);
        self.stats
            .deme_post_replication(target as i32, genotype_ids);
        self.stats
            .track_deme_gls_replication(source as i32, target as i32, Vec::new());
        Ok(())
    }
}
