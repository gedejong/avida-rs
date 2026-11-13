//! Reaction evaluation half of `cEnvironment` (avida-core/source/main/cEnvironment.cc):
//! `SetupInputs`, `SwapInputs`, `TestInput`, `TestOutput`, `TestRequisites`,
//! `TestContextRequisites`, `GetTaskProbability` and `DoProcesses`.
//!
//! Floating point: none of the expressions here has the `a*b + c` shape the
//! reference build fuses, except the inlined `Random::GetRandNormal(mean,
//! variance)` used for detection, which `AvidaRng::get_rand_normal_params`
//! already computes with `mul_add`. Multi-statement sequences
//! (`consumed = rc - used; consumed *= frac;`) are kept as separate
//! roundings.

use crate::context_phenotype::ContextPhenotype;
use crate::environment::Environment;
use crate::reaction::{proc_type, PhenPlastBonusMethod};
use crate::reaction_result::ReactionResult;
use crate::task::{TaskEvaluator, TaskRegistry};
use avida_apto::AvidaRng;
use avida_config::config::Config;

/// Inputs for `DoProcesses` that come straight from `TestOutput`'s
/// arguments and locals.
pub(crate) struct ProcessArgs<'a> {
    pub resource_count: &'a [f64],
    pub rbins_count: &'a [f64],
    pub task_quality: f64,
    pub task_probability: f64,
    pub task_count: i32,
    pub reaction_id: usize,
}

impl<T: TaskRegistry> Environment<T> {
    /// `SetupInputs(ctx, input_array, random)`: fill the organism's input
    /// buffer (resized to the input size).
    ///
    /// * specific inputs (`SetSpecificInputs`) win, optionally with the
    ///   bits in the random mask re-drawn (`GetUInt(1 << 24)` per input);
    /// * random, truly random tasks present: `GetUInt(1 << 31)` per input;
    /// * random otherwise: inputs 0..3 get top bytes 0x0F, 0x33, 0x55 over a
    ///   random 24-bit value, the rest are random 24-bit values;
    /// * not random: the fixed patterns 0x0f13149f, 0x3308e53e, 0x556241eb,
    ///   further inputs being these shifted left by `i / 3`.
    pub fn setup_inputs(&self, rng: &mut AvidaRng, input_array: &mut Vec<i32>, random: bool) {
        let input_size = self.input_size.max(0) as usize;
        input_array.resize(input_size, 0);

        if self.use_specific_inputs {
            *input_array = self.specific_inputs.clone();
            if self.mask != 0 {
                let mask = self.mask;
                for i in 0..input_size {
                    let r = rng.get_uint(1 << 24);
                    // Writing past shorter specific inputs is undefined in C++.
                    if let Some(v) = input_array.get_mut(i) {
                        *v = (((*v as u32) & !mask) | (mask & r)) as i32;
                    }
                }
            }
        } else if random {
            if self.true_rand {
                for v in input_array.iter_mut() {
                    *v = rng.get_uint(1u32 << 31) as i32;
                }
            } else {
                // The top 8 bits of the first three inputs: 00001111,
                // 00110011, 01010101.
                input_array[0] = (15u32 << 24).wrapping_add(rng.get_uint(1 << 24)) as i32;
                input_array[1] = (51u32 << 24).wrapping_add(rng.get_uint(1 << 24)) as i32;
                input_array[2] = (85u32 << 24).wrapping_add(rng.get_uint(1 << 24)) as i32;
                for v in input_array.iter_mut().skip(3) {
                    *v = rng.get_uint(1 << 24) as i32;
                }
            }
        } else {
            // All combinations of inputs are present in the key columns.
            input_array[0] = 0x0f13149f;
            input_array[1] = 0x3308e53e;
            input_array[2] = 0x556241eb;
            for i in 3..input_size {
                input_array[i] = input_array[i % 3].wrapping_shl((i / 3) as u32);
            }
        }
    }

    /// `SwapInputs(ctx, src, dest)`.
    pub fn swap_inputs(&self, src_input_array: &mut Vec<i32>, dest_input_array: &mut Vec<i32>) {
        std::mem::swap(src_input_array, dest_input_array);
    }

