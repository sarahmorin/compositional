/// Bottom up explorer search strategy
use crate::explorer::{Explorer, SearchStrategy};
use egg::{Analysis, Id, Language, RecExpr};

pub enum Task {
    // TODO: Define the structure of a task for bottom-up exploration.
}

pub type BottomUpExplorer<L, N> = Explorer<L, N, Task>;

impl<L, N> SearchStrategy<Task> for BottomUpExplorer<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    fn process_task(&mut self, task: Task) {
        todo!("Implement the actual bottom-up processing logic for the task.");
    }
}
