/// The recursive explorer is a naive search strategy that explore the initial expression recursively.
/// We begin at the root, and recursively optimize children until.
/// Once children have been optimized, we optimize the parent node.
/// At every level, we keep the best node so far and only expose the best node to the parent.
use crate::ast::MinAST;
use crate::egraph::{analysis::Analysis, core::ClassId, core::NodeId, language::Language};
use crate::explorer::{Explorer, SearchStrategy};

pub enum Task {
    // Entry point for optimizing a class
    Optimize(ClassId),
    // Optimize a particular node
    OptimizeNode(NodeId),
    // Finish optimizing a node after all children have been optimized
    OptimizeWithChildren(NodeId),
}

struct State {
    // TODO: Keep track of classes/nodes in progress to break cycles
}

/// Recursive explorer search strategy
pub type RecursiveExplorer<L> = Explorer<L, MinAST, Task, State>;

impl<L> SearchStrategy<Task, State> for RecursiveExplorer<L>
where
    L: Language,
{
    fn init_queue(&mut self) {
        todo!("See the queue with initial search tasks.");
    }

    fn process_task(&mut self, task: Task) {
        todo!("Implement the actual top-down processing logic for the task.");
    }
}
