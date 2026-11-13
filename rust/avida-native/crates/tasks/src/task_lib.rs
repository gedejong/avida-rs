//! Port of `cTaskLib` and `cTaskEntry` (avida-core/source/main/cTaskLib.{h,cc},
//! cTaskEntry.h): the registry of tasks an environment can name, `SetupTests`
//! and `TestOutput`.

use crate::arg_schema::{ArgContainer, ArgSchema, ArgType, Feedback};
use crate::context::{TaskContext, TaskWorld};
use crate::env_reqs::EnvReqs;
use crate::eval_logic::{compute_logic_id, eval_logic, eval_logic3, LogicOp};
use crate::registry_table::{
    TaskKind, UnaryOp, REQ_NEIGHBOR_INPUT, REQ_NEIGHBOR_OUTPUT, SIMPLE_TASKS,
};
use crate::{eval_math, eval_misc, eval_world};

/// `cTaskEntry`: one registered task.
#[derive(Debug, Clone)]
pub struct TaskEntry {
    name: String,
    desc: String,
    id: usize,
    kind: TaskKind,
    args: Option<ArgContainer>,
    prop_id_ave: String,
    prop_id_count: String,
}

impl TaskEntry {
    /// `cTaskEntry(name, desc, in_id, fun, args)`.
    fn new(name: &str, desc: &str, id: usize, kind: TaskKind, args: Option<ArgContainer>) -> Self {
        TaskEntry {
            name: name.to_string(),
            desc: desc.to_string(),
            id,
            kind,
            args,
            prop_id_ave: format!("environment.triggers.{}.average", name),
            prop_id_count: format!("environment.triggers.{}.count", name),
        }
    }

    /// `GetName()`: short keyword for the task.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// `GetDesc()`: human-readable description.
    pub fn desc(&self) -> &str {
        &self.desc
    }
    /// `GetID()`: index in registration order.
    pub fn id(&self) -> usize {
        self.id
    }
    /// `GetTestFun()`: which evaluator runs.
    pub fn kind(&self) -> TaskKind {
        self.kind
    }
    /// `AveragePropertyID()`.
    pub fn average_property_id(&self) -> &str {
        &self.prop_id_ave
    }
    /// `CountPropertyID()`.
    pub fn count_property_id(&self) -> &str {
        &self.prop_id_count
    }
    /// `HasArguments()`.
    pub fn has_arguments(&self) -> bool {
        self.args.is_some()
    }
    /// `GetArguments()` (`None` where the C++ would dereference `NULL`).
    pub fn arguments(&self) -> Option<&ArgContainer> {
        self.args.as_ref()
    }
}

/// `cTaskLib`.
#[derive(Debug, Clone, Default)]
pub struct TaskLib {
    task_array: Vec<TaskEntry>,
    use_neighbor_input: bool,
    use_neighbor_output: bool,
    /// `m_strings`: patterns of the loaded `matchprodstr` tasks.
    m_strings: Vec<String>,
    /// Group ids the C++ passes to `cEnvironment::AddGroupID` while loading
    /// (`form-group-id`, `live-on-patch-id`), in call order. The environment
    /// layer should drain these after `add_task`.
    group_ids: Vec<i32>,
    /// Target ids passed to `cEnvironment::AddTargetID` (`eat-target*`, `move-ft`).
    target_ids: Vec<i32>,
    /// Stand-in arguments for an entry without any (never reached for
    /// registered tasks that read arguments).
    empty_args: ArgContainer,
}

impl TaskLib {
    /// `cTaskLib(cWorld*)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `GetSize()`.
    pub fn get_size(&self) -> usize {
        self.task_array.len()
    }

    /// `GetTask(id)` / `GetTaskReference(id)`.
    pub fn get_task(&self, id: usize) -> Option<&TaskEntry> {
        self.task_array.get(id)
    }

    /// All entries in registration order.
    pub fn tasks(&self) -> &[TaskEntry] {
        &self.task_array
    }

