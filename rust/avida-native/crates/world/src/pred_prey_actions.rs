//! Event actions of the predator/prey and avatar experiments: the
//! constructor (argument parsing) and `Process` halves of these `cAction*`
//! classes at 47f13dadb:
//!
//! * PopulationActions.cc: `cActionInjectGroup`, with the group half of
//!   `cPopulation::Inject` it reaches;
//! * PrintActions.cc: `cActionDumpTargetGrid`, `cActionPrintOrgLocData` and
//!   the forager-class instruction printers
//!   (`cActionPrint{Prey,Predator,TopPredator}[FromSensor]InstructionData`).

use avida_apto::AvidaRng;
use avida_config::feedback::Feedback;
use avida_config::string_util::{as_double, as_int, pop_word, trim};
use avida_genome::{ArgContainer, ArgSchema, Genome};
use avida_hardware::org_set_forage_target;
use avida_hardware::test_cpu::TestCpu;
use avida_systematics::apto_map::AptoMap;

use crate::pop_interface::PopulationInterface;
use crate::world::World;

/// Action names (the registered spellings).
pub const PRED_PREY_ACTIONS: &[&str] = &[
    "InjectGroup",
    "DumpTargetGrid",
    "PrintOrgLocData",
    "PrintPreyInstructionData",
    "PrintPredatorInstructionData",
    "PrintTopPredatorInstructionData",
    "PrintPreyFromSensorInstructionData",
    "PrintPredatorFromSensorInstructionData",
    "PrintTopPredatorFromSensorInstructionData",
    "SaveFlameData",
    "PrintDominantForagerGenotypes",
];

/// Which forager-class instruction counts a printer writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FtInstData {
    Prey,
    Predator,
    TopPredator,
    PreyFromSensor,
    PredatorFromSensor,
    TopPredatorFromSensor,
}

/// A parsed action. Field names follow the C++ members (`m_` dropped).
#[derive(Debug, Clone, PartialEq)]
pub enum PredPreyAction {
    /// `cActionInjectGroup`; `group_id` `None` is the `DEFAULT_GROUP`
    /// default, read when the action runs.
    InjectGroup {
        filename: String,
        cell_id: i32,
        group_id: Option<i32>,
        forager_type: i32,
        trace: bool,
        merit: f64,
        lineage_label: i32,
        neutral_metric: f64,
        trace_filename: String,
    },
    /// `cActionDumpTargetGrid`
    DumpTargetGrid { filename: String },
    /// `cActionPrintOrgLocData`
    PrintOrgLocData { filename: String },
    /// `cActionPrint*InstructionData` of one forager class.
    PrintFtInstructionData {
        which: FtInstData,
        filename: String,
        inst_set: String,
    },
    /// `cActionSaveFlameData` (SaveLoadActions.cc)
    SaveFlameData { filename: String },
    /// `cActionPrintDominantForagerGenotypes`
    PrintDominantForagerGenotypes { filename: String },
}

/// `if (largs.GetSize()) x = largs.PopWord()...`
struct Args(String);

impl Args {
    fn word(&mut self) -> Option<String> {
        if self.0.is_empty() {
            None
        } else {
            Some(pop_word(&mut self.0))
        }
    }
}

