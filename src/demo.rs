/* Demos: saturate a query and compare egg's extractor, the bottom-up frontier analysis, and the top-down analysis */

use std::cmp::Ordering;
use std::fmt;
use std::time::{Duration, Instant};

use crate::analysis::FrontierAnalysis;
use crate::extract::FrontierExtractor;
use crate::lattice::WithTop;
use crate::querylang::*;
use crate::rules::rules;
use crate::topdown::{BasicTopDownAnalysis, TopDownAnalysis};
use crate::topdown_query::{Frontier, FrontierCost};
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
    /// e-graph with both the frontier extractor and egg's extractor, and queries the plain e-graph
    /// with the top-down analysis under three thresholds.
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

        // Top-down runs on the plain e-graph, so its saturation time is the plain one
        let plain_root = plain.egraph.find(plain.roots[0]);
        let topdown = vec![
            self.run_topdown(&plain.egraph, plain_root, "top", "top", Frontier::top()),
            self.run_topdown(
                &plain.egraph,
                plain_root,
                "egg",
                &format!("cost < {}", sep(egg_cost.cost.saturating_add(1))),
                Frontier::threshold(false, None, egg_cost.cost.saturating_add(1)),
            ),
            self.run_topdown(
                &plain.egraph,
                plain_root,
                "full",
                "unsatisfiable",
                Frontier::threshold(true, Some("<no index>"), 0),
            ),
        ];
        // An unsatisfiable threshold computes the root's whole frontier, which should match the bottom-up one
        let topdown_matches = topdown[2].root.as_ref()
            == Some(&Frontier::from_costs(
                frontier_plans.iter().map(|(cost, _)| cost.clone()),
            ));

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
            topdown,
            topdown_matches,
        }
    }

    /// Queries the root of `egraph` top-down with `threshold`, keeping the fastest of several repetitions
    fn run_topdown(
        &self,
        egraph: &EGraph<QueryLang, ()>,
        root: Id,
        label: &'static str,
        threshold_label: &str,
        threshold: Frontier,
    ) -> TopDownRun {
        let mut best = Duration::MAX;
        let mut last = None;
        for _ in 0..EXTRACT_REPS {
            // The analysis takes ownership of the e-graph; clone it outside of the timed region
            let egraph = egraph.clone();
            let start = Instant::now();
            let mut analysis =
                BasicTopDownAnalysis::new(egraph, FrontierCost::new(self.catalog.clone()));
            let result = analysis.query_class(&root, threshold.clone());
            best = best.min(start.elapsed());
            last = Some((analysis, result));
        }
        let (analysis, result) = last.unwrap();
        TopDownRun {
            label,
            threshold: threshold_label.into(),
            time: best,
            calls: analysis.compute().calls,
            classes: egraph
                .classes()
                .filter(|c| analysis.get_analysis(&c.id).is_some())
                .count(),
            result,
            root: analysis.get_analysis(&root).cloned(),
        }
    }
}

/// One top-down query of the root class
pub struct TopDownRun {
    pub label: &'static str,
    /// Description of the threshold
    pub threshold: String,
    pub time: Duration,
    /// Number of `compute_analysis` calls
    pub calls: usize,
    /// Number of e-classes that got an analysis value
    pub classes: usize,
    /// What the query returned
    pub result: Option<Frontier>,
    /// The root's analysis value after the query (its full frontier if the threshold was unsatisfiable)
    pub root: Option<Frontier>,
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
/// saturates with the frontier analysis and then runs the frontier extractor; the top-down pipeline
/// saturates without an analysis and then queries the root with a threshold (costs only, no plans).
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
    /// Top-down queries with a top, egg-cost, and unsatisfiable threshold
    pub topdown: Vec<TopDownRun>,
    /// Whether the unsatisfiable top-down query computed the same root frontier as the bottom-up analysis
    pub topdown_matches: bool,
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
            "{:<16} {:>8} {:>8} {:>6}  {}",
            "e-graph", "nodes", "classes", "iters", "stopped"
        )?;
        for (label, g) in [("egg", &self.plain), ("frontier", &self.frontier)] {
            writeln!(
                f,
                "{:<16} {:>8} {:>8} {:>6}  {:?}",
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
        writeln!(
            f,
            "{:<16} {:>11} {:>11} {:>11} {:>8}",
            "time", "saturate", "extract", "total", "vs egg"
        )?;
        let mut rows = vec![
            ("egg".to_string(), self.saturate_plain, self.egg_time),
            (
                "frontier".to_string(),
                self.saturate_frontier,
                self.frontier_time,
            ),
        ];
        for run in &self.topdown {
            rows.push((
                format!("top-down {}", run.label),
                self.saturate_plain,
                run.time,
            ));
        }
        for (label, saturate, extract) in rows {
            writeln!(
                f,
                "{:<16} {:>11} {:>11} {:>11} {:>8}",
                label,
                fmt_duration(saturate),
                fmt_duration(extract),
                fmt_duration(saturate + extract),
                fmt_ratio(saturate + extract, egg_total)
            )?;
        }
        writeln!(f)?;

        // Top-down work: how much of the e-graph each query analyzed
        writeln!(
            f,
            "{:<16} {:<20} {:>8} {:>17}  {}",
            "top-down work", "threshold", "calls", "classes analyzed", "result"
        )?;
        for run in &self.topdown {
            let result = match &run.result {
                Some(r) => format!("satisfied, {} costs", r.costs().len()),
                None => "not satisfied".into(),
            };
            writeln!(
                f,
                "{:<16} {:<20} {:>8} {:>17}  {}",
                run.label,
                run.threshold,
                sep(run.calls),
                format!("{} / {}", sep(run.classes), sep(self.plain.classes)),
                result
            )?;
        }
        writeln!(
            f,
            "full top-down root frontier {} bottom-up frontier",
            if self.topdown_matches {
                "matches"
            } else {
                "DOES NOT MATCH"
            }
        )?;
        writeln!(f)?;

        // Plans
        writeln!(
            f,
            "{:<16} {:>13} {:>15} {:<10} {:<5} {}",
            "plan", "cost", "rows", "sorted on", "mat", "expression"
        )?;
        let covered = if self.egg_covered {
            "covered by frontier"
        } else {
            "NOT covered by frontier"
        };
        writeln!(
            f,
            "{:<16} {} {}  ({covered})",
            "egg",
            fmt_cost(&self.egg_plan.0),
            self.egg_plan.1
        )?;
        for (cost, plan) in &self.frontier_plans {
            writeln!(f, "{:<16} {} {}", "frontier", fmt_cost(cost), plan)?;
        }
        // Top-down has no plans yet, only the costs it returned (for the full run: the root's whole frontier)
        for run in &self.topdown {
            let costs = if run.label == "full" {
                &run.root
            } else {
                &run.result
            };
            for cost in costs.iter().flat_map(|c| c.costs()) {
                writeln!(
                    f,
                    "{:<16} {} (costs only)",
                    format!("top-down {}", run.label),
                    fmt_cost(cost)
                )?;
            }
        }
        Ok(())
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

/// How many times longer `other` took than `egg`
fn fmt_ratio(other: Duration, egg: Duration) -> String {
    format!("{:.2}x", other.as_secs_f64() / egg.as_secs_f64())
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
