/// Top down exploration strategy for the Explorer.
use crate::explorer::{Explorer, SearchStrategy};
use egg::{Analysis, Id, Language, RecExpr};

pub enum Task {
    // TODO
}

pub type TopDownExplorer<L, N> = Explorer<L, N, Task>;

impl<L, N> SearchStrategy<Task> for TopDownExplorer<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    fn process_task(&mut self, task: Task) {
        todo!("Implement the actual top-down processing logic for the task.");
    }
}

pub enum StructuralTask {
    // TODO
}

pub type StructuralTopDownExplorer<L, N> = Explorer<L, N, StructuralTask>;

impl<L, N> SearchStrategy<StructuralTask> for StructuralTopDownExplorer<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    fn process_task(&mut self, task: StructuralTask) {
        todo!("Implement the actual structural top-down processing logic for the task.");
    }
}