    /// `GetTaskReference(id).GetArguments()` for writing (`SetTaskArg*`
    /// actions); `None` for an unknown id or a task without arguments.
    pub fn task_args_mut(&mut self, id: usize) -> Option<&mut ArgContainer> {
        self.task_array.get_mut(id)?.args.as_mut()
    }

    /// `UseNeighborInput()`.
    pub fn use_neighbor_input(&self) -> bool {
        self.use_neighbor_input
    }
    /// `UseNeighborOutput()`.
    pub fn use_neighbor_output(&self) -> bool {
        self.use_neighbor_output
    }

    /// `GetMatchStrings()`.
    pub fn get_match_strings(&self) -> Vec<String> {
        self.m_strings.clone()
    }
    /// `GetMatchString(int x)`: empty when out of range.
    pub fn get_match_string(&self, x: i32) -> String {
        if x >= 0 && (x as usize) < self.m_strings.len() {
            self.m_strings[x as usize].clone()
        } else {
            String::new()
        }
    }
    /// `GetNumberOfMatchStrings()`.
    pub fn get_number_of_match_strings(&self) -> usize {
        self.m_strings.len()
    }

    /// Group ids registered with the environment by tasks loaded so far.
    pub fn group_ids(&self) -> &[i32] {
        &self.group_ids
    }
    /// Take (and clear) the pending `AddGroupID` calls.
    pub fn take_group_ids(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.group_ids)
    }
    /// Target ids registered with the environment by tasks loaded so far.
    pub fn target_ids(&self) -> &[i32] {
        &self.target_ids
    }
    /// Take (and clear) the pending `AddTargetID` calls.
    pub fn take_target_ids(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.target_ids)
    }

    /// `NewTask`: append an entry; ids follow registration order.
    fn new_task(
        &mut self,
        name: &str,
        desc: &str,
        kind: TaskKind,
        reqs: u32,
        args: Option<ArgContainer>,
    ) {
        if reqs & REQ_NEIGHBOR_INPUT != 0 {
            self.use_neighbor_input = true;
        }
        if reqs & REQ_NEIGHBOR_OUTPUT != 0 {
            self.use_neighbor_output = true;
        }
        let id = self.task_array.len();
        self.task_array
            .push(TaskEntry::new(name, desc, id, kind, args));
    }

    /// Whether `entry` names an `optimize` task.
    fn is_optimize(&self, entry: Option<usize>) -> bool {
        entry
            .and_then(|id| self.task_array.get(id))
            .is_some_and(|t| matches!(t.kind, TaskKind::Optimize))
    }

    /// `cTaskLib::AddTask(name, info, envreqs, feedback)`: register the task
    /// called `name` (arguments in `info`) and return its id, or `None` after
    /// reporting through `feedback`.
    ///
    /// A task without arguments that is already registered is returned as is;
    /// tasks with arguments always get a new entry.
    ///
    /// QUIRK: for the `Load_*` functions that dereference the result of
    /// `cArgContainer::Load` before testing it for `NULL` (`matchstr`,
    /// `matchprodstr`, `all-ones`, `royal-road`, `royal-road-wd`, `form-group-id`,
    /// `live-on-patch-id`, `eat-target*`, `move-ft`) the C++ crashes when the
    /// argument string is invalid. Here the load error is reported and the
    /// final "unknown/unprocessed task entry" error follows as for the other
    /// families.
    pub fn add_task(
        &mut self,
        name: &str,
        info: &str,
        envreqs: &mut EnvReqs,
        feedback: &mut Feedback,
    ) -> Option<usize> {
        // Determine if this task is already in the active library.
        for t in &self.task_array {
            if t.name == name && !t.has_arguments() {
                return Some(t.id);
            }
        }

        // Match up this name to its corresponding task
        let start_size = self.task_array.len();
        let errors_before = feedback.num_errors();

        if let Some(st) = SIMPLE_TASKS.iter().find(|t| t.name == name) {
            self.new_task(name, st.desc, st.kind, st.reqs, None);
        } else {
            self.load_task(name, info, envreqs, feedback);
        }

        // Make sure we have actually found a task
        if self.task_array.len() == start_size {
            // A task the port refuses (`unsupported:`) needs no second error.
            let refused = feedback
                .errors()
                .iter()
                .skip(errors_before)
                .any(|e| e.starts_with("unsupported:"));
            if !refused {
                feedback.error(format!("unknown/unprocessed task entry '{}'", name));
            }
            return None;
        }

        // And return the found task.
        Some(start_size)
    }

    /// The `Load_*` branch of `AddTask`: tasks that parse arguments.
    fn load_task(
        &mut self,
        name: &str,
        info: &str,
        envreqs: &mut EnvReqs,
        feedback: &mut Feedback,
    ) {
        match name {
            // Matching + sequence tasks
            "matchstr" => {
                let mut schema = ArgSchema::new();
                schema.add_required("string", 0, ArgType::Str);
                schema.add_int("partial", 0, 0);
                schema.add_int("binary", 1, 1);
                schema.add_double("pow", 0, 2.0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_outputs(args.get_string(0).len() as i32);
                    self.new_task(name, "MatchStr", TaskKind::MatchStr, 0, Some(args));
                }
            }
            "matchprodstr" => {
                let mut schema = ArgSchema::new();
                schema.add_required("string", 0, ArgType::Str);
                schema.add_int("partial", 0, 0);
                schema.add_int("binary", 1, 1);
                schema.add_double("pow", 0, 2.0);
                schema.add_int("tag", 2, -1);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_outputs(args.get_string(0).len() as i32);
                    self.m_strings.push(args.get_string(0).to_string());
                    // QUIRK: registers Task_MatchStr, not Task_MatchProdStr.
                    self.new_task(name, "MatchProdStr", TaskKind::MatchStr, 0, Some(args));
                }
            }
            "match_number" => {
                let mut schema = ArgSchema::new();
                schema.add_required("target", 0, ArgType::Int);
                schema.add_int("threshold", 1, -1);
                schema.add_required("halflife", 0, ArgType::Double);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(name, "Match Number", TaskKind::MatchNumber, 0, Some(args));
                }
            }
            "sort_inputs" => {
                let mut schema = ArgSchema::new();
                schema.add_required("size", 0, ArgType::Int); // Number of items to sort
                schema.add_int("direction", 1, 0); // < 0 = Descending, Otherwise = Ascending
                schema.add_int("contiguous", 2, 1); // 0 = No, Otherwise = Yes
                schema.add_required("halflife", 0, ArgType::Double);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_inputs(args.get_int(0));
                    envreqs.set_min_outputs(args.get_int(0).wrapping_mul(2));
                    envreqs.set_true_rand_inputs(true);
                    self.new_task(name, "Sort Inputs", TaskKind::SortInputs, 0, Some(args));
                }
            }
            "fibonacci_seq" => {
                let mut schema = ArgSchema::new();
                schema.add_required("target", 0, ArgType::Int);
                schema.add_double("penalty", 0, 0.0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(
                        name,
                        "Fibonacci Sequence",
                        TaskKind::FibonacciSequence,
                        0,
                        Some(args),
                    );
                }
            }

            // Math tasks
            "mult" => {
                self.load_threshold_task(name, info, feedback, "Multiplication", TaskKind::Mult)
            }
            "div" => self.load_threshold_task(name, info, feedback, "Division", TaskKind::Div),
            "log" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Logarithm (natural)",
                TaskKind::Unary(UnaryOp::Log),
            ),
            "log2" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Logarithm (base-2)",
                TaskKind::Unary(UnaryOp::Log2),
            ),
            "log10" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Logarithm (base-10)",
                TaskKind::Unary(UnaryOp::Log10),
            ),
            "sqrt" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Square Root",
                TaskKind::Unary(UnaryOp::Sqrt),
            ),
            "sine" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Sine",
                TaskKind::Unary(UnaryOp::Sine),
            ),
            "cosine" => self.load_threshold_task(
                name,
                info,
                feedback,
                "Cosine",
                TaskKind::Unary(UnaryOp::Cosine),
            ),

            // Optimization tasks
            "optimize" => {
                let mut schema = ArgSchema::new();
                schema.add_required("function", 0, ArgType::Int);
                schema.add_int("binary", 1, 0);
                schema.add_int("varlength", 2, 8);
                schema.add_int("numvars", 3, 2);
                schema.add_double("basepow", 0, 2.0);
                schema.add_double("maxFx", 1, 1.0);
                schema.add_double("minFx", 2, 0.0);
                schema.add_double("thresh", 3, -1.0);
                schema.add_double("threshMax", 4, -1.0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    // Arguments the reference binary crashes, aborts or hangs on
                    // when it evaluates the task.
                    if let Some(msg) = eval_misc::optimize_unsupported(&args) {
                        feedback.error(msg);
                        return;
                    }
                    if args.get_int(1) != 0 {
                        envreqs.set_min_outputs(args.get_int(2).wrapping_mul(args.get_int(3)));
                    } else {
                        // once have ability to change args should in each of these cases change
                        // the max/min to the appropriate defaults for this function
                        match args.get_int(0) {
                            1 => envreqs.set_min_outputs(1),
                            2 => envreqs.set_min_outputs(2),
                            3 => envreqs.set_min_outputs(2),
                            _ => envreqs.set_min_outputs(2),
                        }
                    }
                    self.new_task(name, "Optimize", TaskKind::Optimize, 0, Some(args));
                }
            }

            // State grid
            "sg_path_traversal" => {
                let mut schema = ArgSchema::new();
                schema.add_required("pathlen", 0, ArgType::Int);
                schema.add_required("sgname", 0, ArgType::Str);
                schema.add_required("poison", 1, ArgType::Str);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(
                        name,
                        "State Grid Path Traversal",
                        TaskKind::SGPathTraversal,
                        0,
                        Some(args),
                    );
                }
            }

            // Group formation
            "form-group" => {
                let mut schema = ArgSchema::new();
                schema.add_int("group_size", 0, 1);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(
                        name,
                        "FormSpatialGroups",
                        TaskKind::FormSpatialGroup,
                        0,
                        Some(args),
                    );
                }
            }
            "form-group-id" => {
                let mut schema = ArgSchema::new();
                schema.add_int("group_size", 0, 1);
                schema.add_int("group_id", 1, 1);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    let group_id = args.get_int(1);
                    self.new_task(
                        name,
                        "FormSpatialGroupWithID",
                        TaskKind::FormSpatialGroupWithID,
                        0,
                        Some(args),
                    );
                    // Add this group id to the list in the instructions file.
                    self.group_ids.push(group_id);
                }
            }
            "live-on-patch-id" => {
                let mut schema = ArgSchema::new();
                schema.add_int("patch_id", 0, 1);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    let patch_id = args.get_int(0);
                    self.new_task(
                        name,
                        "LiveOnPatchRes",
                        TaskKind::LiveOnPatchRes,
                        0,
                        Some(args),
                    );
                    // Add this patch id to the list in the instructions file.
                    self.group_ids.push(patch_id);
                }
            }
            "collect-odd-cell" => {
                let mut schema = ArgSchema::new();
                schema.add_int("even_or_odd", 0, 0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(
                        name,
                        "collect-odd-cell",
                        TaskKind::CollectOddCell,
                        0,
                        Some(args),
                    );
                }
            }

            // Feed specific tasks
            "eat-target" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTarget",
                TaskKind::ConsumeTarget,
            ),
            "eat-target-echo" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetEcho",
                TaskKind::ConsumeTargetEcho,
            ),
            "eat-target-nand" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetNand",
                TaskKind::ConsumeTargetLogic(LogicOp::Nand),
            ),
            "eat-target-and" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetAnd",
                TaskKind::ConsumeTargetLogic(LogicOp::And),
            ),
            "eat-target-orn" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetOrn",
                TaskKind::ConsumeTargetLogic(LogicOp::OrNot),
            ),
            "eat-target-or" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetOr",
                TaskKind::ConsumeTargetLogic(LogicOp::Or),
            ),
            "eat-target-andn" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetAndn",
                TaskKind::ConsumeTargetLogic(LogicOp::AndNot),
            ),
            "eat-target-nor" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetNor",
                TaskKind::ConsumeTargetLogic(LogicOp::Nor),
            ),
            "eat-target-xor" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetXor",
                TaskKind::ConsumeTargetLogic(LogicOp::Xor),
            ),
            "eat-target-equ" => self.load_target_task(
                name,
                info,
                feedback,
                "ConsumeTargetEqu",
                TaskKind::ConsumeTargetLogic(LogicOp::Equ),
            ),
            "move-ft" => self.load_target_task(name, info, feedback, "MoveFT", TaskKind::MoveFT),

            // String matching
            "all-ones" => {
                let mut schema = ArgSchema::new();
                schema.add_int("length", 0, 0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_outputs(args.get_int(0));
                    self.new_task(name, "all-ones", TaskKind::AllOnes, 0, Some(args));
                }
            }
            "royal-road" => {
                let mut schema = ArgSchema::new();
                schema.add_int("length", 0, 0);
                schema.add_int("block_count", 1, 0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_outputs(args.get_int(0));
                    self.new_task(name, "royal-road", TaskKind::RoyalRoad, 0, Some(args));
                }
            }
            "royal-road-wd" => {
                let mut schema = ArgSchema::new();
                schema.add_int("length", 0, 0);
                schema.add_int("block_count", 1, 0);
                schema.add_int("width", 2, 0);
                schema.add_int("height", 3, 0);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    envreqs.set_min_outputs(args.get_int(0));
                    self.new_task(
                        name,
                        "royal-road-wd",
                        TaskKind::RoyalRoadWithDitches,
                        0,
                        Some(args),
                    );
                }
            }

            // Division of labor
            "opinion_is" => {
                let mut schema = ArgSchema::new();
                schema.add_required("opinion", 0, ArgType::Int);
                if let Some(args) = ArgContainer::load(info, &schema, feedback) {
                    self.new_task(
                        name,
                        "Whether organism's opinion is set to value.",
                        TaskKind::OpinionIs,
                        0,
                        Some(args),
                    );
                }
            }
            _ => {}
        }
    }

    /// `Load_Mult`, `Load_Div`, `Load_Log*`, `Load_Sqrt`, `Load_Sine`,
    /// `Load_Cosine`: `threshold` (default -1) and a required `halflife`.
    fn load_threshold_task(
        &mut self,
        name: &str,
        info: &str,
        feedback: &mut Feedback,
        desc: &str,
        kind: TaskKind,
    ) {
        let mut schema = ArgSchema::new();
        schema.add_int("threshold", 0, -1);
        schema.add_required("halflife", 0, ArgType::Double);
        if let Some(args) = ArgContainer::load(info, &schema, feedback) {
            self.new_task(name, desc, kind, 0, Some(args));
        }
    }

    /// `Load_ConsumeTarget*` and `Load_MoveFT`: `target_id` (default 1),
    /// registered with the environment afterwards.
    fn load_target_task(
        &mut self,
        name: &str,
        info: &str,
        feedback: &mut Feedback,
        desc: &str,
        kind: TaskKind,
    ) {
        let mut schema = ArgSchema::new();
        schema.add_int("target_id", 0, 1);
        if let Some(args) = ArgContainer::load(info, &schema, feedback) {
            let target_id = args.get_int(0);
            self.new_task(name, desc, kind, 0, Some(args));
            // Add this target id to the list in the instructions file.
            self.target_ids.push(target_id);
        }
    }

    /// `cTaskLib::SetupTests(ctx)`: compute and store the context's logic id.
    pub fn setup_tests(&self, ctx: &mut TaskContext) {
        ctx.logic_id = compute_logic_id(ctx.input_buffer, ctx.output_buffer);
    }

    /// Evaluate the task selected by `ctx.task_entry` when it needs nothing
    /// from the organism or world (logic, math, match, sort-free, optimize,
    /// string tasks); `None` when the task needs a [`TaskWorld`].
    ///
    /// This is the fast path: no trait calls, no allocation for the logic and
    /// math tasks.
    pub fn test_output_pure(&self, ctx: &mut TaskContext) -> Option<f64> {
        let id = ctx.task_entry?;
        let entry = self.task_array.get(id)?;
        let args = entry.args.as_ref().unwrap_or(&self.empty_args);
        let (input, output) = (ctx.input_buffer, ctx.output_buffer);
        let v = match entry.kind {
            TaskKind::Echo => eval_misc::eval_echo(input, output),
            TaskKind::Add => eval_math::eval_add(input, output),
            TaskKind::Add3 => eval_math::eval_add3(input, output),
            TaskKind::Sub => eval_math::eval_sub(input, output),
            TaskKind::AddLowBits => eval_math::eval_add_lowbits(input, output),
            TaskKind::AddMiss => eval_math::eval_add_miss(input, output),
            TaskKind::AddLowBits4 => eval_math::eval_add_lowbits4(input, output),
            TaskKind::AddMiss4 => eval_math::eval_add_miss4(input, output),
            TaskKind::IncLowBits => eval_math::eval_inc_lowbits(input, output),
            TaskKind::DecLowBits => eval_math::eval_dec_lowbits(input, output),
            TaskKind::ArithMiss => eval_math::eval_arith_miss(input, output),
            TaskKind::DontCare => eval_misc::eval_dont_care(),
            TaskKind::Logic(op) => eval_logic(op, ctx.logic_id),
            TaskKind::Logic3(targets) => eval_logic3(targets, ctx.logic_id),
            TaskKind::Math1(op) => eval_math::eval_math1(op, input, output),
            TaskKind::Math2(op) => eval_math::eval_math2(op, input, output),
            TaskKind::Math3(op) => eval_math::eval_math3(op, input, output),
            TaskKind::Fib(idx) => eval_math::eval_fib(idx, input, output),
            TaskKind::MatchStr => eval_misc::eval_match_str(args, output, ctx.received_messages),
            TaskKind::MatchNumber => eval_math::eval_match_number(args, output),
            TaskKind::FibonacciSequence => eval_misc::eval_fibonacci_sequence(ctx, args),
            TaskKind::Mult => eval_math::eval_mult(args, input, output),
            TaskKind::Div => eval_math::eval_div(args, input, output),
            TaskKind::Unary(op) => eval_math::eval_unary(op, args, input, output),
            TaskKind::Optimize => {
                let r = eval_misc::eval_optimize(args, output);
                if let Some(fx) = r.fx {
                    ctx.task_value = fx;
                }
                r.quality
            }
            TaskKind::CommEcho => eval_misc::eval_comm_echo(ctx),
            TaskKind::CommNot => eval_misc::eval_comm_not(ctx),
            TaskKind::AllOnes => eval_misc::eval_all_ones(args, output),
            TaskKind::RoyalRoad => eval_misc::eval_royal_road(args, output),
            TaskKind::RoyalRoadWithDitches => eval_misc::eval_royal_road_wd(args, output),
            _ => return None,
        };
        Some(v)
    }

    /// `cTaskLib::TestOutput(ctx)`: evaluate the task selected by
    /// `ctx.task_entry` (set with [`TaskContext::set_task_entry`]); returns the
    /// quality (usually 0.0 to 1.0). `world` serves the organism / population
    /// queries of the tasks that need them. An unset or unknown task entry
    /// (a `NULL` dereference in C++) evaluates to 0.0.
    pub fn test_output(&self, ctx: &mut TaskContext, world: &mut dyn TaskWorld) -> f64 {
        if let Some(v) = self.test_output_pure(ctx) {
            // `Task_Optimize` reports a quality above 1 on standard output.
            if v > 1.0 && self.is_optimize(ctx.task_entry) {
                world.task_stdout(&eval_misc::optimize_quality_note(v, ctx.task_value));
            }
            return v;
        }
        let Some(id) = ctx.task_entry else {
            return 0.0;
        };
        let Some(entry) = self.task_array.get(id) else {
            return 0.0;
        };
        let args = entry.args.as_ref().unwrap_or(&self.empty_args);
        let ctx_ro: &TaskContext = ctx;
        match entry.kind {
            TaskKind::NandResDep => eval_world::eval_nand_res_dep(ctx_ro.logic_id, world),
            TaskKind::NorResDep => eval_world::eval_nor_res_dep(ctx_ro.logic_id, world),
            TaskKind::XorMax => eval_world::eval_xor_max(ctx_ro, world),
            TaskKind::SortInputs => eval_world::eval_sort_inputs(args, ctx_ro.output_buffer, world),
            TaskKind::MatchProdStr => {
                eval_world::eval_match_prod_str(args, ctx_ro.output_buffer, world)
            }
            TaskKind::MoveUpGradient => eval_world::eval_move_up_gradient(world),
            TaskKind::MoveNeutralGradient => eval_world::eval_move_neutral_gradient(world),
            TaskKind::MoveDownGradient => eval_world::eval_move_down_gradient(world),
            TaskKind::MoveNotUpGradient => eval_world::eval_move_not_up_gradient(world),
            TaskKind::MoveToRightSide => eval_world::eval_move_to_right_side(world),
            TaskKind::MoveToLeftSide => eval_world::eval_move_to_left_side(world),
            TaskKind::Move => eval_world::eval_move(world),
            TaskKind::MoveToTarget => eval_world::eval_move_to_target(world),
            TaskKind::MoveToMovementEvent => eval_world::eval_move_to_movement_event(world),
            TaskKind::MoveBetweenMovementEvent => {
                eval_world::eval_move_between_movement_event(world)
            }
            TaskKind::MoveToEvent => eval_world::eval_move_to_event(world),
            TaskKind::EventKilled => eval_world::eval_event_killed(world),
            TaskKind::SGPathTraversal => eval_world::eval_sg_path_traversal(args, ctx_ro, world),
            TaskKind::CreatePerfectStrings => {
                eval_world::eval_create_perfect_strings(self.m_strings.len(), world)
            }
            TaskKind::FormSpatialGroup => eval_world::eval_form_spatial_group(args, world),
            TaskKind::FormSpatialGroupWithID => {
                eval_world::eval_form_spatial_group_with_id(args, world)
            }
            TaskKind::LiveOnPatchRes => eval_world::eval_live_on_patch_res(args, world),
            TaskKind::CollectOddCell => eval_world::eval_collect_odd_cell(args, world),
            TaskKind::ConsumeTarget => eval_world::eval_consume_target(args, world),
            TaskKind::ConsumeTargetEcho => {
                eval_world::eval_consume_target_echo(args, ctx_ro, world)
            }
            TaskKind::ConsumeTargetLogic(op) => {
                eval_world::eval_consume_target_logic(op, args, ctx_ro, world)
            }
            TaskKind::MoveFT => eval_world::eval_move_ft(args, world),
            TaskKind::Exploded => eval_world::eval_exploded(world),
            TaskKind::Exploded2 => eval_world::eval_exploded2(world),
            TaskKind::ConsumePublicGood => eval_world::eval_consume_public_good(world),
            TaskKind::AIDisplayCost => eval_world::eval_ai_display_cost(world),
            TaskKind::ProducePublicGood => eval_world::eval_produce_public_good(world),
            TaskKind::OpinionIs => eval_world::eval_opinion_is(args, world),
            // Everything else was handled by `test_output_pure`.
            _ => 0.0,
        }
    }
}
