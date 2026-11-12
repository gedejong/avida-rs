//! Task evaluators that reach into the organism, deme, population or world
//! through [`TaskWorld`] (avida-core/source/main/cTaskLib.cc).
//!
//! Each function is the body of the `Task_*` method of the same name; the
//! C++ `ctx.GetOrganism()` / `m_world->...` calls are [`TaskWorld`] methods.

use crate::arg_schema::ArgContainer;
use crate::context::{TaskContext, TaskWorld};
use crate::eval_logic::{eval_logic, LogicOp};
use crate::io_buffer::IoBuffer;
use std::collections::{BTreeMap, HashMap};

/// `Task_Nand_ResourceDependent`: Nand that only pays while the pheromone
/// level is below the crossover (100).
pub(crate) fn eval_nand_res_dep(logic_id: i32, world: &dyn TaskWorld) -> f64 {
    const RES_CROSSOVER_LEVEL: f64 = 100.0;
    if eval_logic(LogicOp::Nand, logic_id) == 0.0 {
        return 0.0;
    }
    if world.pheromone_resource_amount() < RES_CROSSOVER_LEVEL {
        1.0
    } else {
        0.0
    }
}

/// `Task_Nor_ResourceDependent`: Nor that only pays above the crossover.
pub(crate) fn eval_nor_res_dep(logic_id: i32, world: &dyn TaskWorld) -> f64 {
    const RES_CROSSOVER_LEVEL: f64 = 100.0;
    if eval_logic(LogicOp::Nor, logic_id) == 0.0 {
        return 0.0;
    }
    if world.pheromone_resource_amount() > RES_CROSSOVER_LEVEL {
        1.0
    } else {
        0.0
    }
}

/// `Task_XorMax`: point the task's reaction at the most plentiful resource in
/// the cell, then evaluate Xor.
pub(crate) fn eval_xor_max(ctx: &TaskContext, world: &mut dyn TaskWorld) -> f64 {
    let cell_res = if !world.use_avatars() {
        world.get_resources()
    } else {
        world.get_av_resources()
    };

    let mut max_amount = 0.0;
    let mut max_res = 0;
    // if more than one resource is available, set the reaction to use the resource with the
    // most available in this spot (note that, with global resources, the GLOBAL total will
    // evaluated)
    for (i, &amount) in cell_res.iter().enumerate() {
        if amount > max_amount {
            max_amount = amount;
            max_res = i as i32;
        }
    }
    let Some(task_id) = ctx.task_entry else {
        return 0.0;
    };
    if !world.change_reaction_resource(task_id, max_res) {
        return 0.0; // `return false`
    }
    eval_logic(LogicOp::Xor, ctx.logic_id)
}

