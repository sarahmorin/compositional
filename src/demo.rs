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
            plain: GraphStats::new(&plain),
            frontier: GraphStats::new(&runner),
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

/// Size of a saturated e-graph and why saturation stopped
pub struct GraphStats {
    pub nodes: usize,
    pub classes: usize,
    pub iterations: usize,
    pub stop_reason: StopReason,
}

impl GraphStats {
    fn new<N: Analysis<QueryLang>>(runner: &Runner<QueryLang, N>) -> Self {
        Self {
            nodes: runner.egraph.total_number_of_nodes(),
            classes: runner.egraph.number_of_classes(),
            iterations: runner.iterations.len(),
            stop_reason: runner.stop_reason.clone().unwrap(),
        }
    }
}

/// Statistics and extracted plans from running a [`Demo`]. Print it with `{}`.
///
/// The egg pipeline saturates without an analysis and then runs egg's extractor; the frontier pipeline
/// saturates with the frontier analysis and then runs the frontier extractor.
pub struct Report {
    pub name: String,
    pub query: RecExpr<QueryLang>,
    /// E-graph saturated without the analysis
    pub plain: GraphStats,
    /// E-graph saturated with the frontier analysis (should match `plain`)
    pub frontier: GraphStats,
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
        let title = format!("── {} ", self.name);
        writeln!(f, "{title:─<100}")?;
        writeln!(f, "query  {}", self.query)?;
        writeln!(f)?;

        // E-graph sizes
        writeln!(
            f,
            "{:<10} {:>8} {:>8} {:>6}  {}",
            "e-graph", "nodes", "classes", "iters", "stopped"
        )?;
        for (label, g) in [("egg", &self.plain), ("frontier", &self.frontier)] {
            writeln!(
                f,
                "{:<10} {:>8} {:>8} {:>6}  {:?}",
                label,
                sep(g.nodes),
                sep(g.classes),
                g.iterations,
                g.stop_reason
            )?;
        }
        writeln!(
            f,
            "frontier entries: {} total, {:.1} per class, at most {}",
            sep(self.frontier_entries),
            self.frontier_entries as f64 / self.frontier.classes as f64,
            self.frontier_max
        )?;
        writeln!(f)?;

        // Timings
        let egg_total = self.saturate_plain + self.egg_time;
        let frontier_total = self.saturate_frontier + self.frontier_time;
        writeln!(
            f,
            "{:<10} {:>11} {:>11} {:>11}",
            "time", "saturate", "extract", "total"
        )?;
        for (label, saturate, extract, total) in [
            ("egg", self.saturate_plain, self.egg_time, egg_total),
            (
                "frontier",
                self.saturate_frontier,
                self.frontier_time,
                frontier_total,
            ),
        ] {
            writeln!(
                f,
                "{:<10} {:>11} {:>11} {:>11}",
                label,
                fmt_duration(saturate),
                fmt_duration(extract),
                fmt_duration(total)
            )?;
        }
        writeln!(
            f,
            "{:<10} {:>11} {:>11} {:>11}",
            "ratio",
            fmt_ratio(self.saturate_frontier, self.saturate_plain),
            fmt_ratio(self.frontier_time, self.egg_time),
            fmt_ratio(frontier_total, egg_total)
        )?;
        writeln!(f)?;

        // Plans
        writeln!(
            f,
            "{:<10} {:>13} {:>15} {:<10} {:<5} {}",
            "plan", "cost", "rows", "sorted on", "mat", "expression"
        )?;
        for (cost, plan) in &self.frontier_plans {
            writeln!(f, "{:<10} {} {}", "frontier", fmt_cost(cost), plan)?;
        }
        let covered = if self.egg_covered {
            "covered by frontier"
        } else {
            "NOT covered by frontier"
        };
        writeln!(
            f,
            "{:<10} {} {}  ({covered})",
            "egg",
            fmt_cost(&self.egg_plan.0),
            self.egg_plan.1
        )
    }
}

/// Cost columns for the plan table: cost, rows, sort order and materialization
fn fmt_cost(c: &CostProperties) -> String {
    if c.is_top() {
        return format!("{:>13} {:>15} {:<10} {:<5}", "top", "", "", "");
    }
    format!(
        "{:>13} {:>15} {:<10} {:<5}",
        sep(c.cost),
        sep(c.rows),
        c.index.as_deref().unwrap_or("-"),
        if c.materialized { "yes" } else { "no" }
    )
}

/// A duration with a unit suited to its size, e.g. `12.3 µs` or `4.56 ms`
fn fmt_duration(d: Duration) -> String {
    let ns = d.as_nanos() as f64;
    if ns < 1e3 {
        format!("{ns:.0} ns")
    } else if ns < 1e6 {
        format!("{:.1} µs", ns / 1e3)
    } else if ns < 1e9 {
        format!("{:.2} ms", ns / 1e6)
    } else {
        format!("{:.2} s", ns / 1e9)
    }
}

/// How many times longer `frontier` took than `egg`
fn fmt_ratio(frontier: Duration, egg: Duration) -> String {
    format!("{:.2}x", frontier.as_secs_f64() / egg.as_secs_f64())
}

/// A number with thousands separators, e.g. `1,000,000`
fn sep(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(d);
    }
    out
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
