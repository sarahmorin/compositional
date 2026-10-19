//! Top-down frontier analysis over the query language, using the existing `Catalog::op_cost` cost function.

use compositional::lattice::{MeetLattice, WithTop};
use compositional::querylang::{Catalog, CostProperties, QueryLang};
use compositional::rules::rules;
use compositional::topdown::{BasicTopDownAnalysis, ComputeAnalysis, TopDownAnalysis};
use compositional::upset::BackwardUpSet;
use egg::*;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::LazyLock;

/// Same catalog as the demo: a large table with two indexes, a small table with one, and a mid-sized table with none
static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    Catalog::new()
        .with_table("emp".into(), 10_000)
        .with_table("dept".into(), 100)
        .with_table("proj".into(), 1_000)
        .with_index("emp".into(), "dept_id".into())
        .with_index("emp".into(), "salary".into())
        .with_index("dept".into(), "dept_id".into())
});

thread_local! {
    /// Number of `compute_analysis` calls on this test's thread, to measure how much of the e-graph a query touches
    static COMPUTE_CALLS: Cell<usize> = const { Cell::new(0) };
}

/// Newtype so we can give `CostProperties` the `WithTop` and `Hash` impls `BackwardUpSet` needs.
/// Order and equality are those of `CostProperties`; the hash covers the same fields as its `Eq`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
struct Cost(CostProperties);

impl Hash for Cost {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (&self.0.index, self.0.materialized, self.0.cost).hash(state);
    }
}

impl WithTop for Cost {
    fn top() -> Self {
        Cost(CostProperties {
            tables: HashSet::new(),
            cols: HashSet::new(),
            rows: usize::MAX,
            index: None,
            materialized: false,
            cost: usize::MAX,
        })
    }
}

/// The analysis domain: the frontier of non-dominated costs of an e-class.
/// Newtype so it can be `Hash` and so we can implement `ComputeAnalysis` for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frontier(BackwardUpSet<Cost>);

impl Hash for Frontier {
    /// Order-independent hash of the frontier elements
    fn hash<H: Hasher>(&self, state: &mut H) {
        let mut acc = 0u64;
        for e in &self.0.elements {
            let mut h = DefaultHasher::new();
            e.hash(&mut h);
            acc ^= h.finish();
        }
        acc.hash(state);
    }
}

impl PartialOrd for Frontier {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        self.0.partial_cmp(&o.0)
    }
}

impl WithTop for Frontier {
    fn top() -> Self {
        Frontier(BackwardUpSet::top())
    }
}

impl MeetLattice for Frontier {
    fn meet(&self, o: &Self) -> Self {
        Frontier(self.0.meet(&o.0))
    }
}

impl ComputeAnalysis<QueryLang, Frontier> for BasicTopDownAnalysis<QueryLang, Frontier> {
    /// Lift `op_cost` to frontiers: apply it to every combination of one cost per child.
    fn compute_analysis(&mut self, enode: &QueryLang, children: &[Frontier]) -> Frontier {
        COMPUTE_CALLS.with(|c| c.set(c.get() + 1));
        let mut combos: Vec<Vec<CostProperties>> = vec![vec![]];
        for child in children {
            combos = combos
                .iter()
                .flat_map(|combo| {
                    child.0.elements.iter().map(move |c| {
                        let mut next = combo.clone();
                        next.push(c.0.clone());
                        next
                    })
                })
                .collect();
        }
        let costs = combos
            .iter()
            .map(|args| Cost(CATALOG.op_cost(enode, args)))
            .collect();
        Frontier(BackwardUpSet::new(costs))
    }
}

/// A threshold asking for a plan at least as good as (materialized, index, cost)
fn threshold(materialized: bool, index: Option<&str>, cost: usize) -> Frontier {
    let mut t = Cost::top();
    t.0.materialized = materialized;
    t.0.index = index.map(String::from);
    t.0.cost = cost;
    Frontier(vec![t].into())
}

/// Saturate `query` under `rules()`, returning the e-graph and its root class
fn saturate(query: &str) -> (EGraph<QueryLang, ()>, Id) {
    let runner = Runner::default()
        .with_iter_limit(30)
        .with_node_limit(10_000)
        .with_expr(&query.parse().unwrap())
        .run(&rules());
    let root = runner.egraph.find(runner.roots[0]);
    (runner.egraph, root)
}