/// `Task_SortInputs`: score how sorted the organism's output is relative to
/// its own inputs.
///
/// Args: int 0 = size, 1 = direction (negative = descending), 2 = contiguous;
/// double 0 = halflife.
pub(crate) fn eval_sort_inputs(
    args: &ArgContainer,
    output: &IoBuffer,
    world: &dyn TaskWorld,
) -> f64 {
    let size = args.get_int(0);
    let stored = output.num_stored();

    // if less than half, can't possibly reach threshold
    if stored <= (size / 2) {
        return 0.0;
    }

    let mut valmap: HashMap<i32, i32> = HashMap::new();
    let mut score = 0i32;
    let mut maxscore = 0i32;

    // add all valid inputs into the value map
    for i in 0..size {
        valmap.insert(world.get_input_at(i), -1);
    }

    let mut span_start = -1i32;
    let mut span_end = stored;

    if args.get_int(2) != 0 {
        // Contiguous: scan for the largest contiguous span; in the event of a
        // tie, keep the first discovered
        let mut i = 0;
        while i < stored {
            if valmap.contains_key(&output.get(i)) {
                let t_start = i;
                i += 1;
                while i < stored && valmap.contains_key(&output.get(i)) {
                    i += 1;
                }
                if span_start == -1 || (i - t_start) > (span_end - span_start) {
                    span_start = t_start;
                    span_end = i;
                }
            }
            i += 1;
        }

        // no span was found
        if span_start == -1 {
            return 0.0;
        }
    } else {
        // Scattered: search for the first entry that is not a valid input.
        // QUIRK: the C++ loop `while (++span_start < stored && valmap.Has(...))`
        // stops at the first output NOT in the input set.
        loop {
            span_start += 1;
            if !(span_start < stored && valmap.contains_key(&output.get(span_start))) {
                break;
            }
        }

        // scanned past the end of the output, nothing to validate
        if span_start >= stored {
            return 0.0;
        }
    }

    // again, if span is less than half the size can't possibly reach threshold
    if (span_end - span_start) <= (size / 2) {
        return 0.0;
    }

    // insertion sort span
    // - count number of actual entries
    // - count moves required
    // - update valmap, tracking observed inputs
    let mut sorted = vec![0i32; stored.max(1) as usize + 1];
    let ascending = args.get_int(1) >= 0;
    let mut count = 1i32;

    // store first value
    valmap.insert(output.get(span_start), span_start);
    sorted[0] = output.get(span_start);

    // iterate over the remaining span (discovered for contiguous, full output for scattered)
    for i in (span_start + 1)..span_end {
        let value = output.get(i);

        // check for a dup or invalid output, skip it if so
        match valmap.get(&value) {
            Some(&-1) => {}
            _ => continue,
        }

        maxscore += count; // count the maximum moves possible
        count += 1; // iterate the observed count
        valmap.insert(value, i); // save position, so that missing values can be determined later

        // sort value based on ascending for descending, counting moves
        let mut j = count - 2;
        while j >= 0
            && ((ascending && sorted[j as usize] > value)
                || (!ascending && sorted[j as usize] < value))
        {
            sorted[(j + 1) as usize] = sorted[j as usize];
            j -= 1;
            score += 1;
        }
        sorted[(j + 1) as usize] = value;
    }

    // if not all of the inputs were observed
    if count < size {
        // iterate over all inputs
        for i in 0..size {
            // if input was not observed
            if valmap.get(&world.get_input_at(i)) == Some(&-1) {
                maxscore += count; // add to the maximum move count
                score += count; // missing values, scored as maximally out of order
                count += 1; // increment observed count
            }
        }
    }

    let mut quality = 0.0;

    // score of 50% expected with random output
    // - only grant quality when less than 50% maximum moves are required
    if f64::from(score) / f64::from(maxscore) < 0.5 {
        let halflife = -1.0 * args.get_double(0).abs();
        quality = 2.0f64.powf(f64::from(score) / halflife);
    }

    quality
}

/// `Task_MatchProdStr` (never registered; see `TaskKind::MatchProdStr`).
///
/// Args: string 0 = pattern; int 0 = partial, 1 = binary, 2 = tag; double 0 = pow.
pub(crate) fn eval_match_prod_str(
    args: &ArgContainer,
    output: &IoBuffer,
    world: &mut dyn TaskWorld,
) -> f64 {
    let tag = args.get_int(2);
    // These even out the stats tracking.
    world.stats_add_tag(tag, 0);
    world.stats_add_tag(-1, 0);

    let pattern = args.get_string(0);
    let bytes = pattern.as_bytes();
    let partial = args.get_int(0);
    let binary = args.get_int(1);
    let mypow = args.get_double(0);
    let len = bytes.len();
    let mut num_matched = 0;
    let mut max_num_matched = 0;
    let mut num_real = 0;

    if binary == 0 {
        if output.num_stored() > 0 {
            let test_output = output.get(0);
            for j in 0..len {
                let string_index = len - j - 1; // start with last char in string
                let k = 1i32.wrapping_shl(j as u32);
                if (bytes[string_index] == b'0' && (test_output & k) == 0)
                    || (bytes[string_index] == b'1' && (test_output & k) != 0)
                {
                    num_matched += 1;
                }
            }
            max_num_matched = num_matched;
        }
    } else {
        for (j, &p) in bytes.iter().enumerate() {
            if p != b'9' {
                num_real += 1;
            }
            let o = output.get(j as i32);
            if (p == b'0' && o == 0) || (p == b'1' && o == 1) {
                num_matched += 1;
            }
        }
        max_num_matched = num_matched;
    }

    // Check if the organism already produced this string.
    // If so, it receives a perfect score for this task.
    if world.match_already_produced() {
        let prod = world.get_number_strings_produced(tag);
        if prod != 0 {
            max_num_matched = len as i32;
        }
    }

    // Update the organism's tag.
    world.update_tag(tag, max_num_matched);
    if world.get_tag_label() == tag {
        world.set_lineage_label(args.get_int(2));
    }

    // Update stats
    let name = format!("[produced{}]", pattern);
    world.stats_add_string_bits_matched_value(&name, max_num_matched);

    // if the organism hasn't donated, then zero out its reputation.
    if world.get_reputation() > 0 && world.get_number_of_donations() == 0 {
        world.set_reputation(0);
    }

    let mut bonus = 0.0;
    let mut base_bonus = f64::from(max_num_matched) * 2.0 / len as f64 - 1.0;
    if partial != 0 {
        base_bonus = f64::from(max_num_matched) * 2.0 / f64::from(num_real) - 1.0;
    }
    if base_bonus > 0.0 {
        bonus = base_bonus.powf(mypow);
    }
    bonus
}

