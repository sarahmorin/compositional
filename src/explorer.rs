/// Explorer module for optimization using an e-graph with generic search strategies.
use egg::{Analysis, CostFunction, EGraph, Id, Language, RecExpr, Rewrite};

pub struct Explorer<L: Language, N: Analysis<L>, T> {
    // e-graph
    egraph: EGraph<L, N>,
    // rule set
    rules: Vec<Rewrite<L, N>>,
    // task queue
    task_queue: Vec<T>,
    // limits for early stopping
    time_limit: Option<std::time::Duration>,
    // task limit for early stopping
    task_limit: Option<usize>,
}

impl<L, N, T> Explorer<L, N, T>
where
    L: Language,
    N: Analysis<L>,
    T: Clone,
{
    pub fn new(analysis: N) -> Self {
        Self {
            egraph: EGraph::new(analysis),
            rules: Vec::new(),
            task_queue: Vec::new(),
            time_limit: None,
            task_limit: None,
        }
    }

    /// Add an expression to the e-graph.
    pub fn with_expr(mut self, expr: &RecExpr<L>) -> Self {
        self.egraph.add_expr(expr);
        self
    }

    /// Set the rewrite rules for the Explorer.
    /// NOTE: This fully overwrites any existing rules.
    pub fn with_rules(mut self, rules: Vec<Rewrite<L, N>>) -> Self {
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

    /// Push a task onto the task queue.
    pub fn push_task(&mut self, task: T) {
        self.task_queue.push(task);
    }

    /// Pop a task from the task queue.
    fn pop_task(&mut self) -> Option<T> {
        self.task_queue.pop()
    }

    // TODO: Common e-graph interaction methods

    // TODO: Metrics handling
}

/// Trait for defining a search strategy for processing tasks in the Explorer.
pub trait SearchStrategy<T> {
    /// Process a task according to the search strategy.
    /// This method is called for each task popped from the task queue.
    /// This methods is responsible for updating the e-graph and potentially adding new tasks to the task queue.
    fn process_task(&mut self, task: T);
}

/// Trait for running the Explorer with a specific search strategy.
pub trait Runner {
    /// Run the Explorer until a stopping condition is met.
    fn run(&mut self) -> StopReason;
}

pub enum StopReason {
    TimeLimitExceeded,
    TaskLimitExceeded,
    NoMoreTasks,
}

/// Auto-derived Runner implementation for Explorer when it implements SearchStrategy.
impl<L, N, T> Runner for Explorer<L, N, T>
where
    L: Language,
    N: Analysis<L>,
    Explorer<L, N, T>: SearchStrategy<T>,
    T: Clone,
{
    fn run(&mut self) -> StopReason {
        let start_time = std::time::Instant::now();
        let mut task_count = 0;

        while let Some(task) = self.pop_task() {
            // If we've hit the time limit, stop processing tasks.
            if let Some(time_limit) = self.time_limit {
                if start_time.elapsed() >= time_limit {
                    return StopReason::TimeLimitExceeded;
                }
            }

            // If we've hit the task limit, stop processing tasks.
            if let Some(task_limit) = self.task_limit {
                if task_count >= task_limit {
                    return StopReason::TaskLimitExceeded;
                }
            }

            // Process the task using the search strategy.
            self.process_task(task);
            task_count += 1;
        }

        StopReason::NoMoreTasks
    }
}

pub trait Extractor<CF, L>
where
    CF: CostFunction<L>,
    L: Language,
{
    fn extract(&self, eclass: Id, cost_function: CF) -> (CF::Cost, RecExpr<L>);
}

/// Auto-derived Extractor implementation for Explorer.
impl<CF, L, N, T> Extractor<CF, L> for Explorer<L, N, T>
where
    CF: CostFunction<L>,
    L: Language,
    N: Analysis<L>,
    T: Clone,
{
    fn extract(&self, eclass: Id, cost_function: CF) -> (CF::Cost, RecExpr<L>) {
        let extractor = egg::Extractor::new(&self.egraph, cost_function);
        extractor.find_best(eclass)
    }
}
