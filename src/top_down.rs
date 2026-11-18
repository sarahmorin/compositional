use crate::ast::MinAST;
use crate::egraph::{core::NodeId, language::Language};
/// Top down exploration strategy for the Explorer.
use crate::explorer::{Explorer, SearchStrategy};

pub enum Task {
    // TODO
}

struct State;

/// Top down explorer search strategy
///
/// This is a very naive top-down optimizer.
pub type TopDownExplorer<L> = Explorer<L, MinAST, Task, State>;

impl<L> SearchStrategy<Task, State> for TopDownExplorer<L>
where
    L: Language,
{
    fn init_queue(&mut self) {
        todo!("Implement the actual initialization logic for the bottom-up task queue.");
    }

    fn process_task(&mut self, task: Task) {
        todo!("Implement the actual top-down processing logic for the task.");
    }
}

pub enum StructuralTask {
    // TODO
}

pub type StructuralTopDownExplorer<L> = Explorer<L, MinAST, StructuralTask, State>;

impl<L> SearchStrategy<StructuralTask, ()> for StructuralTopDownExplorer<L>
where
    L: Language,
{
    fn init_queue(&mut self) {
        todo!("Implement the actual initialization logic for the bottom-up task queue.");
    }

    fn process_task(&mut self, task: StructuralTask) {
        todo!("Implement the actual structural top-down processing logic for the task.");
    }
}