/// `Task_MoveUpGradient`.
pub(crate) fn eval_move_up_gradient(world: &dyn TaskWorld) -> f64 {
    if world.get_gradient_movement() == 1.0 {
        return 1.0;
    }
    0.0
}

/// `Task_MoveNeutralGradient`.
pub(crate) fn eval_move_neutral_gradient(world: &dyn TaskWorld) -> f64 {
    if world.get_gradient_movement() == 0.0 {
        return 1.0;
    }
    0.0
}

/// `Task_MoveDownGradient`.
pub(crate) fn eval_move_down_gradient(world: &dyn TaskWorld) -> f64 {
    if world.get_gradient_movement() == -1.0 {
        return 1.0;
    }
    0.0
}

/// `Task_MoveNotUpGradient`.
pub(crate) fn eval_move_not_up_gradient(world: &dyn TaskWorld) -> f64 {
    if eval_move_up_gradient(world) != 0.0 {
        return 0.0;
    }
    1.0
}

/// `Task_MoveToRightSide`.
pub(crate) fn eval_move_to_right_side(world: &dyn TaskWorld) -> f64 {
    let x = world.deme_cell_position_x(world.get_cell_id());
    if x == world.world_x().wrapping_sub(1) {
        return 1.0;
    }
    0.0
}

/// `Task_MoveToLeftSide`.
pub(crate) fn eval_move_to_left_side(world: &dyn TaskWorld) -> f64 {
    let x = world.deme_cell_position_x(world.get_cell_id());
    if x == 0 {
        return 1.0;
    }
    0.0
}

/// The cell id `Task_Move` / `Task_MoveFT` use: the avatar's cell when
/// `USE_AVATARS` is set.
fn effective_cell_id(world: &dyn TaskWorld) -> i32 {
    if world.use_avatars() {
        world.get_av_cell_id()
    } else {
        world.get_cell_id()
    }
}

/// `Task_Move`: rewards every change of cell.
pub(crate) fn eval_move(world: &mut dyn TaskWorld) -> f64 {
    let cell_id = effective_cell_id(world);
    if cell_id != world.get_prev_seen_cell_id() {
        world.set_prev_seen_cell_id(cell_id);
        return 1.0;
    }
    0.0
}

/// `Task_MoveToTarget`: reward arriving on a (different) target cell.
pub(crate) fn eval_move_to_target(world: &mut dyn TaskWorld) -> f64 {
    if world.get_cell_id() == -1 {
        return 0.0;
    }
    let cell_data = world.get_cell_data();
    if cell_data <= 0 {
        return 0.0;
    }

    let current_cell = world.deme_get_relative_cell_id(world.get_cell_id());
    let prev_target = world.deme_get_relative_cell_id(world.get_prev_task_cell_id());

    // If the organism is currently on a target cell, see which target cell it previously
    // visited. Since we want them to move back and forth, only reward if we are on
    // a different target cell.
    if cell_data > 1 {
        if current_cell == prev_target {
            // At some point, we may want to return a fraction
            return 0.0;
        } else {
            world.add_reached_task_cell();
            world.set_prev_task_cell_id(current_cell);
            return 1.0;
        }
    }
    0.0
}

/// `Task_MoveToMovementEvent`.
pub(crate) fn eval_move_to_movement_event(world: &mut dyn TaskWorld) -> f64 {
    if world.get_cell_id() == -1 {
        return 0.0;
    }
    let cell_data = world.get_cell_data();
    if cell_data <= 0 {
        return 0.0;
    }
    for i in 0..world.deme_num_movement_predicates() {
        // The C++ dereferences `GetEvent(0)` without a null check; a missing
        // event simply cannot match here.
        if let Some((event_id, _)) = world.deme_mov_predicate_event(i, 0) {
            if event_id == cell_data {
                world.add_reached_task_cell();
                world.set_prev_task_cell_id(cell_data);
                return 1.0;
            }
        }
    }
    0.0
}

