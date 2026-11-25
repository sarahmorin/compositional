use compositional::egraph::analysis::Analysis;
use compositional::egraph::core::{ClassId, EGraph, Node, NodeId};
use compositional::egraph::language::{Language, OpOrVar, Pattern, Term};
use compositional::egraph::parse::{FromOp, parse_pattern, parse_term};
use compositional::egraph::rewrite::Rewrite;
use compositional::{define_language, rewrite};

define_language! {
    /// Small arithmetic language.
    pub enum Math {
        "+" = Add(2),
        "*" = Mul(2),
        "neg" = Neg(1),
        "pi" = Pi,
        Num(i64),
        Sym(String),
    }
}

#[derive(Clone, Debug)]
struct NoAnalysis;

impl Analysis<Math> for NoAnalysis {
    type Data = ();
    fn make(_: &mut EGraph<Math, Self>, _: &Node<Math>, _: NodeId) {}
    fn merge(&mut self, _: &mut (), _: ()) -> (bool, bool) {
        (false, false)
    }
}

fn term(s: &str) -> Term<Math> {
    parse_term(s).unwrap()
}

#[test]
fn language_impl() {
    assert_eq!(Math::Add.arity(), 2);
    assert_eq!(Math::Neg.arity(), 1);
    assert_eq!(Math::Pi.arity(), 0);
    assert_eq!(Math::Num(3).arity(), 0);
    assert!(Math::Num(3).matches(&Math::Num(3)));
    assert!(!Math::Num(3).matches(&Math::Num(4)));

    assert_eq!(Math::from_op("+"), Some(Math::Add));
    assert_eq!(Math::from_op("pi"), Some(Math::Pi));
    assert_eq!(Math::from_op("-7"), Some(Math::Num(-7)));
    // Named ops win over leaves, and leaves are tried in declaration order.
    assert_eq!(Math::from_op("x"), Some(Math::Sym("x".into())));

    assert_eq!(Math::Mul.to_string(), "*");
    assert_eq!(Math::Num(-7).to_string(), "-7");
}

#[test]
fn parse_terms() {
    let t = term("+(x, *(2, neg(pi)))");
    assert_eq!(t.op, Math::Add);
    assert_eq!(t.children[0].op, Math::Sym("x".into()));
    assert_eq!(t.children[1].op, Math::Mul);
    assert_eq!(t.children[1].children[1].children[0].op, Math::Pi);

    // Whitespace is insignificant.
    assert_eq!(term(" + ( x ,y ) ").to_string(), "+(x, y)");
    // FromStr works too.
    let t: Term<Math> = "neg(3)".parse().unwrap();
    assert_eq!(t.to_string(), "neg(3)");
}

#[test]
fn print_round_trips() {
    for s in ["x", "pi", "-3", "neg(x)", "+(x, *(2, neg(pi)))"] {
        assert_eq!(term(s).to_string(), s);
        let p: Pattern<Math> = parse_pattern(s).unwrap();
        assert_eq!(p.to_string(), s);
    }
    let p: Pattern<Math> = parse_pattern("+(?a, *(?b, ?a))").unwrap();
    assert_eq!(p.to_string(), "+(?a, *(?b, ?a))");
}

#[test]
fn parse_patterns() {
    let p: Pattern<Math> = parse_pattern("+(?a, *(2, ?b))").unwrap();
    assert!(matches!(p.root, OpOrVar::Op(Math::Add)));
    assert!(matches!(&p.children[0].root, OpOrVar::Var(v) if v == "a"));
    assert_eq!(p.vars, ["a".to_string(), "b".to_string()].into());

    // A term can't contain variables: `?a` isn't a Num, so it parses as a Sym.
    assert_eq!(term("?a").op, Math::Sym("?a".into()));
}

#[test]
fn parse_errors() {
    let err = |s: &str| parse_term::<Math>(s).unwrap_err();
    assert_eq!(err("+(x)").pos, 0); // arity mismatch
    assert_eq!(err("neg(+)").pos, 4); // arity mismatch on a nested op
    assert_eq!(err("pi(x)").pos, 0); // leaf with children
    assert_eq!(err("").pos, 0); // empty input
    assert_eq!(err("+(x y)").pos, 4); // missing comma
    assert_eq!(err("+(x, y").pos, 6); // unclosed paren
    assert_eq!(err("+(x, )").pos, 5); // missing child
    assert_eq!(err("neg(x) y").pos, 7); // trailing input
    assert!(err("+(x)").msg.contains("expects 2 children but got 1"));

    let perr = |s: &str| parse_pattern::<Math>(s).unwrap_err();
    assert!(perr("?a(x)").msg.contains("cannot have children"));
    assert!(perr("+(?, x)").msg.contains("needs a name"));
}

#[test]
fn rewrite_macro() {
    let r: Rewrite<Math> = rewrite!("commute-add"; "+(?a, ?b)" => "+(?b, ?a)");
    assert_eq!(r.name, "commute-add");
    assert_eq!(r.search.to_string(), "+(?a, ?b)");
    assert_eq!(r.apply.to_string(), "+(?b, ?a)");

    // Names can be any string-like expression.
    let name = format!("rule-{}", 1);
    let r: Rewrite<Math> = rewrite!(name; "neg(neg(?a))" => "?a",);
    assert_eq!(r.name, "rule-1");
}

#[test]
#[should_panic(expected = "rewrite bad: bad apply pattern")]
fn rewrite_macro_panics_on_bad_pattern() {
    let _: Rewrite<Math> = rewrite!("bad"; "+(?a, ?b)" => "+(?a)");
}

#[test]
#[should_panic(expected = "apply pattern uses variables not bound")]
fn rewrite_macro_panics_on_unbound_var() {
    let _: Rewrite<Math> = rewrite!("unbound"; "neg(?a)" => "?b");
}

#[test]
fn rewrites_apply_in_egraph() {
    let rules: Vec<Rewrite<Math>> = vec![
        rewrite!("commute-add"; "+(?a, ?b)" => "+(?b, ?a)"),
        rewrite!("double-neg"; "neg(neg(?a))" => "?a"),
    ];
    let mut eg = EGraph::new(NoAnalysis);
    let sum = eg.add_term(&term("+(x, neg(neg(y)))"));
    let inner = eg.lookup_term(&term("neg(neg(y))")).unwrap();

    eg.try_rewrite_node(&rules[0], sum.0.node());
    eg.try_rewrite_node(&rules[1], inner.node());
    eg.rebuild();

    let class: ClassId = sum.0.class();
    assert!(eg.class_contains(class, &term("+(neg(neg(y)), x)")));
    assert!(eg.class_contains(class, &term("+(y, x)")));
    assert!(eg.class_contains(class, &term("+(x, y)")));
}
