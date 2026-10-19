//! Sanity check for `BasicTopDownAnalysis` using interval analysis over a tiny arithmetic language.

use compositional::lattice::{JoinLattice, Lattice, MeetLattice, WithTop};
use compositional::topdown::{BasicTopDownAnalysis, ComputeAnalysis, TopDownAnalysis};
use egg::*;
use std::cmp::Ordering;

define_language! {
    enum Arith {
        Num(i64),
        "+" = Add([Id; 2]),
        "-" = Sub([Id; 2]),
    }
}

fn rules() -> Vec<Rewrite<Arith, ()>> {
    vec![
        rewrite!("add-comm"; "(+ ?a ?b)" => "(+ ?b ?a)"),
        rewrite!("add-assoc"; "(+ ?a (+ ?b ?c))" => "(+ (+ ?a ?b) ?c)"),
        rewrite!("add-zero"; "(+ ?a 0)" => "?a"),
        rewrite!("sub-zero"; "(- ?a 0)" => "?a"),
        rewrite!("sub-self"; "(- ?a ?a)" => "0"),
        rewrite!("sub-add"; "(- (+ ?a ?b) ?b)" => "?a"),
    ]
}

/// Closed integer interval [lo, hi], ordered by inclusion.
/// Top is [i64::MIN, i64::MAX], meet is intersection, join is the (convex) union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Interval(i64, i64);

impl PartialOrd for Interval {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        let sub = o.0 <= self.0 && self.1 <= o.1;
        let sup = self.0 <= o.0 && o.1 <= self.1;
        match (sub, sup) {
            (true, true) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Less),
            (false, true) => Some(Ordering::Greater),
            (false, false) => None,
        }
    }
}

impl WithTop for Interval {
    fn top() -> Self {
        Interval(i64::MIN, i64::MAX)
    }
}

impl MeetLattice for Interval {
    fn meet(&self, o: &Self) -> Self {
        Interval(self.0.max(o.0), self.1.min(o.1))
    }
}

impl JoinLattice for Interval {
    fn join(&self, o: &Self) -> Self {
        Interval(self.0.min(o.0), self.1.max(o.1))
    }
}

impl Lattice for Interval {}

impl ComputeAnalysis<Arith, Interval> for BasicTopDownAnalysis<Arith, Interval> {
    fn compute_analysis(&mut self, enode: &Arith, c: &[Interval]) -> Interval {
        match enode {
            Arith::Num(n) => Interval(*n, *n),
            Arith::Add(_) => Interval(c[0].0.saturating_add(c[1].0), c[0].1.saturating_add(c[1].1)),
            Arith::Sub(_) => Interval(c[0].0.saturating_sub(c[1].1), c[0].1.saturating_sub(c[1].0)),
        }
    }
}

/// Saturate `expr` under `rules()`, then query the root class with `threshold`.
fn check(expr: &str, threshold: Interval, expect_some: bool) {
    let runner = Runner::default()
        .with_iter_limit(10)
        .with_node_limit(10_000)
        .with_expr(&expr.parse().unwrap())
        .run(&rules());
    let root = runner.egraph.find(runner.roots[0]);

    let mut analysis = BasicTopDownAnalysis::new(runner.egraph);
    let result = analysis.query_class(&root, threshold);
    assert_eq!(
        result.is_some(),
        expect_some,
        "{expr} with threshold {threshold:?}: got {result:?}"
    );
}

#[test]
fn constant() {
    check("5", Interval(0, 10), true);
    check("5", Interval(6, 10), false);
}

#[test]
fn add_sub() {
    // (1 + 2) - 4 = -1
    check("(- (+ 1 2) 4)", Interval(-5, 5), true);
    check("(- (+ 1 2) 4)", Interval(0, 5), false);
}

#[test]
fn rewrites_create_cycles() {
    // x - x and x + 0 put cycles in the e-graph; the value is still 7.
    check("(+ (- (+ 3 4) 4) (+ 4 0))", Interval(6, 8), true);
    check("(+ (- 9 9) (+ 7 0))", Interval(0, 6), false);
}
