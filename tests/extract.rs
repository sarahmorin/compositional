use compositional::define_language;
use compositional::egraph::analysis::Analysis;
use compositional::egraph::core::{ClassId, EGraph, Node, NodeId};
use compositional::egraph::extract::{CostFunction, Extractor};
use compositional::egraph::language::Term;
use compositional::egraph::parse::parse_term;
use std::cmp::Ordering;

define_language! {
    enum Lang {
        "+" = Add(2),
        "*" = Mul(2),
        "f" = F(1),
        "g" = G(1),
        "k" = K(1),
        "m" = M(1),
        "n" = N(1),
        "h" = H(1),
        Num(u64),
        Sym(String),
    }
}

#[derive(Clone, Debug)]
struct NoAnalysis;

impl Analysis<Lang> for NoAnalysis {
    type Data = ();
    fn make(_: &mut EGraph<Lang, Self>, _: &Node<Lang>, _: NodeId) {}
    fn merge(&mut self, _: &mut (), _: ()) -> (bool, bool) {
        (false, false)
    }
}

type EG = EGraph<Lang, NoAnalysis>;

/// Numbers cost their value, symbols cost 1, `*` costs 4 plus its children, and every other
/// operator costs 1 plus its children.
struct Weighted;

impl CostFunction<Lang, u64> for Weighted {
    fn cost(&self, op: &Lang, children: &[u64]) -> u64 {
        let own = match op {
            Lang::Num(n) => *n,
            Lang::Mul => 4,
            _ => 1,
        };
        own + children.iter().sum::<u64>()
    }
}

fn term(s: &str) -> Term<Lang> {
    parse_term(s).unwrap()
}

fn add(eg: &mut EG, s: &str) -> ClassId {
    eg.add_term(&term(s)).0.class()
}

/// Adds `op(child)` and merges it into `class`.
fn add_unary_to(eg: &mut EG, op: Lang, child: ClassId, class: ClassId) -> NodeId {
    eg.add_node_to(Node::new(op, vec![child]), class).node()
}

/// Extracts from `class` and checks the result is in the class and its cost is consistent.
fn extract(eg: &EG, class: ClassId) -> (u64, String) {
    let (cost, t) = Extractor::new(eg, &Weighted).extract_class(class).unwrap();
    assert!(eg.class_contains(class, &t), "{t} not in class");
    assert_eq!(Weighted.term_cost(&t), cost);
    (cost, t.to_string())
}

#[test]
fn acyclic_picks_cheaper_term() {
    let mut eg = EG::new(NoAnalysis);
    let mul = add(&mut eg, "*(x, 2)");
    let sum = add(&mut eg, "+(x, x)");
    eg.union(mul, sum);
    eg.rebuild();
    assert_eq!(extract(&eg, mul), (3, "+(x, x)".into()));

    // Cheapest children are chosen independently below the root.
    let outer = add(&mut eg, "f(*(x, 2))");
    eg.rebuild();
    assert_eq!(extract(&eg, outer), (4, "f(+(x, x))".into()));
}

#[test]
fn self_loop() {
    // x = +(x, 0): the class contains infinitely many terms, the cheapest is x.
    let mut eg = EG::new(NoAnalysis);
    let x = add(&mut eg, "x");
    let plus = eg.add_term(&term("+(x, 0)"));
    eg.union(x, plus.0.class());
    eg.rebuild();
    assert_eq!(extract(&eg, x), (1, "x".into()));

    // Rooted at the + node, its child class must use x (the + node is blocked).
    let (cost, t) = Extractor::new(&eg, &Weighted)
        .extract_node(plus.0.node())
        .unwrap();
    assert_eq!((cost, t.to_string()), (2, "+(x, 0)".into()));
    assert!(eg.node_represents(plus.0.node(), &t));
}

#[test]
fn two_class_cycle_with_exit() {
    // A = {a, f(B)}, B = {g(A)}
    let mut eg = EG::new(NoAnalysis);
    let a = add(&mut eg, "a");
    let b = add(&mut eg, "g(a)");
    let fb = eg.add_node(Node::new(Lang::F, vec![b])).0.class();
    eg.union(a, fb);
    eg.rebuild();
    assert_eq!(extract(&eg, a), (1, "a".into()));
    assert_eq!(extract(&eg, b), (2, "g(a)".into()));
}

