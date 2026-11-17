use compositional::egraph::analysis::Analysis;
use compositional::egraph::core::{ClassId, EGraph, Node, NodeClass, NodeId};
use compositional::egraph::language::{Language, Pattern, Term};
use compositional::egraph::rewrite::Rewrite;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Op {
    Add,
    Mul,
    Sym(&'static str),
}

impl Language for Op {
    fn matches(&self, other: &Self) -> bool {
        self == other
    }
    fn arity(&self) -> usize {
        match self {
            Op::Add | Op::Mul => 2,
            Op::Sym(_) => 0,
        }
    }
}

/// Smallest term size in each class.
#[derive(Clone, Debug)]
struct Size;

impl Analysis<Op> for Size {
    type Data = usize;
    fn make(egraph: &mut EGraph<Op, Self>, enode: &Node<Op>, _class: ClassId) -> usize {
        1 + enode
            .children
            .iter()
            .map(|&c| egraph.class(c).data)
            .sum::<usize>()
    }
    fn merge(&mut self, a: &mut usize, b: usize) -> (bool, bool) {
        let (a_changed, b_changed) = (b < *a, *a < b);
        *a = (*a).min(b);
        (a_changed, b_changed)
    }
}

type EG = EGraph<Op, Size>;

fn sym(s: &'static str) -> Term<Op> {
    Term::new(Op::Sym(s), vec![])
}
fn add(a: Term<Op>, b: Term<Op>) -> Term<Op> {
    Term::new(Op::Add, vec![a, b])
}
fn mul(a: Term<Op>, b: Term<Op>) -> Term<Op> {
    Term::new(Op::Mul, vec![a, b])
}
fn v(name: &str) -> Pattern<Op> {
    Pattern::var(name)
}
fn padd(a: Pattern<Op>, b: Pattern<Op>) -> Pattern<Op> {
    Pattern::new(Op::Add, vec![a, b])
}

fn class_of(egraph: &mut EG, term: &Term<Op>) -> ClassId {
    egraph.add_term(term).class()
}

#[test]
fn add_term_is_hashconsed() {
    let mut eg = EG::new(Size);
    let t = add(sym("x"), sym("y"));
    let n1 = eg.add_term(&t);
    let n2 = eg.add_term(&t);
    assert_eq!(n1, n2);
    assert_eq!(eg.nodes.len(), 3);
    assert_eq!(eg.classes().count(), 3);
    assert_eq!(eg.class(n1.class()).data, 3);
}

#[test]
fn lookups() {
    let mut eg = EG::new(Size);
    let t = add(sym("x"), sym("y"));
    let node_class = eg.add_term(&t);
    let node = node_class.node();
    let class = node_class.class();

    assert_eq!(eg.lookup_term(&t).unwrap().node(), node);
    assert_eq!(eg.lookup_term(&add(sym("y"), sym("x"))), None);
    assert_eq!(eg.lookup_term(&sym("z")), None);
    assert!(eg.class_contains(class, &t));
    assert!(!eg.class_contains(class, &sym("x")));
    assert!(eg.node_represents(node, &t));

    let x = eg.lookup_term(&sym("x")).unwrap();
    assert_eq!(
        eg.lookup_node(&Node::new(Op::Add, vec![x.class(), x.class()])),
        None
    );
}

#[test]
fn add_node_to_existing_class() {
    let mut eg = EG::new(Size);
    let x = class_of(&mut eg, &sym("x"));
    let y = eg.add_node_to(Node::new(Op::Sym("y"), vec![]), x);
    assert_eq!(y.class(), eg.find(x));
    eg.rebuild();
    assert_eq!(eg.node_class(y.node()).class(), eg.find(x));
    assert!(eg.class_contains(x, &sym("y")));
}

#[test]
fn rebuild_restores_congruence_and_drops_duplicates() {
    let mut eg = EG::new(Size);
    // (x + z) + z and (y + z) + z become equal once x = y.
    let a = eg.add_term(&add(add(sym("x"), sym("z")), sym("z"))).node();
    let b = eg.add_term(&add(add(sym("y"), sym("z")), sym("z"))).node();
    let x = class_of(&mut eg, &sym("x"));
    let y = class_of(&mut eg, &sym("y"));
    assert_ne!(eg.node_class(a).class(), eg.node_class(b).class());

    assert!(eg.union(x, y));
    assert!(!eg.union(x, y));
    eg.rebuild();

    // Same class, and the same (forwarded) node.
    assert_eq!(eg.node_class(a), eg.node_class(b));
    // The duplicate nodes were dropped and their ids forwarded.
    assert_eq!(eg.find_node(a), eg.find_node(b));
    assert_eq!(eg.get_node(a), eg.get_node(b));
    assert_eq!(eg.class(eg.node_class(a).class()).nodes.len(), 1);
    // Every node in a class list is canonical and agrees with the hashcons.
    for class in eg.classes() {
        for &n in &class.nodes {
            assert_eq!(eg.find_node(n), n);
            assert_eq!(eg.node_class(n), NodeClass::new(n, class.id));
        }
    }
    assert!(eg.class_contains(
        eg.node_class(a).class(),
        &add(add(sym("y"), sym("z")), sym("z"))
    ));
}

/// A node dropped as a duplicate stays in its children's parent sets, so a later union can
/// put its (forwarded) id back on the worklist. Rebuild must follow the forwarding.
#[test]
fn rebuild_handles_dropped_node_on_worklist() {
    let mut eg = EG::new(Size);
    let xx = eg.add_term(&add(sym("x"), sym("x"))).node();
    let yy = eg.add_term(&add(sym("y"), sym("y"))).node();
    let x = class_of(&mut eg, &sym("x"));
    let y = class_of(&mut eg, &sym("y"));
    eg.union(x, y);
    eg.rebuild();
    // One of x + x / y + y was dropped and forwarded to the other.
    assert_eq!(eg.find_node(xx), eg.find_node(yy));

    // Give z more parents than x, so x's class loses the union and its parents,
    // including the dropped node, go back on the worklist.
    for t in [
        add(sym("z"), sym("a")),
        add(sym("z"), sym("b")),
        add(sym("z"), sym("c")),
        mul(sym("z"), sym("d")),
    ] {
        eg.add_term(&t);
    }
    let z = class_of(&mut eg, &sym("z"));
    assert!(eg.class(z).parents.len() > eg.class(x).parents.len());
    eg.union(x, z);
    eg.rebuild();

    assert_eq!(eg.find(x), eg.find(z));
    assert_eq!(eg.node_class(xx), eg.node_class(yy));
    assert!(eg.class_contains(eg.node_class(xx).class(), &add(sym("z"), sym("z"))));
}

#[test]
fn analysis_is_repaired_upwards() {
    let mut eg = EG::new(Size);
    let root = class_of(&mut eg, &add(add(mul(sym("x"), sym("y")), sym("z")), sym("z")));
    assert_eq!(eg.class(root).data, 7);

    let xy = class_of(&mut eg, &mul(sym("x"), sym("y")));
    let w = class_of(&mut eg, &sym("w"));
    eg.union(xy, w);
    eg.rebuild();
    assert_eq!(eg.class(xy).data, 1);
    assert_eq!(eg.class(root).data, 5);
}

#[test]
fn ematching() {
    let mut eg = EG::new(Size);
    let xy = eg.add_term(&add(sym("x"), sym("y")));
    let xx = eg.add_term(&add(sym("x"), sym("x")));
    eg.union(xy.class(), xx.class());
    eg.rebuild();
    let class = eg.node_class(xy.node()).class();
    let x = eg.lookup_term(&sym("x")).unwrap().class();
    let y = eg.lookup_term(&sym("y")).unwrap().class();

    // Both nodes match (+ ?a ?b); only x + x matches the non-linear (+ ?a ?a).
    let mut matches: Vec<_> = eg
        .match_class(&padd(v("a"), v("b")), class)
        .into_iter()
        .map(|s| (s["a"], s["b"]))
        .collect();
    matches.sort();
    let mut expected = vec![(x, y), (x, x)];
    expected.sort();
    assert_eq!(matches, expected);
    assert_eq!(eg.match_class(&padd(v("a"), v("a")), class).len(), 1);

    // Matching at a node only uses that node as the root.
    assert_eq!(eg.match_node(&padd(v("a"), v("b")), xy.node()).len(), 1);
    assert!(eg.match_node(&padd(v("a"), v("a")), xy.node()).is_empty());

    // A nested pattern: the (+ x y) class appears as a child of the root.
    let root = eg.add_term(&add(add(sym("x"), sym("y")), sym("z"))).node();
    eg.rebuild();
    let nested = padd(padd(v("a"), v("b")), v("c"));
    assert_eq!(eg.match_node(&nested, root).len(), 2);
}

#[test]
fn try_rewrite_returns_new_nodes_in_class() {
    let mut eg = EG::new(Size);
    let comm = Rewrite::new("comm", padd(v("a"), v("b")), padd(v("b"), v("a")));
    let xy = eg.add_term(&add(sym("x"), sym("y"))).node();

    let new = eg.try_rewrite_node(&comm, xy);
    eg.rebuild();
    assert_eq!(new.len(), 1);
    assert!(eg.node_represents(new[0], &add(sym("y"), sym("x"))));
    assert_eq!(eg.node_class(new[0]).class(), eg.node_class(xy).class());

    // Applying again only re-derives existing nodes.
    assert!(eg.try_rewrite_node(&comm, xy).is_empty());
    assert!(eg.try_rewrite_node(&comm, new[0]).is_empty());
}

#[test]
#[should_panic(expected = "not bound by the search pattern")]
fn rewrite_rejects_unbound_apply_vars() {
    Rewrite::new("bad", v("a"), padd(v("a"), v("b")));
}

/// A plain eq-sat loop with commutativity and associativity reaches a fixpoint
/// in which all orderings of x + y + z are equal.
#[test]
fn eqsat_comm_assoc() {
    let mut eg = EG::new(Size);
    let rules = [
        Rewrite::new("comm", padd(v("a"), v("b")), padd(v("b"), v("a"))),
        Rewrite::new(
            "assoc",
            padd(v("a"), padd(v("b"), v("c"))),
            padd(padd(v("a"), v("b")), v("c")),
        ),
    ];
    let root = class_of(&mut eg, &add(add(sym("x"), sym("y")), sym("z")));

    let mut iterations = 0;
    loop {
        let before = (eg.nodes.len(), eg.classes().count());
        let nodes: Vec<NodeId> = eg.classes().flat_map(|c| c.nodes.clone()).collect();
        for &node in &nodes {
            for rule in &rules {
                eg.try_rewrite_node(rule, node);
            }
        }
        eg.rebuild();
        iterations += 1;
        if (eg.nodes.len(), eg.classes().count()) == before {
            break;
        }
        assert!(iterations < 20, "did not saturate");
    }

    for t in [
        add(sym("x"), add(sym("y"), sym("z"))),
        add(sym("z"), add(sym("y"), sym("x"))),
        add(add(sym("z"), sym("x")), sym("y")),
    ] {
        assert!(eg.class_contains(root, &t), "{t:?}");
    }
}