    /// `TestInput(result, inputs, outputs, resource_count)`: not implemented
    /// in the C++ either; always false.
    pub fn test_input(
        &self,
        _result: &mut ReactionResult,
        _inputs: &[i32],
        _outputs: &[i32],
        _resource_count: &[f64],
    ) -> bool {
        false
    }

    /// `TestOutput(ctx, result, taskctx, task_count, reaction_count,
    /// resource_count, rbins_count, is_parasite, context_phenotype)`: test
    /// every active reaction's task against the organism's output and apply
    /// the processes of those performed. Returns `result.GetActive()`.
    ///
    /// `test_mode` is `ctx.GetTestMode()`. Takes `&mut self` only because a
    /// task (`xor-max`) may repoint a reaction's resource mid-evaluation
    /// (see [`TaskEvaluator::take_resource_change`]).
    ///
    /// As in the C++, `task_count` must have an entry per task,
    /// `reaction_count` one per reaction, `resource_count` one per resource
    /// used by a process, and `result` must be sized for this environment;
    /// shorter slices panic. A missing resource bin reads as 0 (the C++
    /// reads past the array).
    ///
    /// Reaction counts are incremented as reactions fire, so a reaction
    /// earlier in the library can satisfy (or, with `noreaction`, block) the
    /// requisites of a later one within the same call.
    #[allow(clippy::too_many_arguments)]
    pub fn test_output(
        &mut self,
        rng: &mut AvidaRng,
        test_mode: bool,
        config: &Config,
        result: &mut ReactionResult,
        taskctx: &mut dyn TaskEvaluator<T>,
        task_count: &[i32],
        reaction_count: &mut [i32],
        resource_count: &[f64],
        rbins_count: &[f64],
        is_parasite: bool,
        mut context_phenotype: Option<&mut ContextPhenotype>,
    ) -> bool {
        // Flag to skip processing of parasite tasks.
        let skip_processing = is_parasite && config.parasite_skip_reactions != 0;

        taskctx.setup_tests(&self.tasklib);

        let num_reactions = self.reaction_lib.size() as usize;
        for i in 0..num_reactions {
            let (active, task) = {
                let r = self.reaction_lib.reaction(i);
                (r.active, r.task)
            };
            if !active {
                continue;
            }
            // The C++ asserts every loaded reaction has a task.
            let Some(task_id) = task else {
                continue;
            };

            taskctx.set_task_entry(&self.tasklib, task_id);
            let task_cnt = task_count[task_id as usize];
            let on_divide = taskctx.on_divide();

            if !self.test_requisites(
                &*taskctx,
                i,
                task_cnt,
                reaction_count,
                on_divide,
                is_parasite,
                config,
            ) && !skip_processing
            {
                continue;
            }

            if let Some(cp) = context_phenotype.as_deref_mut() {
                let blank_tasks = vec![0i32; task_count.len()];
                let blank_reactions = vec![0i32; num_reactions];
                cp.add_task_counts(blank_tasks.len() as i32, &blank_tasks);
                cp.add_reaction_counts(blank_reactions.len() as i32, &blank_reactions);
                let context_task_count = cp.task_counts()[task_id as usize];
                if !self.test_context_requisites(
                    i,
                    context_task_count,
                    cp.reaction_counts(),
                    on_divide,
                ) && !skip_processing
                {
                    continue;
                }
            }

            let task_quality = taskctx.test_output(&self.tasklib, rng);
            if let Some((rid, res_name)) = taskctx.take_resource_change() {
                if rid >= 0 && rid < self.reaction_lib.size() {
                    self.change_resource(rid as usize, &res_name, 0);
                }
            }

            // task_probability is in [0,1], or -1 if not needed. Some
            // phenplastbonus settings force the task to be counted even if
            // it was not demonstrated.
            let mut force_mark_task = false;
            let task_probability =
                self.get_task_probability(rng, test_mode, &mut *taskctx, i, &mut force_mark_task);

            if task_quality == 0.0 && !force_mark_task {
                continue;
            }

            result.mark_task(task_id as usize, task_quality, taskctx.task_value());

            if !skip_processing {
                let args = ProcessArgs {
                    resource_count,
                    rbins_count,
                    task_quality,
                    task_probability,
                    task_count: task_cnt,
                    reaction_id: i,
                };
                self.do_processes(rng, config, &args, result, &mut *taskctx);
                if result.reaction_triggered(i) {
                    reaction_count[i] = reaction_count[i].wrapping_add(1);
                    taskctx.set_first_reaction(i as i32);
                }
            }
        }

        result.active()
    }