/// `Task_MoveBetweenMovementEvent`.
pub(crate) fn eval_move_between_movement_event(world: &mut dyn TaskWorld) -> f64 {
    if world.get_cell_id() == -1 {
        return 0.0;
    }
    let cell_data = world.get_cell_data();
    let prev_target = world.deme_get_relative_cell_id(world.get_prev_task_cell_id());

    // NOTE: as of now, orgs aren't rewarded if they touch a target more than
    //   once in a row.
    if cell_data <= 0 || cell_data == prev_target {
        return 0.0;
    }

    for i in 0..world.deme_num_movement_predicates() {
        // QUIRK: the C++ hard-codes `num_events = 2` (calling GetNumEvents
        // "has problems"); the single-event branch is dead code.
        let num_events = 2;
        for j in 0..num_events {
            if let Some((event_id, active)) = world.deme_mov_predicate_event(i, j) {
                if active && event_id == cell_data {
                    world.add_reached_task_cell();
                    world.set_prev_task_cell_id(cell_data);
                    return 1.0;
                }
            }
        }
    }
    0.0
}

/// `Task_MoveToEvent`.
pub(crate) fn eval_move_to_event(world: &dyn TaskWorld) -> f64 {
    if world.get_cell_id() == -1 {
        return 0.0;
    }
    let cell_data = world.get_cell_data();
    if cell_data <= 0 {
        return 0.0;
    }
    for i in 0..world.deme_num_events() {
        if world.deme_cell_event_id(i) == cell_data {
            return 1.0;
        }
    }
    0.0
}

/// `Task_EventKilled`.
pub(crate) fn eval_event_killed(world: &dyn TaskWorld) -> f64 {
    if world.get_event_killed() {
        return 1.0;
    }
    0.0
}

/// `Task_SGPathTraversal`: fraction of the requested path length covered by
/// distinct non-poison cells visited.
///
/// Args: int 0 = pathlen; string 0 = sgname, 1 = poison.
pub(crate) fn eval_sg_path_traversal(
    args: &ArgContainer,
    ctx: &TaskContext,
    world: &dyn TaskWorld,
) -> f64 {
    if world.state_grid_name() != args.get_string(0) {
        return 0.0;
    }

    let state = world.state_grid_state_id(args.get_string(1));
    if state < 0 {
        return 0.0;
    }

    let ext_mem = ctx.ext_mem;

    // Build and sort history
    let history_offset = 3 + world.state_grid_num_states();
    let mut history: Vec<i32> = ext_mem
        .iter()
        .skip(history_offset.max(0) as usize)
        .copied()
        .collect();
    history.sort_unstable();

    // Calculate how many unique non-poison cells have been touched
    let mut traversed = 0i32;
    let mut last = -1;
    for &h in &history {
        if h == last {
            continue;
        }
        last = h;
        if world.state_grid_state_at(last) != state {
            traversed += 1;
        }
    }

    traversed -= ext_mem.get((3 + state) as usize).copied().unwrap_or(0);

    // quality = (traversed >= 0 ? traversed : 0) / (double) pathlen
    f64::from(if traversed >= 0 { traversed } else { 0 }) / f64::from(args.get_int(0))
}

/// `Task_CreatePerfectStrings`: `num_strings` is `m_strings.size()` (the
/// number of `matchprodstr` tasks loaded).
pub(crate) fn eval_create_perfect_strings(num_strings: usize, world: &mut dyn TaskWorld) -> f64 {
    let mut min = -1;
    for i in 0..num_strings {
        let temp = world.get_number_strings_on_hand(i as i32);

        // Figure out what the minimum amount of a string is.
        if min == -1 || temp < min {
            min = temp;
        }
    }

    // Bonus for creating perfect strings!
    let mut bonus = f64::from(min);

    // Add in some value for just creating stuff
    for i in 0..num_strings {
        let temp = world.get_number_strings_on_hand(i as i32);
        if temp > min {
            bonus += f64::from(temp - min);
        }
    }

    // Update stats
    world.stats_inc_perfect_match(min);
    if min > 0 {
        world.stats_inc_perfect_match_org();
    }

    bonus
}

