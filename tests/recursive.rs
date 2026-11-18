use compositional::ast::MinAST;
use compositional::define_language;
use compositional::egraph::core::{EGraph, NodeId};
use compositional::egraph::extract::CostFunction;
use compositional::egraph::language::{Pattern, Term};
use compositional::egraph::parse::{parse_pattern, parse_term};
use compositional::egraph::rewrite::Rewrite;
use compositional::explorer::Runner;
use compositional::recursive::RecursiveExplorer;
use compositional::rewrite;

define_language! {
    enum Lang {
        "+" = Add(2),
        "f" = F(1),
        "g" = G(1),
        "h" = H(1),
        "k" = K(1),
        "m" = M(1),
        "n" = N(1),
        Sym(String),
    }
}

type EG = EGraph<Lang, MinAST>;

fn term(s: &str) -> Term<Lang> {
    parse_term(s).unwrap()
}

fn pat(s: &str) -> Pattern<Lang> {
    parse_pattern(s).unwrap()
}

/// `g(k(x))` is cheaper as `m(x)`, but `f(g(x))` collapses to `x` only if `g` is kept.
fn rules() -> Vec<Rewrite<Lang>> {
    vec![
        rewrite!("g-k"; "g(k(?x))" => "m(?x)"),
        rewrite!("f-g"; "f(g(?x))" => "?x"),
    ]
}

/// Runs the recursive explorer on `expr` and returns the extracted cost and term.
fn optimize(expr: &str, rules: Vec<Rewrite<Lang>>) -> (usize, String, usize) {
    let mut explorer = RecursiveExplorer::new(MinAST)
        .with_expr(&term(expr))
        .with_rules(rules);
    explorer.run();
    let (cost, best) = explorer.extract();
    (cost, best.to_string(), explorer.get_metrics().tasks_processed)
}

/// Unrestricted equality saturation, for comparison.
fn saturate(expr: &str, rules: &[Rewrite<Lang>]) -> (usize, String) {
    let mut eg = EG::new(MinAST);
    let root = eg.add_term(&term(expr)).class();
    loop {
        let before = eg.num_nodes();
        let nodes: Vec<NodeId> = eg.classes().flat_map(|c| c.nodes.clone()).collect();
        for &node in &nodes {
            for rule in rules {
                eg.try_rewrite_node(rule, node);
            }
        }
        eg.rebuild();
        if eg.num_nodes() == before {
            break;
        }
    }
    let (cost, best) = eg.extract_min_ast(root);
    (cost, best.to_string())
}

#[test]
fn recursive_misses_rewrite_behind_optimal_child() {
    let (cost, best, _) = optimize("f(g(k(a)))", rules());
    assert_eq!((cost, best.as_str()), (3, "f(m(a))"));

    assert_eq!(saturate("f(g(k(a)))", &rules()), (2, "k(a)".to_string()));
}

#[test]
fn restricted_view_sees_best_child() {
    let mut rules = rules();
    rules.push(rewrite!("f-m"; "f(m(?x))" => "?x"));
    let (cost, best, _) = optimize("f(g(k(a)))", rules);
    assert_eq!((cost, best.as_str()), (1, "a"));
}

#[test]
fn rewrites_optimize_fresh_subterms() {
    // `h-n` builds a fresh `g(k(a))` class, which is optimized before the root sees it.
    let rules = vec![
        rewrite!("h-n"; "h(n(?x))" => "h(g(k(?x)))"),
        rewrite!("g-k"; "g(k(?x))" => "?x"),
    ];
    let (cost, best, _) = optimize("h(n(a))", rules);
    assert_eq!((cost, best.as_str()), (2, "h(a)"));
}

#[test]
fn match_restricted_only_sees_best_node() {
    let mut eg = EG::new(MinAST);
    let root = eg.add_term(&term("f(g(k(a)))")).node();
    let g = eg.lookup_term(&term("g(k(a))")).unwrap().class();
    let m = eg.add_term(&term("m(a)")).class();
    eg.union(g, m);
    eg.rebuild();

    let behind = pat("f(g(?x))");
    assert_eq!(eg.match_node(&behind, root).len(), 1);
    assert!(eg.match_restricted(&behind, root).is_none());

    let subst = eg.match_restricted(&pat("f(m(?x))"), root).unwrap();
    assert!(eg.class_contains(subst["x"], &term("a")));
}

#[test]
fn extraction_follows_repaired_analysis() {
    let mut eg = EG::new(MinAST);
    let root = eg.add_term(&term("f(g(k(a)))")).class();
    let g = eg.lookup_term(&term("g(k(a))")).unwrap().class();
    // Give m(n(b))'s class more parents so it leads the union, making g(k(a))'s class id
    // differ from the node id of g(k(a)).
    let m = eg.add_term(&term("m(n(b))")).class();
    eg.add_term(&term("h(m(n(b)))"));
    eg.add_term(&term("n(m(n(b)))"));
    eg.union(g, m);
    eg.rebuild();

    // Makes g(k(a)) cheaper, so its new cost is pushed up through repair_analysis.
    let k = eg.lookup_term(&term("k(a)")).unwrap().class();
    let a = eg.lookup_term(&term("a")).unwrap().class();
    eg.union(k, a);
    eg.rebuild();

    let (cost, best) = eg.extract_min_ast(root);
    assert_eq!((cost, best.to_string().as_str()), (3, "f(g(a))"));
    assert_eq!(MinAST.term_cost(&best), cost);
    assert!(eg.class_contains(root, &best));
}

#[test]
fn shared_subterms_are_optimized_once() {
    let (_, best, single) = optimize("f(g(k(a)))", rules());
    let (_, both, shared) = optimize("+(f(g(k(a))), f(g(k(a))))", rules());
    assert_eq!(both, format!("+({best}, {best})"));
    assert!(shared < 2 * single, "{shared} tasks vs {single} for one copy");
}