    /// [`Environment::test_output`] for an environment whose evaluation
    /// changes nothing in the environment, which the parallel execution
    /// mode checks before it runs output tests in the organisms' bursts
    /// (`avida_hardware::local_output::output_is_local`): no task asks to
    /// change a reaction's resource (`xor-max`), no process uses a
    /// `phenplastbonus` method (the plasticity test runs test CPUs over the
    /// environment), and there is no context phenotype. It is the same loop
    /// on a shared borrow, so that many workers can evaluate at once.
    #[allow(clippy::too_many_arguments)]
    pub fn test_output_shared(
        &self,
        rng: &mut AvidaRng,
        config: &Config,
        result: &mut ReactionResult,
        taskctx: &mut dyn TaskEvaluator<T>,
        task_count: &[i32],
        reaction_count: &mut [i32],
        resource_count: &[f64],
        rbins_count: &[f64],
        is_parasite: bool,
    ) -> bool {
        let skip_processing = is_parasite && config.parasite_skip_reactions != 0;
        taskctx.setup_tests(&self.tasklib);
        let num_reactions = self.reaction_lib.size() as usize;
        for i in 0..num_reactions {
            let (active, task) = {
                let r = self.reaction_lib.reaction(i);
                (r.active, r.task)
            };
            if !active {
                continue;
            }
            let Some(task_id) = task else {
                continue;
            };
            taskctx.set_task_entry(&self.tasklib, task_id);
            let task_cnt = task_count[task_id as usize];
            let on_divide = taskctx.on_divide();
            if !self.test_requisites(
                &*taskctx,
                i,
                task_cnt,
                reaction_count,
                on_divide,
                is_parasite,
                config,
            ) && !skip_processing
            {
                continue;
            }
            let task_quality = taskctx.test_output(&self.tasklib, rng);
            // `get_task_probability` of an environment without a plastic
            // process: -1, nothing forced.
            if task_quality == 0.0 {
                continue;
            }
            result.mark_task(task_id as usize, task_quality, taskctx.task_value());
            if !skip_processing {
                let args = ProcessArgs {
                    resource_count,
                    rbins_count,
                    task_quality,
                    task_probability: -1.0,
                    task_count: task_cnt,
                    reaction_id: i,
                };
                self.do_processes(rng, config, &args, result, &mut *taskctx);
                if result.reaction_triggered(i) {
                    reaction_count[i] = reaction_count[i].wrapping_add(1);
                    taskctx.set_first_reaction(i as i32);
                }
            }
        }
        result.active()
    }