/// `Task_FormSpatialGroup`.
///
/// QUIRK: no guard for an ideal size of 0: `0/0` is NaN and the C++ returns
/// it (`reward < 0` is false for NaN).
pub(crate) fn eval_form_spatial_group(args: &ArgContainer, world: &mut dyn TaskWorld) -> f64 {
    let t = f64::from(args.get_int(0));
    let mut group_id = 0;
    if world.has_opinion() {
        group_id = world.get_opinion_first();
    }
    let g = f64::from(world.number_of_organisms_in_group(group_id));
    let num = (t - g) * (t - g);
    let denom = t * t;

    let mut reward = 1.0 - (num / denom);
    if reward < 0.0 {
        reward = 0.0;
    }
    reward
}

/// `Task_FormSpatialGroupWithID`.
pub(crate) fn eval_form_spatial_group_with_id(
    args: &ArgContainer,
    world: &mut dyn TaskWorld,
) -> f64 {
    let t = f64::from(args.get_int(0));
    let des_group_id = args.get_int(1);

    let mut reward = 0.0;
    let mut group_id = -1;
    if world.has_opinion() {
        group_id = world.get_opinion_first();
    }

    // If the organism is in the group...
    if group_id == des_group_id {
        let g = f64::from(world.number_of_organisms_in_group(group_id));
        // If the population size is less than the max size
        if g < t {
            reward = 1.0;
        } else {
            let num = (t - g) * (t - g);
            let denom = t * t;

            if denom > 0.0 {
                reward = 1.0 - (num / denom);
                if reward < 0.0 {
                    reward = 0.0;
                }
            } else {
                reward = 0.0;
            }
        }
    }
    reward
}

/// `Task_LiveOnPatchRes`: reward for an opinion equal to the patch id.
///
/// QUIRK: without an opinion the id is -1, so `patch_id=-1` is rewarded for
/// opinion-less organisms (unlike `Task_OpinionIs`).
pub(crate) fn eval_live_on_patch_res(args: &ArgContainer, world: &dyn TaskWorld) -> f64 {
    let des_patch_id = args.get_int(0);
    let mut patch_id = -1;
    if world.has_opinion() {
        patch_id = world.get_opinion_first();
    }
    if patch_id == des_patch_id {
        1.0
    } else {
        0.0
    }
}

/// `Task_CollectOddCell`: `even_or_odd` 0 rewards odd cells, otherwise even.
pub(crate) fn eval_collect_odd_cell(args: &ArgContainer, world: &dyn TaskWorld) -> f64 {
    let even_odds = args.get_int(0);
    let mut reward = 0.0;
    // `%` keeps the sign of the dividend, as in C++ (cell -1 counts as odd)
    let cell_id_mod_2 = world.get_cell_id() % 2;
    if even_odds == 0 {
        if cell_id_mod_2 != 0 {
            reward = 1.0;
        }
    } else if cell_id_mod_2 == 0 {
        reward = 1.0;
    }
    reward
}

/// `Task_ConsumeTarget`.
pub(crate) fn eval_consume_target(args: &ArgContainer, world: &dyn TaskWorld) -> f64 {
    if world.get_forage_target() == args.get_int(0) {
        1.0
    } else {
        0.0
    }
}

/// `Task_ConsumeTargetEcho`.
pub(crate) fn eval_consume_target_echo(
    args: &ArgContainer,
    ctx: &TaskContext,
    world: &dyn TaskWorld,
) -> f64 {
    if world.get_forage_target() == args.get_int(0) {
        crate::eval_misc::eval_echo(ctx.input_buffer, ctx.output_buffer)
    } else {
        0.0
    }
}

/// `Task_ConsumeTargetNand` .. `Task_ConsumeTargetEqu`.
pub(crate) fn eval_consume_target_logic(
    op: LogicOp,
    args: &ArgContainer,
    ctx: &TaskContext,
    world: &dyn TaskWorld,
) -> f64 {
    if world.get_forage_target() == args.get_int(0) {
        eval_logic(op, ctx.logic_id)
    } else {
        0.0
    }
}

