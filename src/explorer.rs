/// Explorer module for optimization using an e-graph with generic search strategies.
use crate::egraph::{
    analysis::Analysis,
    core::{ClassId, EGraph, NodeClass, NodeId},
    language::{Language, Term},
    rewrite::Rewrite,
};
use crate::metrics::Metrics;
use log::info;

pub struct Explorer<L: Language, N: Analysis<L>, T, S> {
    // e-graph
    pub egraph: EGraph<L, N>,
    // rule set
    pub(crate) rules: Vec<Rewrite<L>>,
    // task queue
    task_queue: Vec<T>,
    // Additional seach stategy specific data
    pub(crate) state: S,
    // root e-class id for extraction
    root: Option<NodeClass>,
    // limits for early stopping
    time_limit: Option<std::time::Duration>,
    // task limit for early stopping
    task_limit: Option<usize>,
    // metrics
    metrics: Metrics,
}

impl<L, N, T, S> Explorer<L, N, T, S>
where
    L: Language,
    N: Analysis<L>,
    T: Clone,
    S: Clone + Default,
{
    /// Create a new Explorer.
    pub fn new(analysis: N) -> Self {
        Self {
            egraph: EGraph::new(analysis),
            rules: Vec::new(),
            task_queue: Vec::new(),
            state: S::default(),
            root: None,
            time_limit: None,
            task_limit: None,
            metrics: Metrics::new(),
        }
    }

    /// Add an expression to the e-graph.
    /// The most recently added expression will be used as the root for extraction unless overridden by `with_expr`.
    pub fn with_expr(mut self, expr: &Term<L>) -> Self {
        let root_id = self.egraph.add_term(expr);
        self.root = Some(root_id);
        self
    }

    /// Manually set the root e-class id for extraction.
    pub fn with_root(mut self, root: NodeClass) -> Self {
        self.root = Some(root);
        self
    }

    /// Set the rewrite rules for the Explorer.
    /// NOTE: This fully overwrites any existing rules.
    pub fn with_rules(mut self, rules: Vec<Rewrite<L>>) -> Self {
        self.rules = rules;
        self
    }

    /// Set the time limit for the Explorer.
    pub fn with_time_limit(mut self, time_limit: std::time::Duration) -> Self {
        self.time_limit = Some(time_limit);
        self
    }

    /// Set the task limit for the Explorer.
    pub fn with_task_limit(mut self, task_limit: usize) -> Self {
        self.task_limit = Some(task_limit);
        self
    }

    /// Reset task queue and state for the Explorer.
    pub fn reset_tasks(&mut self) {
        self.task_queue.clear();
        self.state = S::default();
    }

    /// Push a task onto the task queue.
    pub(crate) fn push_task(&mut self, task: T) {
        self.task_queue.push(task);
    }

    /// Pop a task from the task queue.
    fn pop_task(&mut self) -> Option<T> {
        self.task_queue.pop()
    }

    /// Get root e-class id for extraction.
    pub(crate) fn get_root(&self) -> Option<NodeClass> {
        self.root
    }

    /// Get a node from the e-graph.
    // fn get_node(&self, id: NodeId) -> &L {
    //     self.egraph.get_node(id)
    // }

    /// Get the metrics for the explorer.
    pub fn get_metrics(&self) -> &Metrics {
        &self.metrics
    }

    /// Set the metrics for the explorer.
    fn set_metrics(&mut self, elapsed: std::time::Duration, tasks_processed: usize) {
        self.metrics.time_elapsed = elapsed;
        self.metrics.tasks_processed = tasks_processed;
        self.metrics.total_nodes = self.egraph.num_nodes();
        self.metrics.total_classes = self.egraph.num_classes();
    }

    /// Reset the metrics for the explorer.
    fn reset_metrics(&mut self) {
        self.metrics = Metrics::new();
    }

    // TODO: More e-graph interactions
    // TODO: Metrics handling
}

/// Trait for defining a search strategy for processing tasks in the Explorer.
pub trait SearchStrategy<T, S> {
    /// Process a task according to the search strategy.
    /// This method is called for each task popped from the task queue.
    /// This methods is responsible for updating the e-graph and potentially adding new tasks to the task queue.
    fn process_task(&mut self, task: T);

    /// Initialize the task queue and state for the search strategy.
    /// Assumes we are optimizing the current root class.
    /// Called once at the start of run().
    fn init_queue(&mut self) {
        info!(
            "Task queue initialized with default implementation (none!). Override this method for custom initialization."
        );
    }
}

/// Trait for running the Explorer with a specific search strategy.
pub trait Runner {
    /// Run the Explorer until a stopping condition is met.
    fn run(&mut self) -> StopReason;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    TimeLimitExceeded,
    TaskLimitExceeded,
    NoMoreTasks,
}

/// Auto-derived Runner implementation for Explorer when it implements SearchStrategy.
impl<L, N, T, S> Runner for Explorer<L, N, T, S>
where
    L: Language,
    N: Analysis<L>,
    Explorer<L, N, T, S>: SearchStrategy<T, S>,
    T: Clone,
    S: Clone + Default,
{
    fn run(&mut self) -> StopReason {
        self.reset_metrics();
        // QUESTION: Should we count this in the time metric? or no?
        self.init_queue();
        let start_time = std::time::Instant::now();
        let mut task_count = 0;

        while let Some(task) = self.pop_task() {
            // If we've hit the time limit, stop processing tasks.
            if let Some(time_limit) = self.time_limit {
                if start_time.elapsed() >= time_limit {
                    self.set_metrics(start_time.elapsed(), task_count);
                    return StopReason::TimeLimitExceeded;
                }
            }

            // If we've hit the task limit, stop processing tasks.
            if let Some(task_limit) = self.task_limit {
                if task_count >= task_limit {
                    self.set_metrics(start_time.elapsed(), task_count);
                    return StopReason::TaskLimitExceeded;
                }
            }

            // Process the task using the search strategy.
            self.process_task(task);
            task_count += 1;
        }

        self.set_metrics(start_time.elapsed(), task_count);
        StopReason::NoMoreTasks
    }
}

// pub trait Extractor<CF, L>
// where
//     CF: CostFunction<L>,
//     L: Language,
// {
//     fn extract(&self, eclass: Id, cost_function: CF) -> (CF::Cost, RecExpr<L>);
// }

// /// Auto-derived Extractor implementation for Explorer.
// impl<CF, L, N, T, S> Extractor<CF, L> for Explorer<L, N, T, S>
// where
//     CF: CostFunction<L>,
//     L: Language,
//     N: Analysis<L>,
//     T: Clone,
//     S: Clone + Default,
// {
//     fn extract(&self, eclass: Id, cost_function: CF) -> (CF::Cost, RecExpr<L>) {
//         let extractor = egg::Extractor::new(&self.egraph, cost_function);
//         extractor.find_best(eclass)
//     }
// }
