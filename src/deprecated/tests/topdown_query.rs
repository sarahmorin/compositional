//! Top-down frontier analysis over the query language, using the existing `Catalog::op_cost` cost function.

use compositional::querylang::{Catalog, QueryLang};
use compositional::rules::rules;
use compositional::topdown::{BasicTopDownAnalysis, TopDownAnalysis};
use compositional::topdown_query::{Frontier, FrontierCost};
use egg::*;

/// Same catalog as the demo: a large table with two indexes, a small table with one, and a mid-sized table with none
fn catalog() -> Catalog {
    Catalog::new()
        .with_table("emp".into(), 10_000)
        .with_table("dept".into(), 100)
        .with_table("proj".into(), 1_000)
        .with_index("emp".into(), "dept_id".into())
        .with_index("emp".into(), "salary".into())
        .with_index("dept".into(), "dept_id".into())
}

/// A threshold asking for a plan at least as good as (materialized, index, cost)
fn threshold(materialized: bool, index: Option<&str>, cost: usize) -> Frontier {
    Frontier::threshold(materialized, index, cost)
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
    let mut analysis = BasicTopDownAnalysis::new(egraph, FrontierCost::new(catalog()));
    let result = analysis.query_class(&root, threshold);
    (result, analysis.compute().calls)
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
