/// Experiment harness for comparing search strategies on a common set of expressions.
pub mod results;

use crate::egraph::{
    analysis::Analysis,
    extract::CostFunction,
    language::{Language, Term},
    rewrite::Rewrite,
};
use crate::explorer::{Explorer, StopReason};
use crate::metrics::Metrics;
use log::info;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

pub use results::{Record, Results};

/// A search strategy the harness can run.
///
/// Each strategy builds its own explorer and extracts with its own method. The harness then
/// scores every extracted term with the experiment's cost function, so results are comparable.
pub trait Strategy<L: Language> {
    fn name(&self) -> &str;

    /// Explores `expr` with `rules` under `limits` and extracts the best term found.
    fn run(&self, expr: &Term<L>, rules: &[Rewrite<L>], limits: &Limits) -> Outcome<L>;
}

/// What a single strategy run produces.
pub struct Outcome<L: Language> {
    pub stop: StopReason,
    pub metrics: Metrics,
    pub term: Term<L>,
}

/// Early-stopping limits applied to every run.
#[derive(Clone, Debug, Default)]
pub struct Limits {
    pub time: Option<Duration>,
    pub tasks: Option<usize>,
}

impl Limits {
    /// Sets these limits on an explorer.
    pub fn apply<L, N, T, S>(&self, mut explorer: Explorer<L, N, T, S>) -> Explorer<L, N, T, S>
    where
        L: Language,
        N: Analysis<L>,
        T: Clone,
        S: Clone + Default,
    {
        if let Some(time) = self.time {
            explorer = explorer.with_time_limit(time);
        }
        if let Some(tasks) = self.tasks {
            explorer = explorer.with_task_limit(tasks);
        }
        explorer
    }
}

/// Runs every strategy on every expression with a shared rule set and cost function.
pub struct Experiment<L: Language, C, D> {
    rules: Vec<Rewrite<L>>,
    cost_fn: C,
    exprs: Vec<(String, Term<L>)>,
    strategies: Vec<Box<dyn Strategy<L>>>,
    limits: Limits,
    _cost: PhantomData<D>,
}

impl<L, C, D> Experiment<L, C, D>
where
    L: Language,
    C: CostFunction<L, D>,
    D: PartialOrd + Clone,
{
    pub fn new(rules: Vec<Rewrite<L>>, cost_fn: C) -> Self {
        Self {
            rules,
            cost_fn,
            exprs: Vec::new(),
            strategies: Vec::new(),
            limits: Limits::default(),
            _cost: PhantomData,
        }
    }

    pub fn with_expr(mut self, name: &str, expr: Term<L>) -> Self {
        self.exprs.push((name.to_string(), expr));
        self
    }

    pub fn with_strategy(mut self, strategy: impl Strategy<L> + 'static) -> Self {
        self.strategies.push(Box::new(strategy));
        self
    }

    pub fn with_time_limit(mut self, time: Duration) -> Self {
        self.limits.time = Some(time);
        self
    }

    pub fn with_task_limit(mut self, tasks: usize) -> Self {
        self.limits.tasks = Some(tasks);
        self
    }

    /// Runs every (expression, strategy) pair, in expression-major order.
    pub fn run(&self) -> Results<L, D> {
        let mut records = Vec::new();
        for (expr_name, expr) in &self.exprs {
            let initial_cost = self.cost_fn.term_cost(expr);
            for strategy in &self.strategies {
                info!("running {} on {expr_name}", strategy.name());
                let start = Instant::now();
                let outcome = strategy.run(expr, &self.rules, &self.limits);
                let total_time = start.elapsed();
                records.push(Record {
                    expr: expr_name.clone(),
                    strategy: strategy.name().to_string(),
                    stop: outcome.stop,
                    metrics: outcome.metrics,
                    total_time,
                    initial_cost: initial_cost.clone(),
                    cost: self.cost_fn.term_cost(&outcome.term),
                    term: outcome.term,
                });
            }
        }
        Results { records }
    }
}
