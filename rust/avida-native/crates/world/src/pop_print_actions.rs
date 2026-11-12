//! The PrintActions.cc actions that print per-deme organism statistics and
//! donation counts through `cPopulation` (avida-core/source at 47f13dadb):
//! `cActionPrintDemeAllStats` (registered as `PrintDemeAllStats` and
//! `PrintDemeStats`), `cActionPrintDemeFitness`, `cActionPrintDemeLifeFitness`,
//! `cActionPrintDemeDonorStats`, `cActionPrintDemeSpacialEnergy`,
//! `cActionPrintDemeSpacialSleep`, `cActionPrintDonationStats` and
//! `cActionPrintPhenotypeStatus`, with the cPopulation.cc printers they
//! call.
//!
//! `PrintDemeTasksData` names `cActionPrintDemeTasks`: PrintActions.cc also
//! declares a `STATS_OUT_FILE` action of that name, but registers it after
//! `cActionPrintDemeTasks`, and `cActionLibrary` refuses the second
//! registration.

use avida_config::string_util::pop_word;
use avida_hardware::organism::Organism;

use crate::stat_tools::DoubleSum;
use crate::world::World;

/// The registered action names this module parses.
pub const POP_PRINT_ACTIONS: &[&str] = &[
    "PrintDonationStats",
    "PrintDemeAllStats",
    "PrintDemeStats",
    "PrintDemeDonorStats",
    "PrintDemeSpacialEnergyStats",
    "PrintDemeSpacialSleepStats",
    "PrintDemeFitnessData",
    "PrintDemeLifeFitnessData",
    "PrintDemeTasksData",
    "PrintPhenotypeStatus",
];

/// A parsed action of this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopPrintAction {
    /// `cActionPrintDonationStats`
    DonationStats,
    /// `cActionPrintDemeAllStats`
    DemeAllStats,
    /// `cActionPrintDemeDonorStats`
    DemeDonor,
    /// `cActionPrintDemeSpacialEnergy`
    DemeSpatialEnergy,
    /// `cActionPrintDemeSpacialSleep`
    DemeSpatialSleep,
    /// `cActionPrintDemeFitness`
    DemeFitness,
    /// `cActionPrintDemeLifeFitness`
    DemeLifeFitness,
    /// `cActionPrintDemeTasks`
    DemeTasks,
    /// `cActionPrintPhenotypeStatus` (`POP_OUT_FILE`)
    PhenotypeStatus { filename: String },
}

/// The constructor of the action `name` (a [`POP_PRINT_ACTIONS`] spelling)
/// applied to `args`; `None` for any other name.
pub fn parse(name: &str, args: &str) -> Option<PopPrintAction> {
    Some(match name {
        "PrintDonationStats" => PopPrintAction::DonationStats,
        "PrintDemeAllStats" | "PrintDemeStats" => PopPrintAction::DemeAllStats,
        "PrintDemeDonorStats" => PopPrintAction::DemeDonor,
        "PrintDemeSpacialEnergyStats" => PopPrintAction::DemeSpatialEnergy,
        "PrintDemeSpacialSleepStats" => PopPrintAction::DemeSpatialSleep,
        "PrintDemeFitnessData" => PopPrintAction::DemeFitness,
        "PrintDemeLifeFitnessData" => PopPrintAction::DemeLifeFitness,
        "PrintDemeTasksData" => PopPrintAction::DemeTasks,
        "PrintPhenotypeStatus" => {
            // `if (largs == "") m_filename = DEFAULT; else m_filename = largs.PopWord();`
            let mut largs = args.to_string();
            let filename = if largs.is_empty() {
                "phenotype_status.dat".to_string()
            } else {
                pop_word(&mut largs)
            };
            PopPrintAction::PhenotypeStatus { filename }
        }
        _ => return None,
    })
}

/// How a per-deme printer reduces its `cDoubleSum`.
#[derive(Clone, Copy)]
enum Reduce {
    Ave,
    Sum,
}