/// The constructor of `cAction<name>`, or `None` when `name` is not one of
/// [`PRED_PREY_ACTIONS`].
pub fn parse(
    name: &str,
    args: &str,
    default_inst_set: &str,
    feedback: &mut Feedback,
) -> Option<PredPreyAction> {
    let mut a = Args(args.to_string());
    let ft_inst = |which: FtInstData, default: &str, pattern: &str| {
        // `largs.Trim(); if (largs.GetSize()) m_filename = largs.PopWord();
        // else m_filename = DEFAULT;`
        let mut largs = args.to_string();
        trim(&mut largs);
        let mut a = Args(largs);
        let mut filename = a.word().unwrap_or_else(|| default.to_string());
        let inst_set = a.word().unwrap_or_else(|| default_inst_set.to_string());
        if filename.is_empty() {
            filename = pattern.replace("%s", &inst_set);
        }
        PredPreyAction::PrintFtInstructionData {
            which,
            filename,
            inst_set,
        }
    };
    Some(match name {
        "InjectGroup" => {
            let filename = a.word().unwrap_or_default();
            let cell_id = a.word().map_or(0, |w| as_int(&w));
            let group_id = a.word().map(|w| as_int(&w));
            let forager_type = a.word().map_or(-1, |w| as_int(&w));
            let trace = a.word().is_some_and(|w| as_int(&w) != 0);
            let merit = a.word().map_or(-1.0, |w| as_double(&w));
            let lineage_label = a.word().map_or(0, |w| as_int(&w));
            let neutral_metric = a.word().map_or(0.0, |w| as_double(&w));
            let trace_filename = a.word().unwrap_or_default();
            PredPreyAction::InjectGroup {
                filename,
                cell_id,
                group_id,
                forager_type,
                trace,
                merit,
                lineage_label,
                neutral_metric,
                trace_filename,
            }
        }
        "DumpTargetGrid" => PredPreyAction::DumpTargetGrid {
            filename: a.word().unwrap_or_default(),
        },
        "SaveFlameData" => {
            // `if (args) m_filename = argc->GetString(0);`: `args` is a
            // non-null string, so a failed load leaves the empty name.
            let mut schema = ArgSchema::new(':', '=', false);
            schema.add_string("filename", 0, "flame_data");
            let filename = ArgContainer::load(args, &schema, feedback)
                .map_or_else(String::new, |c| c.get_string(0).to_string());
            PredPreyAction::SaveFlameData { filename }
        }
        "PrintDominantForagerGenotypes" => PredPreyAction::PrintDominantForagerGenotypes {
            filename: a.word().unwrap_or_default(),
        },
        "PrintOrgLocData" => PredPreyAction::PrintOrgLocData {
            filename: a.word().unwrap_or_default(),
        },
        "PrintPreyInstructionData" => ft_inst(
            FtInstData::Prey,
            "prey_instruction.dat",
            "prey_instruction-%s.dat",
        ),
        "PrintPredatorInstructionData" => ft_inst(
            FtInstData::Predator,
            "predator_instruction.dat",
            "predator_instruction-%s.dat",
        ),
        "PrintTopPredatorInstructionData" => ft_inst(
            FtInstData::TopPredator,
            "top_pred_instruction.dat",
            "top_pred_instruction-%s.dat",
        ),
        "PrintPreyFromSensorInstructionData" => ft_inst(
            FtInstData::PreyFromSensor,
            "prey_from_sensor_exec.dat",
            "prey_from_sensor_exec-%s.dat",
        ),
        "PrintPredatorFromSensorInstructionData" => ft_inst(
            FtInstData::PredatorFromSensor,
            "predator_from_sensor_exec.dat",
            "predator_from_sensor_exec-%s.dat",
        ),
        "PrintTopPredatorFromSensorInstructionData" => ft_inst(
            FtInstData::TopPredatorFromSensor,
            "top_pred_from_sensor_exec.dat",
            "top_pred_from_sensor_exec-%s.dat",
        ),
        _ => return None,
    })
}

