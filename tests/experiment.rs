use compositional::ast::MinAST;
use compositional::define_language;
use compositional::egraph::language::Term;
use compositional::egraph::parse::parse_term;
use compositional::egraph::rewrite::Rewrite;
use compositional::experiment::{Experiment, Limits, Outcome, Strategy};
use compositional::explorer::StopReason;
use compositional::metrics::Metrics;
use compositional::recursive::Recursive;
use compositional::rewrite;

define_language! {
    enum Lang {
        "+" = Add(2),
        "f" = F(1),
        "g" = G(1),
        "k" = K(1),
        "m" = M(1),
        Sym(String),
    }
}

fn term(s: &str) -> Term<Lang> {
    parse_term(s).unwrap()
}

fn rules() -> Vec<Rewrite<Lang>> {
    vec![
        rewrite!("g-k"; "g(k(?x))" => "m(?x)"),
        rewrite!("f-g"; "f(g(?x))" => "?x"),
    ]
}

/// Baseline strategy that returns the input unchanged.
struct Identity;

impl Strategy<Lang> for Identity {
    fn name(&self) -> &str {
        "identity"
    }

    fn run(&self, expr: &Term<Lang>, _: &[Rewrite<Lang>], _: &Limits) -> Outcome<Lang> {
        Outcome {
            stop: StopReason::NoMoreTasks,
            metrics: Metrics::new(),
            term: expr.clone(),
        }
    }
}

fn experiment() -> Experiment<Lang, MinAST, usize> {
    Experiment::new(rules(), MinAST)
        .with_expr("nested", term("f(g(k(a)))"))
        .with_expr("sum", term("+(f(g(k(a))), b)"))
        .with_strategy(Recursive)
        .with_strategy(Identity)
}

#[test]
fn runs_every_strategy_on_every_expression() {
    let results = experiment().run();
    let summary: Vec<_> = results
        .records
        .iter()
        .map(|r| {
            let names = (r.expr.as_str(), r.strategy.as_str());
            (names, r.initial_cost, r.cost, r.term.to_string())
        })
        .collect();
    assert_eq!(
        summary,
        [
            (("nested", "recursive"), 4, 3, "f(m(a))".to_string()),
            (("nested", "identity"), 4, 4, "f(g(k(a)))".to_string()),
            (("sum", "recursive"), 6, 5, "+(f(m(a)), b)".to_string()),
            (("sum", "identity"), 6, 6, "+(f(g(k(a))), b)".to_string()),
        ]
    );
    let recursive = &results.records[0];
    assert_eq!(recursive.stop, StopReason::NoMoreTasks);
    assert!(recursive.metrics.total_nodes > 0);
}

#[test]
fn limits_reach_the_explorer() {
    let results = experiment().with_task_limit(1).run();
    let recursive = &results.records[0];
    assert_eq!(recursive.stop, StopReason::TaskLimitExceeded);
    assert_eq!(recursive.cost, recursive.initial_cost);
}

#[test]
fn results_print_and_write_csv() {
    let results = experiment().run();

    let table = results.to_string();
    let lines: Vec<&str> = table.lines().collect();
    assert_eq!(lines.len(), 5);
    assert!(lines[0].starts_with("expr"));
    assert!(lines[1].starts_with("nested"));

    let path = std::env::temp_dir().join("compositional-test/results.csv");
    results.write_csv(&path).unwrap();
    let csv = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 5);
    assert!(lines[0].starts_with("expr,strategy,stop,"));
    // Terms containing commas are quoted.
    assert!(lines[3].ends_with(",\"+(f(m(a)), b)\""));
}
