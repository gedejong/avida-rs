//! Port of `cContextPhenotype` (avida-core/source/main/cContextPhenotype.{h,cc})
//! (data and its two count-accumulating methods).

/// `cContextPhenotype`: task and reaction counts accumulated for a context
/// (e.g. a thread), tested by context requisites.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextPhenotype {
    /// `m_cur_merit`
    pub cur_merit: f64,
    /// `m_cur_task_count`
    pub cur_task_count: Vec<i32>,
    /// `m_cur_reaction_count`
    pub cur_reaction_count: Vec<i32>,
    /// `m_number_tasks`
    pub number_tasks: i32,
    /// `m_number_reactions`
    pub number_reactions: i32,
}

impl ContextPhenotype {
    /// `cContextPhenotype()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddTaskCounts(number_tasks, cur_task_count)`: if the size changed,
    /// resize and zero first; then add element-wise.
    pub fn add_task_counts(&mut self, number_tasks: i32, cur_task_count: &[i32]) {
        if self.number_tasks != number_tasks {
            self.cur_task_count = vec![0; number_tasks.max(0) as usize];
            self.number_tasks = number_tasks;
        }
        for (dst, src) in self.cur_task_count.iter_mut().zip(cur_task_count) {
            *dst = dst.wrapping_add(*src);
        }
    }

    /// `AddReactionCounts(number_reactions, cur_reaction_count)`.
    pub fn add_reaction_counts(&mut self, number_reactions: i32, cur_reaction_count: &[i32]) {
        if self.number_reactions != number_reactions {
            self.cur_reaction_count = vec![0; number_reactions.max(0) as usize];
            self.number_reactions = number_reactions;
        }
        for (dst, src) in self.cur_reaction_count.iter_mut().zip(cur_reaction_count) {
            *dst = dst.wrapping_add(*src);
        }
    }

    /// `GetTaskCounts()`.
    pub fn task_counts(&self) -> &[i32] {
        &self.cur_task_count
    }

    /// `GetReactionCounts()`.
    pub fn reaction_counts(&self) -> &[i32] {
        &self.cur_reaction_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulate_and_resize() {
        let mut c = ContextPhenotype::new();
        c.add_task_counts(3, &[1, 2, 3]);
        c.add_task_counts(3, &[1, 1, 1]);
        assert_eq!(c.task_counts(), &[2, 3, 4]);
        c.add_task_counts(2, &[5, 5]);
        assert_eq!(c.task_counts(), &[5, 5]);
        c.add_reaction_counts(2, &[0, 0]);
        assert_eq!(c.reaction_counts(), &[0, 0]);
    }
}