/// A = {10, f(B)}, B = {100, g(A), k(X)}, X = {50, m(B)} with unary ops costing 1.
/// True bests: A = 10, B = g(10) = 11, X = m(g(10)) = 12.
///
/// While extracting B, X is first computed with g and k blocked, giving 50. That result must
/// not be memoized past the frames it depends on, or a later lookup of X would return 50.
#[test]
fn blocked_results_are_not_reused_out_of_context() {
    let mut eg = EG::new(NoAnalysis);
    let a = add(&mut eg, "10");
    let b = add(&mut eg, "100");
    let x = add(&mut eg, "50");
    add_unary_to(&mut eg, Lang::F, b, a);
    add_unary_to(&mut eg, Lang::G, a, b);
    add_unary_to(&mut eg, Lang::K, x, b);
    add_unary_to(&mut eg, Lang::M, b, x);
    eg.rebuild();

    // One extractor for all three, so the memo is shared across calls.
    let mut ex = Extractor::new(&eg, &Weighted);
    let mut get = |class| {
        let (cost, t) = ex.extract_class(class).unwrap();
        (cost, t.to_string())
    };
    assert_eq!(get(b), (11, "g(10)".into()));
    assert_eq!(get(x), (12, "m(g(10))".into()));
    assert_eq!(get(a), (10, "10".into()));

    // Same answers from fresh extractors, in each order.
    assert_eq!(extract(&eg, x), (12, "m(g(10))".into()));
    assert_eq!(extract(&eg, a), (10, "10".into()));
}

/// Componentwise order on pairs: (1, 2) and (2, 1) are incomparable.
#[derive(Clone, Debug, PartialEq)]
struct Pair(u64, u64);

impl PartialOrd for Pair {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self.0.cmp(&other.0), self.1.cmp(&other.1)) {
            (a, b) if a == b => Some(a),
            (Ordering::Equal, o) | (o, Ordering::Equal) => Some(o),
            _ => None,
        }
    }
}

struct PairCost;

impl CostFunction<Lang, Pair> for PairCost {
    fn cost(&self, op: &Lang, children: &[Pair]) -> Pair {
        let own = match op {
            Lang::Sym(s) if s == "p" => Pair(1, 2),
            Lang::Sym(s) if s == "q" => Pair(2, 1),
            Lang::Sym(s) if s == "r" => Pair(3, 3),
            _ => Pair(1, 1),
        };
        children
            .iter()
            .fold(own, |acc, c| Pair(acc.0 + c.0, acc.1 + c.1))
    }
}

#[test]
fn incomparable_costs_keep_first_candidate() {
    let mut eg = EG::new(NoAnalysis);
    let p = add(&mut eg, "p");
    let q = add(&mut eg, "q");
    let r = add(&mut eg, "r");
    eg.union(p, q);
    eg.union(p, r);
    eg.rebuild();

    // r is dominated; p and q are incomparable, so whichever comes first in the class wins.
    let first = eg.get_node(eg.class(p).nodes[0]).root.clone();
    assert_ne!(first, Lang::Sym("r".into()));
    let (cost, t) = Extractor::new(&eg, &PairCost).extract_class(p).unwrap();
    assert_eq!(t.op, first);
    assert_eq!(PairCost.term_cost(&t), cost);
}

/// R = {+(B, X), 1000}, B = {100, k(Y)}, Y = {1, h(X)}, X = {50, m(B), n(R)}.
/// True bests: B = k(1) = 2, X = m(k(1)) = 3, so +(B, X) costs 6.
///
/// Extracting rooted at the + node (depth 0): under k (depth 1), X is computed with both the
/// + node and k blocked, giving 50 with deps {0, 1}. Once k pops, the + node's second child
/// reads X again. That entry must already be evicted (it depends on k), even though the frame
/// at depth 0 is still on the stack.
#[test]
fn results_are_evicted_when_their_deepest_dependency_pops() {
    let mut eg = EG::new(NoAnalysis);
    let r = add(&mut eg, "1000");
    let b = add(&mut eg, "100");
    let y = add(&mut eg, "1");
    let x = add(&mut eg, "50");
    add_unary_to(&mut eg, Lang::K, y, b);
    add_unary_to(&mut eg, Lang::H, x, y);
    add_unary_to(&mut eg, Lang::M, b, x);
    add_unary_to(&mut eg, Lang::N, r, x);
    let plus = eg.add_node_to(Node::new(Lang::Add, vec![b, x]), r).node();
    eg.rebuild();

    let (cost, t) = Extractor::new(&eg, &Weighted).extract_node(plus).unwrap();
    assert_eq!((cost, t.to_string()), (6, "+(k(1), m(k(1)))".into()));
    assert_eq!(extract(&eg, r), (6, "+(k(1), m(k(1)))".into()));
}