    /// `TestRequisites(taskctx, reaction, task_count, reaction_count,
    /// on_divide, is_parasite)`: true if any requisite of the reaction is
    /// met (or, with none, when not on divide).
    ///
    /// QUIRK: without an organism (deme-level evaluation) the cell-box
    /// check always fails, so no requisite can be met.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn test_requisites(
        &self,
        taskctx: &dyn TaskEvaluator<T>,
        reaction_id: usize,
        task_count: i32,
        reaction_count: &[i32],
        on_divide: bool,
        is_parasite: bool,
        config: &Config,
    ) -> bool {
        let req_list = &self.reaction_lib.reaction(reaction_id).requisites;
        if req_list.is_empty() {
            // On divide the default is not to check the task.
            return !on_divide;
        }

        for cur_req in req_list {
            let mut satisfied = true;

            // Have all reactions been met?
            if taskctx.has_organism() {
                for &react_id in &cur_req.prior_reactions {
                    if reaction_count[react_id] == 0 && taskctx.stolen_reaction_count(react_id) == 0
                    {
                        satisfied = false;
                        break;
                    }
                }
            } else {
                // Called as a deme reaction.
                for &react_id in &cur_req.prior_reactions {
                    if reaction_count[react_id] == 0 {
                        satisfied = false;
                        break;
                    }
                }
            }
            if !satisfied {
                continue;
            }

            // Have all no-reactions been met?
            for &react_id in &cur_req.prior_noreactions {
                if reaction_count[react_id] != 0 {
                    satisfied = false;
                    break;
                }
            }
            if !satisfied {
                continue;
            }

            // Task counts.
            if task_count < cur_req.min_task_count {
                continue;
            }
            if task_count >= cur_req.max_task_count {
                continue;
            }

            // Reaction counts.
            if reaction_count[reaction_id] < cur_req.min_reaction_count {
                continue;
            }
            if reaction_count[reaction_id] >= cur_req.max_reaction_count {
                continue;
            }

            // Total reaction counts.
            let tot_reactions = reaction_count.iter().fold(0i32, |a, &b| a.wrapping_add(b));
            if tot_reactions < cur_req.min_tot_reaction_count {
                continue;
            }
            if tot_reactions >= cur_req.max_tot_reaction_count {
                continue;
            }

            // Divide requisites: 0 = only on IO, 1 = only on divide, 2 = always.
            let div_type = cur_req.divide_only;
            if div_type == 1 && !on_divide {
                continue;
            }
            if div_type == 0 && on_divide {
                continue;
            }

            if cur_req.parasite_only != 0 && !is_parasite {
                continue;
            }

            if !taskctx.has_organism()
                || !cur_req
                    .cell_box
                    .in_cell_box(taskctx.cell_id(), config.world_x, config.world_y)
            {
                continue;
            }

            return true;
        }
        false
    }

    /// `TestContextRequisites(reaction, task_count, reaction_count,
    /// on_divide)`: as `TestRequisites` against context counts, without the
    /// stolen-reaction, parasite and cell-box checks.
    pub(crate) fn test_context_requisites(
        &self,
        reaction_id: usize,
        task_count: i32,
        reaction_count: &[i32],
        on_divide: bool,
    ) -> bool {
        let req_list = &self.reaction_lib.reaction(reaction_id).context_requisites;
        if req_list.is_empty() {
            return !on_divide;
        }

        for cur_req in req_list {
            let mut satisfied = true;
            for &react_id in &cur_req.prior_reactions {
                if reaction_count[react_id] == 0 {
                    satisfied = false;
                    break;
                }
            }
            if !satisfied {
                continue;
            }
            for &react_id in &cur_req.prior_noreactions {
                if reaction_count[react_id] != 0 {
                    satisfied = false;
                    break;
                }
            }
            if !satisfied {
                continue;
            }

            if task_count < cur_req.min_task_count {
                continue;
            }
            if task_count >= cur_req.max_task_count {
                continue;
            }
            if reaction_count[reaction_id] < cur_req.min_reaction_count {
                continue;
            }
            if reaction_count[reaction_id] >= cur_req.max_reaction_count {
                continue;
            }
            let tot_reactions = reaction_count.iter().fold(0i32, |a, &b| a.wrapping_add(b));
            if tot_reactions < cur_req.min_tot_reaction_count {
                continue;
            }
            if tot_reactions >= cur_req.max_tot_reaction_count {
                continue;
            }
            let div_type = cur_req.divide_only;
            if div_type == 1 && !on_divide {
                continue;
            }
            if div_type == 0 && on_divide {
                continue;
            }
            return true;
        }
        false
    }

    /// Whether any process of any reaction sets a `phenplastbonus` method
    /// other than `default`, so that `GetTaskProbability` asks the
    /// plasticity of the organism's genotype.
    pub fn uses_phenplast_bonus(&self) -> bool {
        (0..self.reaction_lib.size().max(0) as usize).any(|i| {
            self.reaction_lib
                .reaction(i)
                .processes
                .iter()
                .any(|p| p.pp_method != PhenPlastBonusMethod::Default)
        })
    }

    /// `GetTaskProbability(ctx, taskctx, processes, force_mark_task)`: the
    /// plastic task probability when any process uses a non-default
    /// phenplastbonus method, else -1 (always -1 in test mode).
    ///
    /// Takes `&mut self` to lend the environment to the evaluator: the
    /// C++ runs test CPUs from inside this call (see
    /// [`TaskEvaluator::task_probability`]).
    fn get_task_probability(
        &mut self,
        rng: &mut AvidaRng,
        test_mode: bool,
        taskctx: &mut dyn TaskEvaluator<T>,
        reaction_id: usize,
        force_mark_task: &mut bool,
    ) -> f64 {
        *force_mark_task = false;
        if test_mode {
            return -1.0;
        }
        let reaction = self.reaction_lib.reaction(reaction_id);
        let mut task_prob = -1.0;
        let mut test_plasticity = false;
        for p in &reaction.processes {
            let pp = p.pp_method;
            if pp != PhenPlastBonusMethod::Default {
                test_plasticity = true;
                if pp == PhenPlastBonusMethod::FullBonus || pp == PhenPlastBonusMethod::FracBonus {
                    *force_mark_task = true;
                }
            }
        }
        if test_plasticity {
            // taskctx.GetTaskEntry()->GetID() is the reaction's task.
            let task_id = reaction.task.unwrap_or(-1);
            task_prob = taskctx.task_probability(self, rng, task_id);
        }
        // An undemonstrated task need not be marked.
        *force_mark_task = *force_mark_task && task_prob > 0.0;
        task_prob
    }

    /// `DoProcesses(ctx, process_list, resource_count, rbins_count,
    /// task_quality, task_probability, task_count, reaction_id, result,
    /// taskctx)` for the processes of reaction `args.reaction_id`.
    pub(crate) fn do_processes(
        &self,
        rng: &mut AvidaRng,
        config: &Config,
        args: &ProcessArgs<'_>,
        result: &mut ReactionResult,
        taskctx: &mut dyn TaskEvaluator<T>,
    ) {
        let reaction_id = args.reaction_id;
        let resource_count = args.resource_count;
        let rbins_count = args.rbins_count;
        let task_probability = args.task_probability;
        let task_count = args.task_count;

        for cur_process in &self.reaction_lib.reaction(reaction_id).processes {
            let max_consumed = cur_process.max_number;
            let min_consumed = cur_process.min_number;

            let pp_meth = cur_process.pp_method;
            let task_plasticity_modifier =
                if pp_meth == PhenPlastBonusMethod::NoBonus && task_probability != 1.0 {
                    0.0
                } else if pp_meth == PhenPlastBonusMethod::FracBonus {
                    task_probability
                } else {
                    1.0
                };

            // Phenplast full bonus uses a task quality of 1.0.
            let local_task_quality = if pp_meth == PhenPlastBonusMethod::FullBonus
                || pp_meth == PhenPlastBonusMethod::FracBonus
            {
                1.0
            } else {
                args.task_quality
            };

            // Determine resource consumption.
            let mut consumed: f64 = 0.0;
            let in_resource = cur_process.resource;

            match in_resource {
                None => {
                    // Infinite resource.
                    consumed = max_consumed * local_task_quality * task_plasticity_modifier;
                }
                Some(res_id) if self.resource_lib.resource(res_id).hgt_metabolize => {
                    // HGT metabolism: consume one genome fragment from the
                    // organism's cell; the amount consumed is its length.
                    if let Some(len) = taskctx.metabolize_genome_fragment(rng) {
                        consumed = local_task_quality * len as f64;
                        result.consume(res_id, len as f64, true);
                    }
                    // If we can't metabolize a fragment, stop here.
                    if consumed == 0.0 {
                        continue;
                    }
                }
                Some(res_id) => {
                    // A finite resource. Was it set to 0 for this cell?
                    if resource_count[res_id] == 0.0 {
                        consumed = 0.0;
                    } else {
                        consumed = resource_count[res_id] - result.consumed(res_id);
                        consumed *= cur_process.max_fraction;
                    }

                    let may_use_rbins = config.use_resource_bins;
                    let mut using_rbins = false;
                    // Indexing an absent bin is undefined in C++; 0 here.
                    let rbin = rbins_count.get(res_id).copied().unwrap_or(0.0);

                    if cur_process.internal {
                        consumed = rbin;
                        using_rbins = true;
                    } else if may_use_rbins
                        && rbin > 0.0
                        && (config.use_stored_fraction * rbin)
                            > (config.env_fraction_threshold * consumed)
                    {
                        // Use the bin when its usable fraction beats the
                        // threshold fraction of the environment's offer.
                        consumed = config.use_stored_fraction * rbin;
                        using_rbins = true;
                    }

                    // Not above the maximum consumption.
                    if consumed > max_consumed {
                        consumed = max_consumed;
                    }

                    // Modify by task quality and plasticity.
                    consumed = consumed * local_task_quality * task_plasticity_modifier;

                    // Below the minimum consumption?
                    if consumed < min_consumed {
                        consumed = 0.0;
                    }

                    // Nothing to consume: stop here.
                    if consumed == 0.0 {
                        continue;
                    }

                    // Can't consume more than what's available.
                    if !using_rbins {
                        consumed = consumed.min(resource_count[res_id]);
                    } else {
                        consumed = consumed.min(rbin);
                    }

                    if cur_process.depletable {
                        result.consume(res_id, consumed, !using_rbins);
                    }
                }
            }

            // The reaction is performed if we get here.
            result.mark_reaction(reaction_id);

            let mut bonus = consumed * cur_process.value;

            if !cur_process.is_germline {
                // How much of the bonus belongs to the deme?
                let mut deme_bonus = 0.0;
                if cur_process.deme_fraction != 0.0 {
                    deme_bonus = cur_process.deme_fraction * bonus;
                    // `(1 - f) * bonus`; IEEE multiplication commutes.
                    bonus *= 1.0 - cur_process.deme_fraction;
                }

                match cur_process.proc_type {
                    proc_type::ADD => {
                        result.add_bonus(bonus, reaction_id);
                        result.add_deme_bonus(deme_bonus);
                    }
                    proc_type::MULT => {
                        result.mult_bonus(bonus);
                        // deme_bonus is zero unless this is a deme reaction.
                        if cur_process.deme_fraction != 0.0 {
                            result.mult_deme_bonus(deme_bonus);
                        }
                    }
                    proc_type::POW => {
                        result.mult_bonus(2.0f64.powf(bonus));
                        result.mult_deme_bonus(2.0f64.powf(deme_bonus));
                    }
                    proc_type::LIN => {
                        result.add_bonus(bonus * task_count as f64, reaction_id);
                    }
                    proc_type::ENERGY => {
                        result.add_energy(bonus);
                    }
                    proc_type::ENZYME => {
                        // QUIRK: the C++ dereferences a missing resource
                        // here; without one no bonus is given.
                        if let Some(res_id) = in_resource {
                            let rc = resource_count[res_id];
                            let reward = cur_process.value * rc / (rc + cur_process.k_sub_m);
                            result.add_bonus(reward, reaction_id);
                        }
                    }
                    proc_type::EXP => {
                        // Cumulative reward is value * integral of
                        // exp(-decay * task_count); add one step's worth.
                        let decay = cur_process.max_fraction;
                        let value = cur_process.value;
                        let step = ((task_count.wrapping_sub(1)) as f64 * decay).exp()
                            - (task_count as f64 * decay).exp();
                        result.add_bonus(value * (1.0 / decay) * step, reaction_id);
                    }
                    _ => {}
                }
            } else {
                // Germline propensity instead of bonus.
                match cur_process.proc_type {
                    proc_type::ADD => result.add_germline(bonus),
                    proc_type::MULT => result.mult_germline(bonus),
                    proc_type::POW => result.mult_germline(2.0f64.powf(bonus)),
                    _ => {}
                }
            }

            // Detection events.
            if let Some(detected_id) = cur_process.detect {
                let real_amount = resource_count[detected_id];
                let estimated_amount = rng
                    .get_rand_normal_params(real_amount, cur_process.detection_error * real_amount);
                if estimated_amount < cur_process.detection_threshold {
                    result.detect(detected_id, 0.0);
                } else {
                    result.detect(detected_id, estimated_amount);
                }
            }

            // Byproducts.
            if let Some(product_id) = cur_process.product {
                let product_size = consumed * cur_process.conversion;
                result.produce(product_id, product_size, !cur_process.internal);
            }

            // Instructions to run.
            if !cur_process.inst.is_empty() {
                result.add_inst(&cur_process.inst);
            }

            let prob_lethal = cur_process.lethal;
            let mut lethal = false;
            if prob_lethal != 0.0 && prob_lethal != 1.0 {
                let x = rng.get_double();
                if x < prob_lethal {
                    lethal = true;
                }
            } else {
                lethal = prob_lethal != 0.0;
            }

            result.set_random_resource(cur_process.is_random_resource);
            result.set_lethal(lethal);
            result.set_sterilize(cur_process.sterilize);
        }
    }
}
