/* Query Optimization Rewrite Rules */

use crate::querylang::QueryLang;
use egg::{Analysis, Rewrite, rewrite};

/// A basic set of query optimization rules: logical rewrites that reorder the query, and implementation
/// rules that introduce physical operators for each logical operator.
///
/// The rules are generic over the analysis so the same rules can saturate e-graphs with or without the
/// frontier analysis. There is no schema, so the logical rewrites do not check which table the predicate
/// columns belong to (e.g. associativity may move a join predicate away from its table).
pub fn rules<N: Analysis<QueryLang>>() -> Vec<Rewrite<QueryLang, N>> {
    vec![
        // Logical rewrites
        rewrite!("join-commute"; "(JOIN ?a ?b ?p)" => "(JOIN ?b ?a ?p)"),
        rewrite!("join-assoc"; "(JOIN (JOIN ?a ?b ?p) ?c ?q)" => "(JOIN ?a (JOIN ?b ?c ?q) ?p)"),
        rewrite!("select-commute"; "(SELECT (SELECT ?t ?a) ?b)" => "(SELECT (SELECT ?t ?b) ?a)"),
        rewrite!("select-pushdown"; "(SELECT (JOIN ?a ?b ?p) ?c)" => "(JOIN (SELECT ?a ?c) ?b ?p)"),
        // Access paths: a sequential scan always works, and an index scan on the column a select or join uses
        rewrite!("seq-scan"; "(SCAN (T- ?t))" => "(SEQ_SCAN ?t)"),
        rewrite!("index-scan"; "(SCAN (I- ?t ?c))" => "(INDEX_SCAN (I- ?t ?c))"),
        rewrite!("select-index-scan";
            "(SELECT (SCAN (T- ?t)) (C- ?c))" => "(SELECT (INDEX_SCAN (I- ?t ?c)) (C- ?c))"),
        rewrite!("join-index-scan";
            "(JOIN (SCAN (T- ?t)) ?r (C- ?c))" => "(JOIN (INDEX_SCAN (I- ?t ?c)) ?r (C- ?c))"),
        // Physical implementations of select and join
        rewrite!("exhaustive-select"; "(SELECT ?t ?c)" => "(X_SELECT ?t ?c)"),
        rewrite!("sort-select"; "(SELECT ?t ?c)" => "(S_SELECT ?t ?c)"),
        rewrite!("nested-loop-join"; "(JOIN ?a ?b ?p)" => "(NL_JOIN ?a ?b ?p)"),
        rewrite!("merge-sort-join"; "(JOIN ?a ?b ?p)" => "(MS_JOIN ?a ?b ?p)"),
    ]
}
