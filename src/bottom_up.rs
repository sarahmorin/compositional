use crate::egraph::{analysis::Analysis, language::Language};
/// Bottom up explorer search strategy
use crate::explorer::{Explorer, SearchStrategy};

#[derive(Debug, Clone)]
pub enum Task {
    // TODO: Define the structure of a task for bottom-up exploration.
}

#[derive(Debug, Clone, Default)]
struct State;

pub type BottomUpExplorer<L, N> = Explorer<L, N, Task, State>;

impl<L, N> SearchStrategy<Task, State> for BottomUpExplorer<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    fn init_queue(&mut self) {
        self.reset_tasks();

        todo!("Implement the actual initialization logic for the bottom-up task queue.");
    }

    fn process_task(&mut self, task: Task) {
        todo!("Implement the actual bottom-up processing logic for the task.");
    }
}