impl World {
    /// `Process` of a [`PredPreyAction`].
    pub(crate) fn run_pred_prey_action(&mut self, action: &PredPreyAction) -> Result<(), String> {
        match action {
            PredPreyAction::InjectGroup {
                filename,
                cell_id,
                group_id,
                forager_type,
                trace,
                merit,
                lineage_label,
                neutral_metric,
                trace_filename,
            } => {
                if filename.is_empty() {
                    eprintln!("error: no organism file specified");
                    return Ok(());
                }
                let Some(genome) = self.load_genome(filename, "", false)? else {
                    return Ok(());
                };
                if *cell_id < 0 || *cell_id as usize >= self.pop.cells.len() {
                    return Err(format!(
                        "InjectGroup into nonexistent cell {cell_id} (C++: out-of-bounds cell)"
                    ));
                }
                let group_id = group_id.unwrap_or(self.cfg.default_group);
                // `trace`: the inject ends with `SetupMiniTrace` on the
                // organism in the (possibly moved) target cell.
                self.mini_trace.inject_trace = *trace;
                let injected = self.inject_into_group(
                    &genome,
                    crate::run::injected_source(),
                    *cell_id,
                    *merit,
                    *lineage_label,
                    *neutral_metric,
                    Some((group_id, *forager_type)),
                );
                self.mini_trace.inject_trace = false;
                injected?;
                if !trace_filename.is_empty() {
                    return self.set_status_trace(*cell_id, trace_filename);
                }
                Ok(())
            }
            PredPreyAction::DumpTargetGrid { filename } => self.dump_target_grid(filename),
            PredPreyAction::PrintOrgLocData { filename } => self.print_org_loc_data(filename),
            PredPreyAction::PrintFtInstructionData {
                which,
                filename,
                inst_set,
            } => {
                let (stats, out) = (&self.stats, &mut self.out);
                match which {
                    FtInstData::Prey => stats.print_prey_instruction_data(out, filename, inst_set),
                    FtInstData::Predator => {
                        stats.print_predator_instruction_data(out, filename, inst_set)
                    }
                    FtInstData::TopPredator => {
                        stats.print_top_predator_instruction_data(out, filename, inst_set)
                    }
                    FtInstData::PreyFromSensor => {
                        stats.print_prey_from_sensor_instruction_data(out, filename, inst_set)
                    }
                    FtInstData::PredatorFromSensor => {
                        stats.print_predator_from_sensor_instruction_data(out, filename, inst_set)
                    }
                    FtInstData::TopPredatorFromSensor => stats
                        .print_top_predator_from_sensor_instruction_data(out, filename, inst_set),
                }
            }
            PredPreyAction::SaveFlameData { filename } => {
                let update = self.stats.update();
                self.save_flame_data(&format!("flamedat/{filename}-{update}.spop"))
            }
            PredPreyAction::PrintDominantForagerGenotypes { filename } => {
                self.print_dominant_forager_genotypes(filename)
            }
        }
    }

    /// `cPopulation::SaveFlameData(filename)`: id, abundance and depth of
    /// every genotype (parasites' included) with a living member, in the
    /// order of the C++ `Apto::Map` keyed by genotype id.
    fn save_flame_data(&mut self, filename: &str) -> Result<(), String> {
        let mut df = self.out.create_with_path(filename)?;
        df.write_comment("Flame Data Save");
        df.write_time_stamp();

        // Build up hash table of all current genotypes
        let mut index: AptoMap<i32, usize> = AptoMap::new();
        let mut groups = Vec::new();
        for cell in 0..self.pop.cells.len() {
            let Some(org) = self.pop.orgs[cell].as_ref() else {
                continue;
            };
            // Handle any parasites
            let mut found: Vec<_> = org
                .state
                .parasites
                .iter()
                .filter_map(|p| self.parasite_genotype(p))
                .collect();
            // Handle the organism itself
            if let Some(g) = self.systematics.genotype_of(&self.pop.membership[cell]) {
                found.push(g);
            }
            for g in found {
                let Some(id) = self.systematics.group_id(g) else {
                    continue;
                };
                if !index.has(&id) {
                    groups.push(g);
                    index.set(&id, groups.len() - 1);
                }
            }
        }

        // Output all current genotypes
        for &i in index.values() {
            let g = groups[i];
            let sys = &self.systematics;
            df.write_with_format(sys.group_id(g).unwrap_or(-1), "ID", "genotype_id");
            df.write_with_format(
                sys.num_units(g).unwrap_or(0),
                "Number of currently living organisms",
                "num_units",
            );
            df.write_with_format(sys.depth(g).unwrap_or(0), "Phylogenetic Depth", "depth");
            df.endl();
        }
        Ok(())
    }