impl World {
    /// `Process(ctx)` of an action of this module.
    pub(crate) fn run_pop_print_action(&mut self, action: &PopPrintAction) -> Result<(), String> {
        match action {
            PopPrintAction::DonationStats => self.print_donation_stats(),
            PopPrintAction::DemeAllStats => self.print_deme_all_stats(),
            PopPrintAction::DemeDonor => self.print_deme_donor(),
            PopPrintAction::DemeSpatialEnergy => self.print_deme_spatial_energy_data(),
            PopPrintAction::DemeSpatialSleep => self.print_deme_spatial_sleep_data(),
            PopPrintAction::DemeFitness => self.print_deme_fitness(),
            PopPrintAction::DemeLifeFitness => self.print_deme_life_fitness(),
            PopPrintAction::DemeTasks => self.print_deme_tasks(),
            PopPrintAction::PhenotypeStatus { filename } => self.print_phenotype_status(filename),
        }
    }

    /// The organism in `cell`, or `None` for an empty cell. The printers run
    /// between updates, so an occupied cell always holds its organism.
    fn printed_org(&self, cell: i32) -> Result<Option<&Organism>, String> {
        if !self.is_occupied(cell) {
            return Ok(None);
        }
        self.pop.orgs[cell as usize]
            .as_deref()
            .map(Some)
            .ok_or_else(|| "deme printer reached the running organism".to_string())
    }

    /// The organisms of deme `d`, in cell order.
    fn deme_orgs(&self, d: usize) -> Result<Vec<&Organism>, String> {
        let deme = &self.pop.demes[d];
        let mut orgs = Vec::new();
        for i in 0..deme.size() {
            if let Some(o) = self.printed_org(deme.cell_id(i))? {
                orgs.push(o);
            }
        }
        Ok(orgs)
    }

