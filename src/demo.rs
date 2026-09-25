/* Demos: saturate a query with the frontier analysis and compare extractors */

use std::cmp::Ordering;
use std::fmt;
use std::time::{Duration, Instant};

use crate::analysis::FrontierAnalysis;
use crate::extract::FrontierExtractor;
use crate::querylang::*;
use crate::rules::rules;
use egg::*;

/// Timings keep the fastest of several repetitions, which also hides one-time warmup costs
const SATURATE_REPS: usize = 3;
const EXTRACT_REPS: usize = 10;

/// A small catalog: a large table with two indexes, a small table with one, and a mid-sized table with none
pub fn catalog() -> Catalog {
    Catalog::new()
        .with_table("emp".into(), 10_000)
        .with_table("dept".into(), 100)
        .with_table("proj".into(), 1_000)
        .with_index("emp".into(), "dept_id".into())
        .with_index("emp".into(), "salary".into())
        .with_index("dept".into(), "dept_id".into())
}

/// An example query to optimize. Build one with [`Demo::new`], optionally adjust it, and call [`Demo::run`].
pub struct Demo {
    name: String,
    query: RecExpr<QueryLang>,
    catalog: Catalog,
    iter_limit: usize,
    node_limit: usize,
    time_limit: Duration,
}

impl Demo {
    /// A demo for `query` (an s-expression) over the default [`catalog`]
    pub fn new(name: &str, query: &str) -> Self {
        Self {
            name: name.into(),
            query: query
                .parse()
                .unwrap_or_else(|e| panic!("bad query {query}: {e}")),
            catalog: catalog(),
            iter_limit: 30,
            node_limit: 10_000,
            time_limit: Duration::from_secs(5),
        }
    }

    pub fn with_catalog(mut self, catalog: Catalog) -> Self {
        self.catalog = catalog;
        self
    }

    pub fn with_iter_limit(mut self, iter_limit: usize) -> Self {
        self.iter_limit = iter_limit;
        self
    }

    pub fn with_node_limit(mut self, node_limit: usize) -> Self {
        self.node_limit = node_limit;
        self
    }

    pub fn with_time_limit(mut self, time_limit: Duration) -> Self {
        self.time_limit = time_limit;
        self
    }

    fn runner<N: Analysis<QueryLang>>(&self, analysis: N) -> Runner<QueryLang, N> {
        Runner::new(analysis)
            .with_iter_limit(self.iter_limit)
            .with_node_limit(self.node_limit)
            .with_time_limit(self.time_limit)
            .with_expr(&self.query)
    }

    /// Saturates the query with and without the frontier analysis, then extracts from the analyzed
    /// e-graph with both the frontier extractor and egg's extractor.
    pub fn run(&self) -> Report {
        // Saturate without any analysis, to show what the frontier analysis adds to saturation time
        let (saturate_plain, plain) = time(SATURATE_REPS, || self.runner(()).run(&rules()));
        let (saturate_frontier, runner) = time(SATURATE_REPS, || {
            self.runner(FrontierAnalysis::new(self.catalog.clone()))
                .run(&rules())
        });

        let egraph = &runner.egraph;
        let root = egraph.find(runner.roots[0]);
        let sizes: Vec<usize> = egraph.classes().map(|c| c.data.0.len()).collect();

        let (frontier_time, frontier_plans) = time(EXTRACT_REPS, || {
            FrontierExtractor::new(egraph).find_frontier(root)
        });

        // egg's extractor computes its costs from scratch (in `new`), so that is part of its time
        let (egg_time, (_, egg_plan)) = time(EXTRACT_REPS, || {
            Extractor::new(egraph, ScalarCostFn(self.catalog.clone())).find_best(root)
        });
        let egg_cost = self.catalog.clone().cost_rec(&egg_plan);
        let egg_covered = frontier_plans.iter().any(|(bound, _)| *bound <= egg_cost);

        Report {
            name: self.name.clone(),
            query: self.query.clone(),
            stop_reason: runner.stop_reason.clone(),
            iterations: runner.iterations.len(),
            nodes: egraph.total_number_of_nodes(),
            classes: egraph.number_of_classes(),
            plain_nodes: plain.egraph.total_number_of_nodes(),
            saturate_plain,
            saturate_frontier,
            frontier_entries: sizes.iter().sum(),
            frontier_max: sizes.iter().copied().max().unwrap_or(0),
            frontier_time,
            frontier_plans,
            egg_time,
            egg_plan: (egg_cost, egg_plan),
            egg_covered,
        }
    }
}

