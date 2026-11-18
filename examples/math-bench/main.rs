use compositional::ast::MinAST;
use compositional::define_language;
use compositional::egraph::parse::parse_term;
use compositional::egraph::rewrite::Rewrite;
use compositional::experiment::{Experiment, cli};
use compositional::recursive::Recursive;
use compositional::rewrite as rw;
use ordered_float::NotNan;
use std::process::ExitCode;
use std::time::Duration;

pub type Constant = NotNan<f64>;
pub type Symbol = String;

define_language! {
    pub enum Math {
        "+" = Add(2),
        "-" = Sub(2),
        "*" = Mul(2),
        "/" = Div(2),
        "pow" = Pow(2),
        "sqrt" = Sqrt(1),

        // "d" = Diff([Id; 2]),
        // "i" = Integral([Id; 2]),
        // "ln" = Ln(Id),
        // "sin" = Sin(Id),
        // "cos" = Cos(Id),

        Constant(Constant),
        Symbol(Symbol),
    }
}

#[rustfmt::skip]
pub fn rules() -> Vec<Rewrite<Math>> {
    vec![
        // Addition
        rw!("add-comm";     "+(?a, ?b)"         => "+(?b, ?a)"),
        rw!("add-assoc";    "+(+(?a, ?b), ?c)"  => "+(?a, +(?b, ?c))"),
        rw!("add-zero";     "+(?a, 0)"          => "?a"),
        rw!("zero-add";     "+(0, ?a)"          => "?a"),

        // Multiplication
        rw!("mul-comm";     "*(?a, ?b)"         => "*(?b, ?a)"),
        rw!("mul-assoc";    "*(*(?a, ?b), ?c)"  => "*(?a, *(?b, ?c))"),
        rw!("mul-one";      "*(?a, 1)"          => "?a"),
        // rw!("one-mul";      "*(1, ?a)"          => "?a"),
        rw!("mul-zero";     "*(?a, 0)"          => "0"),
        // rw!("zero-mul";     "*(0, ?a)"          => "0"),
        rw!("mul-distrib";  "*(?a, +(?b, ?c))"  => "+(*(?a, ?b), *(?a, ?c))"),
        rw!("mul-factor";   "+(*(?a, ?b), *(?a, ?c))" => "*(?a, +(?b, ?c))"),

        // Subtraction
        rw!("sub-zero";     "-(?a, 0)"          => "?a"),
        rw!("sub-cancel";   "-(?a, ?a)"         => "0"),
        rw!("sub-canon";    "-(?a, ?b)"         => "+(?a, *(-1, ?b))"),

        // Division
        rw!("div-one";      "/(?a, 1)"          => "?a"),
        rw!("div-cancel";   "/(?a, ?a)"         => "1"), // HACK: if not 0
        rw!("div-zero";     "/(0, ?a)"          => "0"),
        rw!("div-canon";    "/(?a, ?b)"         => "*(?a, pow(?b, -1))"), // HACK: if not 0
        rw!("mul-div-cancel"; "*(?a, /(?b, ?a))" => "?b"), // HACK: if not 0

        // Powers
        rw!("pow-one";      "pow(?a, 1)"        => "?a"),
        rw!("pow-2";        "pow(?a, 2)"        => "*(?a, ?a)"),
        rw!("pow-zero";     "pow(?a, 0)"        => "1"),
        rw!("pow-mul";      "*(pow(?a, ?b), pow(?a, ?c))" => "pow(?a, +(?b, ?c))"),
        rw!("pow-recip";    "pow(?a, -1)"       => "/(1, ?a)"),
        rw!("sqrt-sq";      "pow(sqrt(?a), 2)"  => "?a"),
        rw!("sq-sqrt";      "sqrt(pow(?a, 2))"  => "?a"),

        // rw!("ln-exp";       "ln(pow(e, ?x))"    => "?x"),
        // rw!("exp-ln";       "pow(e, ln(?x))"    => "?x"),
        // TODO: add more rules for trig, diff, integral, etc.
    ]
}

/// Named test expressions.
#[rustfmt::skip]
const EXPRS: &[(&str, &str)] = &[
    ("add-zero",        "+(x, 0)"),
    ("mul-div-cancel",  "*(x, /(y, x))"),
    ("sub-cancel",      "-(*(a, 1), a)"),
    ("sqrt-sq",         "pow(sqrt(+(x, 0)), 2)"),
    ("factor",          "+(*(a, b), *(a, c))"),
    ("factor-commuted", "+(*(b, a), *(a, c))"),
    ("powers",          "*(pow(2, x), pow(2, y))"),
    ("add-chain",       "+(+(+(a, b), c), d)"),
];

fn main() -> ExitCode {
    let mut experiment = Experiment::new(rules(), MinAST)
        .with_strategy(Recursive)
        .with_time_limit(Duration::from_secs(10));
    for (name, expr) in EXPRS {
        experiment = experiment.with_expr(name, parse_term(expr).unwrap());
    }
    cli::run(experiment)
}