    /// `cActionPrintDominantForagerGenotypes::Process`: the genome of the
    /// most abundant genotype born as each forager type, tested on a test
    /// CPU with its own RNG (seed 0) so the world's draws are untouched.
    fn print_dominant_forager_genotypes(&mut self, m_filename: &str) -> Result<(), String> {
        let Some(role) = self.systematics.arbiter_for_role("genotype") else {
            return Ok(());
        };
        let pps = self.cfg.pred_prey_switch;
        let mut num_fts = if pps == -2 || pps > -1 { 3 } else { 1 };
        num_fts += self
            .env
            .target_ids()
            .iter()
            .filter(|&&t| t != -1 && t != -2 && t != -3)
            .count() as i32;

        let handles = match self.systematics.genotypes() {
            Some(a) => a.iter_groups(),
            None => return Ok(()),
        };
        let mut it = handles.into_iter();
        let mut checked: Vec<i32> = Vec::new();
        let update = self.stats.update();
        let mut i = 0;
        while i < num_fts {
            let Some(handle) = it.next() else {
                break;
            };
            let bg = avida_systematics::GroupRef { role, handle };
            let prop = |name: &str| self.systematics.property(bg, name);
            if prop("threshold").bool_value() || i == 0 {
                let last_birth_group_id = prop("last_group_id").int_value();
                let last_birth_cell = prop("last_birth_cell").int_value();
                let last_birth_forager_type = prop("last_forager_type").int_value();
                if i != 0 && checked.contains(&last_birth_forager_type) {
                    // `i--; continue;` then the loop's `i++`.
                    continue;
                }
                checked.push(last_birth_forager_type);
                let filename = if m_filename.is_empty() {
                    format!(
                        "archive/ft{}_grp{}_{}.org",
                        last_birth_forager_type,
                        last_birth_group_id,
                        prop("name").string_value()
                    )
                } else {
                    format!("{m_filename}.ft{last_birth_forager_type}_grp{last_birth_group_id}")
                };
                let genome = Genome::from_string(&prop("genome").string_value());
                let mut df = self.out.create_with_path(&filename)?;
                // need a random number generator to pass to testcpu that does
                // not affect any other random number pulls
                let mut rng = AvidaRng::new(0);
                let result = {
                    let mut tw = crate::world::make_test_world(
                        &self.cfg,
                        &mut self.env,
                        &mut rng,
                        &self.hw,
                        &mut self.stats,
                        &mut self.host,
                        &*self.group_attack_fn,
                        self.test_sterilize,
                        &self.pop,
                    );
                    TestCpu::new(&mut tw).and_then(|mut cpu| {
                        cpu.print_genome(
                            &mut tw,
                            &genome,
                            &mut df,
                            &filename,
                            update,
                            true,
                            last_birth_cell,
                            last_birth_group_id,
                            last_birth_forager_type,
                        )
                    })
                };
                self.flush_host();
                result?;
            }
            i += 1;
        }
        Ok(())
    }

    /// The tail of `cPopulation::Inject`: the `inject_group` block and the
    /// avatar of an injected organism.
    pub(crate) fn inject_group_and_avatar(&mut self, cell_id: i32, group: Option<(i32, i32)>) {
        let c = cell_id as usize;
        if let Some((group_id, forager_type)) = group {
            let update = self.stats.update();
            let Some(mut org) = self.pop.orgs[c].take() else {
                return;
            };
            if let Err(e) = org.state.set_opinion(&self.cfg, update, group_id) {
                self.set_fatal(e);
            }
            self.join_group(&mut [(cell_id, &mut org.state)], cell_id, group_id);
            {
                // `SetForageTarget(ctx, forager_type)`: not an inject-time
                // switch, so the organism leaves the prey count it entered.
                let mut iface = PopulationInterface { w: self, cell_id };
                if let Err(e) =
                    org_set_forage_target(&mut org.state, &mut iface, forager_type, false)
                {
                    self.set_fatal(e);
                }
            }
            org.state.phenotype.birth_cell_id = cell_id;
            self.try_write_birth_loc_data(&org.state);
            org.state.phenotype.birth_group_id = group_id;
            org.state.phenotype.birth_forager_type = forager_type;
            self.pop.orgs[c] = Some(org);
        }
        if self.cfg.use_avatars != 0 && !self.cfg.neural_networking {
            self.add_pred_prey_av_of_cell(cell_id);
        }
        // `if (trace) SetupMiniTrace(cell_array[cell_id].GetOrganism())`
        self.mini_trace_after_inject(cell_id);
    }