/// Query the root class of `egraph` with `threshold`, returning the result and the number of `compute_analysis` calls
fn query(egraph: EGraph<QueryLang, ()>, root: Id, threshold: Frontier) -> (Option<Frontier>, usize) {
    COMPUTE_CALLS.with(|c| c.set(0));
    let result = BasicTopDownAnalysis::new(egraph).query_class(&root, threshold);
    (result, COMPUTE_CALLS.with(|c| c.get()))
}

/// Saturate `query` under `rules()`, then query the root class with `threshold`.
fn check(query_str: &str, threshold: Frontier, expect_some: bool) {
    let (egraph, root) = saturate(query_str);
    let (result, _) = query(egraph, root, threshold.clone());
    assert_eq!(
        result.is_some(),
        expect_some,
        "{query_str} with threshold {threshold:?}: got {result:?}"
    );
}

/// A loose threshold should be satisfied after computing only a small part of the e-graph.
/// We compare against an unsatisfiable threshold, which forces the analysis to explore everything reachable from the root.
fn check_early_stop(query_str: &str, loose: Frontier) {
    let (egraph, root) = saturate(query_str);
    let nodes = egraph.total_number_of_nodes();

    let (result, loose_calls) = query(egraph.clone(), root, loose);
    assert!(result.is_some(), "{query_str}: loose threshold not satisfied");

    let (result, full_calls) = query(egraph, root, threshold(true, Some("none"), 0));
    assert!(result.is_none(), "{query_str}: unsatisfiable threshold satisfied: {result:?}");

    println!("{query_str}\n  e-nodes: {nodes}, loose: {loose_calls} calls, full: {full_calls} calls");
    assert!(
        loose_calls * 5 < full_calls,
        "{query_str}: loose threshold took {loose_calls} calls vs {full_calls} for full exploration"
    );
}

#[test]
fn scan() {
    // Only plan: SEQ_SCAN, materialized, cost 10_000
    let q = "(SCAN (T- emp))";
    check(q, threshold(false, None, 10_000), true);
    check(q, threshold(false, None, 9_999), false);
}

#[test]
fn selection() {
    // Best plan: S_SELECT over INDEX_SCAN on salary, sorted on salary, cost 10_000 + 5_000
    let q = "(SELECT (SCAN (T- emp)) (C- salary))";
    check(q, threshold(false, None, 15_000), true);
    check(q, threshold(false, Some("salary"), 15_001), true);
    check(q, threshold(false, None, 14_999), false);
    check(q, threshold(false, Some("dept_id"), 100_000), false);
}

#[test]
fn join() {
    // Best plan: MS_JOIN over index scans on dept_id, sorted on dept_id, cost (10_000 + 100) * 2
    let q = "(JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id))";
    check(q, threshold(false, None, 20_200), true);
    check(q, threshold(false, Some("dept_id"), 20_201), true);
    check(q, threshold(false, None, 20_199), false);
}

// ---------- Early stopping ----------
// Joins and selections over joins saturate into e-graphs with many alternative plans per class.
// A loose threshold is satisfied by the first non-top plan found, so most of the e-graph is never analyzed.

/// Any plan that is not top
fn any_plan() -> Frontier {
    threshold(false, None, usize::MAX - 1)
}

#[test]
fn early_stop_three_way_join() {
    check_early_stop(
        "(JOIN (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (SCAN (T- proj)) (C- proj_id))",
        any_plan(),
    );
}

#[test]
fn early_stop_selection_over_join() {
    check_early_stop(
        "(SELECT (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (C- salary))",
        any_plan(),
    );
}

#[test]
fn early_stop_selection_over_three_way_join() {
    check_early_stop(
        "(SELECT (JOIN (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (SCAN (T- proj)) (C- proj_id)) (C- salary))",
        any_plan(),
    );
}

#[test]
fn early_stop_sorted_three_way_join() {
    // Stricter: the result must be sorted on dept_id, which needs an index scan or merge-sort join on the left,
    // so the first plan found is not enough, but we still stop well before exploring the whole e-graph.
    check_early_stop(
        "(JOIN (JOIN (SCAN (T- emp)) (SCAN (T- dept)) (C- dept_id)) (SCAN (T- proj)) (C- proj_id))",
        threshold(false, Some("dept_id"), usize::MAX - 1),
    );
}