/// Runs `f` `reps` times, returning the fastest time and the last result
fn time<T>(reps: usize, mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut best = Duration::MAX;
    let mut result = None;
    for _ in 0..reps {
        let start = Instant::now();
        result = Some(f());
        best = best.min(start.elapsed());
    }
    (best, result.unwrap())
}

/// Statistics and extracted plans from running a [`Demo`]. Print it with `{}`.
pub struct Report {
    pub name: String,
    pub query: RecExpr<QueryLang>,
    pub stop_reason: Option<StopReason>,
    pub iterations: usize,
    pub nodes: usize,
    pub classes: usize,
    /// Size of the e-graph saturated without the analysis (should match `nodes`)
    pub plain_nodes: usize,
    pub saturate_plain: Duration,
    pub saturate_frontier: Duration,
    /// Total number of frontier entries over all classes
    pub frontier_entries: usize,
    /// Largest frontier of any class
    pub frontier_max: usize,
    pub frontier_time: Duration,
    /// Every non-dominated plan for the root, with its cost bound
    pub frontier_plans: Vec<(CostProperties, RecExpr<QueryLang>)>,
    pub egg_time: Duration,
    /// egg's single best plan by scalar cost, with its actual cost
    pub egg_plan: (CostProperties, RecExpr<QueryLang>),
    /// Whether some frontier plan is at least as good as egg's plan
    pub egg_covered: bool,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== {} ===", self.name)?;
        writeln!(f, "query:       {}", self.query)?;
        writeln!(
            f,
            "e-graph:     {} nodes, {} classes after {} iterations ({:?})",
            self.nodes,
            self.classes,
            self.iterations,
            self.stop_reason.as_ref().unwrap()
        )?;
        writeln!(
            f,
            "saturation:  {:?} with frontier analysis, {:?} without ({} nodes)",
            self.saturate_frontier, self.saturate_plain, self.plain_nodes
        )?;
        writeln!(
            f,
            "frontiers:   {} entries total, {:.1} per class on average, at most {}",
            self.frontier_entries,
            self.frontier_entries as f64 / self.classes as f64,
            self.frontier_max
        )?;
        writeln!(
            f,
            "extraction:  frontier {:?} ({} plan{}), egg {:?} (1 plan)",
            self.frontier_time,
            self.frontier_plans.len(),
            if self.frontier_plans.len() == 1 {
                ""
            } else {
                "s"
            },
            self.egg_time
        )?;
        writeln!(f, "frontier plans:")?;
        for (cost, plan) in &self.frontier_plans {
            writeln!(f, "  {}  {}", fmt_cost(cost), plan)?;
        }
        writeln!(
            f,
            "egg plan (covered by frontier: {}):",
            if self.egg_covered { "yes" } else { "no" }
        )?;
        writeln!(f, "  {}  {}", fmt_cost(&self.egg_plan.0), self.egg_plan.1)
    }
}

fn fmt_cost(c: &CostProperties) -> String {
    if c.is_top() {
        return format!("{:<52}", "top");
    }
    format!(
        "cost {:>9}  rows {:>9}  {:<14} {:<12}",
        c.cost,
        c.rows,
        c.index
            .as_ref()
            .map_or("unsorted".into(), |i| format!("sorted {i}")),
        if c.materialized { "materialized" } else { "" }
    )
}

/// egg's extractor needs a total order on costs (it panics on incomparable ones), so the baseline compares
/// only the scalar `cost` field. It picks one plan per class and cannot see sort order or materialization.
#[derive(Debug, Clone)]
pub struct ScalarCost(pub CostProperties);

impl PartialEq for ScalarCost {
    fn eq(&self, other: &Self) -> bool {
        self.0.cost == other.0.cost
    }
}

impl PartialOrd for ScalarCost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.0.cost.partial_cmp(&other.0.cost)
    }
}

/// Our cost function, with costs compared as [`ScalarCost`] for egg's extractor
pub struct ScalarCostFn(pub Catalog);

impl CostFunction<QueryLang> for ScalarCostFn {
    type Cost = ScalarCost;

    fn cost<C>(&mut self, enode: &QueryLang, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        let args: Vec<CostProperties> = enode.children().iter().map(|&c| costs(c).0).collect();
        ScalarCost(self.0.op_cost(enode, &args))
    }
}
