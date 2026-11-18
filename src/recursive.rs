/// The recursive explorer is a naive search strategy that explores the initial expression recursively.
/// Before a node is rewritten, its children are optimized. Rewrites at a node may then only see
/// the best node of each class below it, so every level exposes only its best option to its parent.
use crate::ast::MinAST;
use crate::egraph::{
    core::{ClassId, NodeId},
    language::{Language, Term},
    rewrite::Rewrite,
};
use crate::experiment::{Limits, Outcome, Strategy};
use crate::explorer::{Explorer, Runner, SearchStrategy};
use std::collections::{HashMap, HashSet};

/// Tasks run in LIFO order, so a task runs only after every task pushed after it.
#[derive(Clone, Debug)]
pub enum Task {
    /// Entry point for optimizing a class. Skipped if the class was already entered.
    Optimize(ClassId),
    /// Optimize a node's children, then the node itself.
    OptimizeNode(NodeId),
    /// Finish optimizing a node after all children have been optimized.
    OptimizeWithChildren(NodeId),
    /// Every node in the class has been optimized.
    FinishClass(ClassId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    InProgress,
    Done,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    /// Status of every class entered so far, keyed by canonical id.
    status: HashMap<ClassId, Status>,
    /// Nodes already scheduled for rewriting, so each node is rewritten at most once.
    visited: HashSet<NodeId>,
}

/// Recursive explorer search strategy
pub type RecursiveExplorer<L> = Explorer<L, MinAST, Task, State>;

/// The recursive explorer as an experiment strategy.
pub struct Recursive;

impl<L> Strategy<L> for Recursive
where
    L: Language,
{
    fn name(&self) -> &str {
        "recursive"
    }

    fn run(&self, expr: &Term<L>, rules: &[Rewrite<L>], limits: &Limits) -> Outcome<L> {
        let explorer = RecursiveExplorer::new(MinAST)
            .with_expr(expr)
            .with_rules(rules.to_vec());
        let mut explorer = limits.apply(explorer);
        let stop = explorer.run();
        let (_, term) = explorer.extract();
        Outcome {
            stop,
            metrics: explorer.get_metrics().clone(),
            term,
        }
    }
}

impl<L> RecursiveExplorer<L>
where
    L: Language,
{
    /// The best term found for the root expression and its cost.
    pub fn extract(&self) -> (usize, Term<L>) {
        let root = self.get_root().expect("explorer has no root expression");
        self.egraph.extract_min_ast(root.class())
    }

    /// Rebuilds the e-graph and re-keys class statuses under their canonical ids.
    fn rebuild(&mut self) {
        self.egraph.rebuild();
        for (class, status) in std::mem::take(&mut self.state.status) {
            let entry = self
                .state
                .status
                .entry(self.egraph.find(class))
                .or_insert(status);
            // A merged class is still in progress if either of its halves was.
            if status == Status::InProgress {
                *entry = status;
            }
        }
    }
}

impl<L> SearchStrategy<Task, State> for RecursiveExplorer<L>
where
    L: Language,
{
    fn init_queue(&mut self) {
        self.reset_tasks();
        let root = self.get_root().expect("explorer has no root expression");
        self.push_task(Task::Optimize(root.class()));
    }

    fn process_task(&mut self, task: Task) {
        match task {
            Task::Optimize(class) => {
                let class = self.egraph.find(class);
                // Done: already optimized. In progress: a cycle back to an ancestor, which
                // children see at its best so far.
                if self.state.status.contains_key(&class) {
                    return;
                }
                self.state.status.insert(class, Status::InProgress);
                self.push_task(Task::FinishClass(class));
                for node in self.egraph.class(class).nodes.clone() {
                    self.push_task(Task::OptimizeNode(node));
                }
            }
            Task::OptimizeNode(node) => {
                let node = self.egraph.find_node(node);
                if !self.state.visited.insert(node) {
                    return;
                }
                self.push_task(Task::OptimizeWithChildren(node));
                for child in self.egraph.get_node(node).children.clone() {
                    self.push_task(Task::Optimize(child));
                }
            }
            Task::OptimizeWithChildren(node) => {
                let node = self.egraph.find_node(node);
                let class = self.egraph.node_class(node).class();
                // Match every rule before applying any, so all rules see the same e-graph.
                let matches: Vec<_> = self
                    .rules
                    .iter()
                    .filter_map(|rule| {
                        let subst = self.egraph.match_restricted(&rule.search, node)?;
                        Some((&rule.apply, subst))
                    })
                    .collect();
                let mut new_roots = Vec::new();
                for (apply, subst) in matches {
                    let (result, root) = self.egraph.instantiate_root(apply, &subst);
                    self.egraph.union(class, result);
                    new_roots.extend(root);
                }
                self.rebuild();
                // New nodes at this level are optimized in turn: their children first (some
                // may be fresh classes built by the rewrite), then the nodes themselves.
                for root in new_roots {
                    self.push_task(Task::OptimizeNode(root));
                }
            }
            Task::FinishClass(class) => {
                let class = self.egraph.find(class);
                self.state.status.insert(class, Status::Done);
            }
        }
    }
}