    /// The shape shared by `PrintDemeFitness`, `PrintDemeLifeFitness`,
    /// `PrintDemeMerit`, `PrintDemeGestationTime`, `PrintDemeDonor` and
    /// `PrintDemeReceiver`: one `cDoubleSum` of `value` over each deme's
    /// organisms per column.
    fn print_deme_column(
        &mut self,
        filename: &str,
        comment: &str,
        reduce: Reduce,
        value: impl Fn(&Organism) -> f64,
    ) -> Result<(), String> {
        let mut columns = Vec::with_capacity(self.pop.demes.len());
        for d in 0..self.pop.demes.len() {
            let mut sum = DoubleSum::new();
            for o in self.deme_orgs(d)? {
                sum.add(value(o));
            }
            columns.push(match reduce {
                Reduce::Ave => sum.average(),
                Reduce::Sum => sum.sum(),
            });
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment(comment);
        df.write_time_stamp();
        df.write(update, "update");
        for (deme_id, v) in columns.into_iter().enumerate() {
            df.write(v, &format!("Deme {deme_id}"));
        }
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeAllStats(ctx)`
    fn print_deme_all_stats(&mut self) -> Result<(), String> {
        self.print_deme_fitness()?;
        self.print_deme_life_fitness()?;
        self.print_deme_column(
            "deme_merit.dat",
            "Average merits for each deme in population",
            Reduce::Ave,
            |o| o.state.phenotype.merit.get_double(),
        )?;
        self.print_deme_column(
            "deme_gest_time.dat",
            "Average gestation time for each deme in population",
            Reduce::Ave,
            |o| f64::from(o.state.phenotype.gestation_time),
        )?;
        self.print_deme_tasks()?;
        self.print_deme_donor()?;
        self.print_deme_column(
            "deme_receiver.dat",
            "Num orgs doing receiving a donate for each deme in population",
            Reduce::Sum,
            |o| f64::from(u8::from(o.state.phenotype.is_receiver)),
        )?;
        self.print_deme_mutation_rate()?;
        self.print_deme_resource()?;
        self.print_deme_instructions()?;

        if self.cfg.energy_enabled {
            self.print_deme_spatial_energy_data()?;
            self.print_deme_spatial_sleep_data()?;
        }
        Ok(())
    }

    /// `cPopulation::PrintDemeFitness()`
    fn print_deme_fitness(&mut self) -> Result<(), String> {
        self.print_deme_column(
            "deme_fitness.dat",
            "Average fitnesses for each deme in the population",
            Reduce::Ave,
            |o| o.state.phenotype.fitness,
        )
    }

    /// `cPopulation::PrintDemeLifeFitness()`
    fn print_deme_life_fitness(&mut self) -> Result<(), String> {
        self.print_deme_column(
            "deme_lifetime_fitness.dat",
            "Average life fitnesses for each deme in the population",
            Reduce::Ave,
            |o| o.state.phenotype.life_fitness,
        )
    }

    /// `cPopulation::PrintDemeDonor()`
    fn print_deme_donor(&mut self) -> Result<(), String> {
        self.print_deme_column(
            "deme_donor.dat",
            "Num orgs doing doing a donate for each deme in population",
            Reduce::Sum,
            |o| f64::from(u8::from(o.state.phenotype.is_donor_last)),
        )
    }

    /// `cPopulation::PrintDemeTasks()`: how many organisms of each deme did
    /// each task in their last gestation.
    fn print_deme_tasks(&mut self) -> Result<(), String> {
        let num_task = self.env.num_tasks().max(0) as usize;
        let mut columns = Vec::new();
        for d in 0..self.pop.demes.len() {
            // `Apto::Stat::Accumulator<int>`
            let mut single_deme_task = vec![0i32; num_task];
            for o in self.deme_orgs(d)? {
                for (j, n) in single_deme_task.iter_mut().enumerate() {
                    // only interested if task is done once!
                    if o.state
                        .phenotype
                        .last_task_count
                        .get(j)
                        .is_some_and(|&c| c > 0)
                    {
                        *n += 1;
                    }
                }
            }
            for (j, n) in single_deme_task.into_iter().enumerate() {
                columns.push((n, format!("Deme {d}, Task {j}")));
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_task.dat")?;
        df.write_comment("Num orgs doing each task for each deme in population");
        df.write_time_stamp();
        df.write(update, "update");
        for (n, descr) in columns {
            df.write(n, &descr);
        }
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeMutationRate()`
    fn print_deme_mutation_rate(&mut self) -> Result<(), String> {
        let mut columns = Vec::new();
        let mut total_mut_rate = DoubleSum::new();
        for d in 0..self.pop.demes.len() {
            let mut single_deme_mut_rate = DoubleSum::new();
            for o in self.deme_orgs(d)? {
                single_deme_mut_rate.add(o.state.mut_rates.copy.mut_prob);
            }
            columns.push(single_deme_mut_rate.average());
            total_mut_rate.add(single_deme_mut_rate.average());
        }
        let update = self.stats.update();
        let df = self.out.static_with_path("deme_mut_rates.dat")?;
        df.write_comment("Average mutation rates for organisms in each deme");
        df.write_time_stamp();
        df.write(update, "update");
        for (deme_id, v) in columns.into_iter().enumerate() {
            df.write(v, &format!("Deme {deme_id}"));
        }
        df.write(
            total_mut_rate.average(),
            "Average deme mutation rate averaged across Demes.",
        );
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDemeInstructions()`: per deme and instruction set,
    /// the summed last-gestation execution counts of the deme's organisms
    /// of that instruction set.
    fn print_deme_instructions(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let inst_sets: Vec<(String, usize)> = self
            .hw
            .registry()
            .inst_sets()
            .iter()
            .map(|is| (is.inst_set_name().to_string(), is.size().max(0) as usize))
            .collect();
        for d in 0..self.pop.demes.len() {
            for (inst_set, num_inst) in &inst_sets {
                // `Apto::Stat::Accumulator<int>`
                let mut single_deme_inst = vec![0i32; *num_inst];
                for o in self.deme_orgs(d)? {
                    if o.state.genome().inst_set() != inst_set {
                        continue;
                    }
                    for (j, s) in single_deme_inst.iter_mut().enumerate() {
                        let c = o.state.phenotype.last_inst_count.get(j).copied().ok_or(
                            "PrintDemeInstructions: short instruction count (C++: out-of-bounds \
                             array read)",
                        )?;
                        *s = s.wrapping_add(c);
                    }
                }
                let df = self
                    .out
                    .static_with_path(&format!("deme_instruction-{d}-{inst_set}.dat"))?;
                df.write_comment(&format!(
                    "Number of times each instruction is exectued in deme {d}"
                ));
                df.write_time_stamp();
                df.write(update, "update");
                for (j, s) in single_deme_inst.into_iter().enumerate() {
                    df.write(s, &format!("Inst {j}"));
                }
                df.endl();
            }
        }
        Ok(())
    }

    /// `cPopulation::PrintDemeSpatialEnergyData()`: one Matlab array per
    /// deme and call, of the stored energy in each of the deme's cells.
    /// QUIRK: the cells are read as consecutive ids from 0 (the deme layout
    /// is ignored), and the rows wrap at the world's width.
    fn print_deme_spatial_energy_data(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let world_x = self.cfg.world_x;
        let mut cell_id = 0;
        for d in 0..self.pop.demes.len() {
            let gridsize = self.pop.demes[d].size();
            let mut values = Vec::with_capacity(gridsize.max(0) as usize);
            for _ in 0..gridsize {
                values.push(match self.printed_org(cell_id)? {
                    Some(o) => o.state.phenotype.energy_store,
                    None => 0.0,
                });
                cell_id += 1;
            }
            let df = self
                .out
                .static_with_path(&format!("deme_{d:07}_spatial_energy.m"))?;
            df.write_raw(&format!("deme_{d:07}_energy_{update:07} = [ ..."));
            for (j, v) in values.into_iter().enumerate() {
                df.write_block_element(v, j as i32, world_x);
            }
            df.write_raw("];");
            df.endl();
        }
        Ok(())
    }

    /// `cPopulation::PrintDemeSpatialSleepData()`: like
    /// [`print_deme_spatial_energy_data`](Self::print_deme_spatial_energy_data)
    /// for `IsSleeping()` (an `int` element; an empty cell writes the
    /// `double` 0.0), all demes in one file.
    fn print_deme_spatial_sleep_data(&mut self) -> Result<(), String> {
        let update = self.stats.update();
        let world_x = self.cfg.world_x;
        let mut cell_id = 0;
        let mut demes = Vec::with_capacity(self.pop.demes.len());
        for d in 0..self.pop.demes.len() {
            let gridsize = self.pop.demes[d].size();
            let mut values = Vec::with_capacity(gridsize.max(0) as usize);
            for _ in 0..gridsize {
                values.push(self.printed_org(cell_id)?.map(|o| o.state.is_sleeping));
                cell_id += 1;
            }
            demes.push(values);
        }
        let df = self.out.static_with_path("deme_spatial_sleep.m")?;
        for (d, values) in demes.into_iter().enumerate() {
            df.write_raw(&format!("deme_{d:07}_sleep_{update:07} = [ ..."));
            for (j, v) in values.into_iter().enumerate() {
                match v {
                    Some(sleeping) => {
                        df.write_block_element(i32::from(sleeping), j as i32, world_x)
                    }
                    None => df.write_block_element(0.0, j as i32, world_x),
                }
            }
            df.write_raw("];");
            df.endl();
        }
        Ok(())
    }

    /// `cPopulation::PrintDemeSpatialResData(res, i, deme_id, ctx)`: the
    /// spatial deme resource `res_name` of deme `deme_id` as a Matlab array
    /// (`amounts` in cell order), rows wrapping at the world's width.
    pub(crate) fn print_deme_spatial_res_data(
        &mut self,
        res_name: &str,
        deme_id: usize,
        amounts: &[f64],
    ) -> Result<(), String> {
        let update = self.stats.update();
        let world_x = self.cfg.world_x;
        let df = self
            .out
            .static_with_path(&format!("deme_spatial_resource_{res_name}.m"))?;
        df.write_raw(&format!("deme_{deme_id:07}_{res_name}_{update:07} = [ ..."));
        for (j, &v) in amounts.iter().enumerate() {
            df.write_block_element(v, j as i32, world_x);
        }
        df.write_raw("];");
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintDonationStats()`: how many organisms' parents
    /// made or received each kind of donation.
    fn print_donation_stats(&mut self) -> Result<(), String> {
        // makers, receivers, cheaters per kind
        let mut sums = [[DoubleSum::new(), DoubleSum::new(), DoubleSum::new()]; 5];
        for cell in 0..self.pop.cells.len() as i32 {
            let Some(o) = self.printed_org(cell)? else {
                continue;
            };
            let ph = &o.state.phenotype;
            let kinds = [
                (ph.is_donor_last, ph.is_receiver_last),
                (ph.is_donor_edit_last, ph.is_receiver_edit_last),
                (ph.is_donor_kin_last, ph.is_receiver_kin_last),
                (ph.is_donor_threshgb_last, ph.is_receiver_threshgb_last),
                (
                    ph.is_donor_quanta_threshgb_last,
                    ph.is_receiver_quanta_threshgb_last,
                ),
            ];
            for (s, (donor, receiver)) in sums.iter_mut().zip(kinds) {
                if donor {
                    s[0].add(1.0);
                }
                if receiver {
                    s[1].add(1.0);
                    if !donor {
                        s[2].add(1.0);
                    }
                }
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path("donations.dat")?;
        df.write_comment("Info about organisms giving donations in the population");
        df.write_time_stamp();
        df.write(update, "update");
        for (s, kind) in sums.iter().zip([
            "donation",
            "edit_donation",
            "kin_donation",
            "threshgb_donation",
            "quanta_threshgb_donation",
        ]) {
            df.write(s[0].sum(), &format!("parent made at least one {kind}"));
            df.write(s[1].sum(), &format!("parent received at least one {kind}"));
            df.write(
                s[2].sum(),
                &format!("parent received at least one {kind} but did not make one"),
            );
        }
        df.endl();
        Ok(())
    }

    /// `cPopulation::PrintPhenotypeStatus(filename)`: the merit components
    /// of every organism, in cell order.
    fn print_phenotype_status(&mut self, filename: &str) -> Result<(), String> {
        let mut rows = Vec::new();
        for i in 0..self.pop.cells.len() as i32 {
            let Some(o) = self.printed_org(i)? else {
                continue;
            };
            let ph = &o.state.phenotype;
            rows.push((
                i,
                ph.merit.get_double(),
                ph.cur_merit_base(&self.cfg),
                ph.cur_bonus,
                ph.last_merit_base,
                ph.last_bonus,
                ph.life_fitness,
            ));
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Num orgs doing each task for each deme in population");
        df.write_time_stamp();
        df.write(update, "Update");
        for (i, merit, base, bonus, last_base, last_bonus, life_fitness) in rows {
            df.write(merit, &format!("cur_merit {i};"));
            df.write(base, &format!("cur_merit_base {i};"));
            df.write(bonus, &format!("cur_merit_bonus {i};"));
            df.write(last_base, &format!("last_merit_base {i}"));
            df.write(last_bonus, &format!("last_merit_bonus {i}"));
            df.write(life_fitness, &format!("life_fitness {i}"));
            df.write("*", "*");
        }
        df.endl();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_registered_cpp_actions() {
        for n in POP_PRINT_ACTIONS {
            assert!(crate::action_library::is_registered(n), "{n}");
            assert!(parse(n, "").is_some(), "{n}");
            assert_eq!(crate::actions::canonical_name(n), Some(*n));
        }
        assert_eq!(
            parse("PrintPhenotypeStatus", "ps.dat more"),
            Some(PopPrintAction::PhenotypeStatus {
                filename: "ps.dat".to_string()
            })
        );
        assert_eq!(
            parse("PrintDemeStats", ""),
            Some(PopPrintAction::DemeAllStats)
        );
    }
}