/// `Task_MoveFT`: reward a move made while foraging for the desired target.
pub(crate) fn eval_move_ft(args: &ArgContainer, world: &mut dyn TaskWorld) -> f64 {
    let mut reward = 0.0;
    let cell_id = effective_cell_id(world);
    let moved = cell_id != world.get_prev_seen_cell_id();

    if moved {
        let des_target = args.get_int(0);
        let target_res = world.get_forage_target();

        // If the organism is on the right resource...
        if target_res == des_target {
            reward = 1.0;
            world.set_prev_seen_cell_id(cell_id);
        }
    }
    reward
}

/// `Task_Exploded`.
pub(crate) fn eval_exploded(world: &dyn TaskWorld) -> f64 {
    if world.get_kaboom_executed() {
        1.0
    } else {
        0.0
    }
}

/// `Task_Exploded2`.
pub(crate) fn eval_exploded2(world: &dyn TaskWorld) -> f64 {
    if world.get_kaboom_executed2() {
        1.0
    } else {
        0.0
    }
}

/// `Task_AIDisplayCost`: charged while the lyse (autoinducer) flag is set.
pub(crate) fn eval_ai_display_cost(world: &dyn TaskWorld) -> f64 {
    if world.get_lyse_display() {
        1.0
    } else {
        0.0
    }
}

/// `Task_ProducePublicGood`: the same flag as `Task_Exploded`.
pub(crate) fn eval_produce_public_good(world: &dyn TaskWorld) -> f64 {
    if world.get_kaboom_executed() {
        1.0
    } else {
        0.0
    }
}

/// `Task_ConsumePublicGood`: when at least three of the nine surrounding cells
/// (toroidal) hold a producer, consume from one of them.
///
/// The C++ picks the producer with `std::rand() % size` (the C library
/// generator, [`TaskWorld::std_rand`]) into a `std::set<cOrganism*>`, which is
/// ordered by heap address. With a single producer (only possible in worlds
/// two cells wide or high, where wrapped offsets revisit cells) the pick is
/// defined; with more the order cannot be reproduced, which the evaluator
/// reports through [`TaskWorld::unreproducible_producer_choice`] before
/// carrying on with the producers in cell order. The producer set is
/// deduplicated by cell (an organism reached through several wrapped offsets
/// counts once in the set but every visit counts towards `good_counter`, as
/// in C++).
pub(crate) fn eval_consume_public_good(world: &mut dyn TaskWorld) -> f64 {
    let mut good_counter = 0;
    let cell_id = world.get_cell_id();
    let radius = 1;

    let world_x = world.world_x();
    let world_y = world.world_y();
    if world_x == 0 {
        return 0.0; // `cellID % 0`: undefined in C++
    }
    let cell_x = cell_id % world_x;
    let cell_y = (cell_id - cell_x) / world_x;

    let mut prod_set: BTreeMap<i32, ()> = BTreeMap::new();
    for i in (cell_x - radius)..=(cell_x + radius) {
        for j in (cell_y - radius)..=(cell_y + radius) {
            // TODO in C++: make it modulus instead of subtract so that the
            // radius can be bigger than the size of the world.
            let x = if i < 0 {
                world_x + i
            } else if i >= world_x {
                i - world_x
            } else {
                i
            };
            let y = if j < 0 {
                world_y + j
            } else if j >= world_y {
                j - world_y
            } else {
                j
            };

            let neighbor = y * world_x + x;
            // do we actually have someone in neighborhood?
            if world.cell_kaboom_executed(neighbor) == Some(true) {
                good_counter += 1;
                prod_set.insert(neighbor, ());
            }
        }
    }
    if good_counter >= 3 {
        // Select a random organism from the producing set to actually get
        // resource from
        let size = prod_set.len() as i32;
        let r = (world.std_rand() % size) as usize;
        if size > 1 {
            world.unreproducible_producer_choice(prod_set.len());
        }
        if let Some((&chosen, _)) = prod_set.iter().nth(r) {
            world.clear_kaboom_executed(chosen);
        }
        1.0
    } else {
        0.0
    }
}

/// `Task_OpinionIs`: the organism's opinion equals the configured value.
pub(crate) fn eval_opinion_is(args: &ArgContainer, world: &dyn TaskWorld) -> f64 {
    if world.has_opinion() {
        if world.get_opinion_first() == args.get_int(0) {
            1.0
        } else {
            0.0
        }
    } else {
        0.0
    }
}
