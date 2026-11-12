//! The population layer's half of an analyze-mode run
//! ([`avida_analyze::SetupHooks`]): `cWorld::setup` builds the migration
//! matrix and the whole `cPopulation` in analyze mode too, and `-e` prints
//! the action library. Analyze mode then never touches the population
//! again; what remains visible is the world RNG after the set-up draws
//! (topology, time slicer, gradient resources) and the lines set-up
//! printed, so the population built here is dropped.

use avida_analyze::SetupHooks;
use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::Feedback;
use avida_hardware::task_adapter::Env;

use crate::migration_matrix::MigrationMatrix;
use crate::population::Population;
use crate::stats::Stats;
use crate::world::WorldResEnv;

/// The hooks `avida -a` runs analyze mode with.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalyzeSetup;

impl SetupHooks for AnalyzeSetup {
    fn describe_all_actions(&self) -> Option<String> {
        Some(crate::action_library::describe_all())
    }

    fn load_migration_matrix(
        &mut self,
        config: &Config,
        working_dir: &str,
        feedback: &mut Feedback,
    ) -> Option<bool> {
        let count_parasites = config.demes_parasite_migration_rate > 0.0;
        let count_offspring = config.demes_migration_rate > 0.0;
        Some(MigrationMatrix::new().load(
            config.num_demes,
            &config.migration_file,
            working_dir,
            count_parasites,
            count_offspring,
            false,
            feedback,
        ))
    }

    fn setup_population(
        &mut self,
        config: &mut Config,
        env: &Env,
        rng: &mut AvidaRng,
    ) -> Option<Result<Vec<String>, String>> {
        Some(setup_population(config, env, rng))
    }
}

/// `new cPopulation(world)` as `World::new` runs it, for its side effects.
fn setup_population(
    config: &mut Config,
    env: &Env,
    rng: &mut AvidaRng,
) -> Result<Vec<String>, String> {
    let task_names: Vec<String> = env
        .tasklib()
        .0
        .tasks()
        .iter()
        .map(|t| t.desc().to_string())
        .collect();
    let num_reactions = env.num_reactions().max(0) as usize;
    let reaction_names: Vec<String> = (0..num_reactions)
        .map(|i| env.reaction_name(i).to_string())
        .collect();
    let num_resources = env.resource_lib().size().max(0) as usize;
    let mut stats = Stats::new(
        task_names,
        reaction_names,
        num_resources,
        f64::from(config.res_for_deme_rep),
    );
    let mut res_env = WorldResEnv {
        update: stats.update(),
        ..WorldResEnv::default()
    };
    let (_pop, report) = Population::new(
        config,
        env.mut_rates(),
        env.resource_lib(),
        &mut stats,
        rng,
        &mut res_env,
        env.num_tasks().max(0) as usize,
        num_reactions,
    )?;
    if let Some(what) = res_env.unsupported.take() {
        return Err(what);
    }
    // No driver is attached while the world is set up: the C++ reports
    // through a NULL `WorldDriver`.
    if let Some(msg) = res_env.errors.first().or(report.warnings.first()) {
        return Err(format!(
            "unsupported: population set-up reports '{msg}' (C++: \
             cWorld::GetDriver() dereferences the NULL driver during cWorld::setup)"
        ));
    }
    Ok(report.stdout)
}