    /// `cStats::PrintTargets` / `PrintTopPredTargets` over the forage
    /// targets of the living organisms.
    pub(crate) fn print_targets_action(
        &mut self,
        method: &str,
        filename: &str,
    ) -> Result<(), String> {
        let forage_targets: Vec<i32> = self
            .pop
            .live_org_list
            .iter()
            .filter_map(|&c| self.pop.orgs[c as usize].as_ref())
            .map(|o| o.state.forage_target)
            .collect();
        let pred_prey_switch = self.cfg.pred_prey_switch;
        let target_ids = self.env.target_ids().clone();
        if method == "PrintTopPredTargets" {
            self.stats.print_top_pred_targets(
                &mut self.out,
                filename,
                pred_prey_switch,
                &target_ids,
                &forage_targets,
            )
        } else {
            self.stats.print_targets(
                &mut self.out,
                filename,
                pred_prey_switch,
                &target_ids,
                &forage_targets,
            )
        }
    }

    /// `GetFacedDir()` of the organism in `cell_id`.
    pub(crate) fn faced_dir_of(&self, cell_id: i32) -> i32 {
        let cell = &self.pop.cells[cell_id as usize];
        match cell.cell_faced() {
            Some(f) => {
                let faced = &self.pop.cells[f as usize];
                cell.faced_dir((faced.x, faced.y))
            }
            None => 0,
        }
    }

    /// `cActionDumpTargetGrid::Process`: the forage target in each cell,
    /// -99 for none.
    fn dump_target_grid(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let use_av = self.cfg.use_avatars != 0;
        let filename = match (filename.is_empty(), use_av) {
            (false, _) => filename.to_string(),
            (true, true) => format!("grid_dumps/avatar_grid.{update}.dat"),
            (true, false) => format!("grid_dumps/target_grid.{update}.dat"),
        };
        let mut df = self.out.create_with_path(&filename)?;
        let world_x = self.pop.world_x;
        for j in 0..self.pop.world_y {
            let mut line = String::new();
            for i in 0..world_x {
                let cell = j * world_x + i;
                let target = if use_av {
                    self.avatar_grid_target(cell)
                } else {
                    self.pop.orgs[cell as usize]
                        .as_ref()
                        .map_or(-99, |o| o.state.forage_target)
                };
                line.push_str(&format!("{target} "));
            }
            line.push('\n');
            std::io::Write::write_all(df.ofstream(), line.as_bytes()).map_err(|e| e.to_string())?;
            df.flush();
        }
        Ok(())
    }

    /// `cActionPrintOrgLocData::Process`: organism locations and other
    /// organism data (for movies).
    fn print_org_loc_data(&mut self, filename: &str) -> Result<(), String> {
        let update = self.stats.update();
        let filename = if filename.is_empty() {
            format!("grid_dumps/org_loc.{update}.dat")
        } else {
            filename.to_string()
        };
        let mut df = self.out.create_with_path(&filename)?;
        let use_av = self.cfg.use_avatars != 0;
        let mut text = String::new();
        if !use_av {
            text.push_str(
                "# org_id,org_cellx,org_celly,org_forage_target,org_group_id,org_facing\n",
            );
        } else {
            text.push_str(
                "# org_id,org_cellx,org_celly,org_forage_target,org_group_id,org_facing,\
                 av_cellx,av_celly,av_facing\n",
            );
        }
        let world_x = self.cfg.world_x;
        let live: Vec<i32> = self.pop.live_org_list.clone();
        for loc in live {
            let faced_dir = self.faced_dir_of(loc);
            let Some(org) = self.pop.orgs[loc as usize].as_mut() else {
                continue;
            };
            let (id, uid) = (org.state.id, org.state.uid);
            let ft = org.state.forage_target;
            let opinion = if org.state.has_opinion() {
                org.state.get_opinion().0
            } else {
                -1
            };
            text.push_str(&format!(
                "{id},{},{},{ft},{opinion},{faced_dir}",
                loc % world_x,
                loc / world_x
            ));
            if use_av {
                let (avloc, avfaced_dir) = self.avatar_cell_and_facing(uid);
                text.push_str(&format!(
                    ",{},{},{avfaced_dir}",
                    avloc % world_x,
                    avloc / world_x
                ));
            }
            text.push('\n');
        }
        std::io::Write::write_all(df.ofstream(), text.as_bytes()).map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }
}
