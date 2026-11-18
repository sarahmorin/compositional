/// Command-line frontend for experiments. An example binary builds an `Experiment` and hands
/// it to `run`, which reads options from the command line:
///
/// ```text
/// cargo run --example math-bench -- --expr factor --strategy recursive --csv out/math.csv
/// ```
use super::Experiment;
use crate::egraph::{extract::CostFunction, language::Language};
use std::fmt::Display;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "\
Usage: cargo run --example <EXAMPLE> -- [OPTIONS]

Options:
  -e, --expr NAMES        Run only these expressions (comma-separated, repeatable)
  -s, --strategy NAMES    Run only these strategies (comma-separated, repeatable)
      --time-limit SECS   Stop each run after SECS seconds
      --task-limit N      Stop each run after N tasks
      --csv PATH          Also write the results to PATH as CSV
      --list              List the expressions and strategies, then exit
  -h, --help              Print this help, then exit";

#[derive(Debug, Default, PartialEq)]
struct Options {
    exprs: Vec<String>,
    strategies: Vec<String>,
    time_limit: Option<Duration>,
    task_limit: Option<usize>,
    csv: Option<PathBuf>,
    list: bool,
    help: bool,
}

/// Runs `experiment` as configured by the command-line arguments and prints the results.
/// Exits with failure if the arguments are invalid or any run panicked.
pub fn run<L, C, D>(experiment: Experiment<L, C, D>) -> ExitCode
where
    L: Language + Display,
    C: CostFunction<L, D>,
    D: PartialOrd + Clone + Display,
{
    let opts = match parse_args(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(err) => {
            eprintln!("error: {err}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if opts.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if opts.list {
        print_list(&experiment);
        return ExitCode::SUCCESS;
    }
    let experiment = match configure(experiment, &opts) {
        Ok(experiment) => experiment,
        Err(err) => {
            eprintln!("error: {err}");
            return ExitCode::from(2);
        }
    };

    let results = experiment.run();
    println!("{results}");
    if let Some(path) = &opts.csv {
        if let Err(err) = results.write_csv(path) {
            eprintln!("error: could not write {}: {err}", path.display());
            return ExitCode::FAILURE;
        }
        println!("wrote {}", path.display());
    }
    if results.records.iter().any(|r| r.result.is_err()) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut opts = Options::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "-e" | "--expr" => opts.exprs.extend(split_names(&value()?)),
            "-s" | "--strategy" => opts.strategies.extend(split_names(&value()?)),
            "--time-limit" => {
                let secs = value()?;
                let time = secs.parse().ok().and_then(|s| Duration::try_from_secs_f64(s).ok());
                opts.time_limit = Some(time.ok_or(format!("invalid --time-limit: {secs}"))?);
            }
            "--task-limit" => {
                let n = value()?;
                opts.task_limit = Some(n.parse().map_err(|_| format!("invalid --task-limit: {n}"))?);
            }
            "--csv" => opts.csv = Some(value()?.into()),
            "--list" => opts.list = true,
            "-h" | "--help" => opts.help = true,
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(opts)
}

fn split_names(names: &str) -> impl Iterator<Item = String> + '_ {
    names.split(',').filter(|n| !n.is_empty()).map(str::to_string)
}

/// Narrows the experiment to the selected expressions and strategies and applies the limits.
fn configure<L, C, D>(
    mut experiment: Experiment<L, C, D>,
    opts: &Options,
) -> Result<Experiment<L, C, D>, String>
where
    L: Language,
{
    retain_named(&mut experiment.exprs, &opts.exprs, |(name, _)| name, "expression")?;
    retain_named(&mut experiment.strategies, &opts.strategies, |s| s.name(), "strategy")?;
    if opts.time_limit.is_some() {
        experiment.limits.time = opts.time_limit;
    }
    if opts.task_limit.is_some() {
        experiment.limits.tasks = opts.task_limit;
    }
    Ok(experiment)
}

/// Keeps the items named in `wanted`, or all of them if `wanted` is empty.
fn retain_named<T>(
    items: &mut Vec<T>,
    wanted: &[String],
    name: impl Fn(&T) -> &str,
    kind: &str,
) -> Result<(), String> {
    if let Some(unknown) = wanted.iter().find(|w| !items.iter().any(|i| name(i) == *w)) {
        return Err(format!("unknown {kind}: {unknown} (see --list)"));
    }
    if !wanted.is_empty() {
        items.retain(|i| wanted.iter().any(|w| name(i) == w));
    }
    Ok(())
}

fn print_list<L: Language + Display, C, D>(experiment: &Experiment<L, C, D>) {
    println!("Expressions:");
    for (name, expr) in &experiment.exprs {
        println!("  {name}: {expr}");
    }
    println!("Strategies:");
    for strategy in &experiment.strategies {
        println!("  {}", strategy.name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &str) -> Result<Options, String> {
        parse_args(args.split_whitespace().map(str::to_string))
    }

    #[test]
    fn parses_options() {
        let opts = parse("-e a,b --expr c -s rec --time-limit 1.5 --task-limit 10 --csv x.csv");
        assert_eq!(
            opts.unwrap(),
            Options {
                exprs: vec!["a".into(), "b".into(), "c".into()],
                strategies: vec!["rec".into()],
                time_limit: Some(Duration::from_millis(1500)),
                task_limit: Some(10),
                csv: Some("x.csv".into()),
                ..Options::default()
            }
        );
        assert_eq!(parse("").unwrap(), Options::default());
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(parse("--bogus").unwrap_err().contains("unknown argument"));
        assert!(parse("--expr").unwrap_err().contains("needs a value"));
        assert!(parse("--time-limit -1").unwrap_err().contains("invalid --time-limit"));
        assert!(parse("--task-limit x").unwrap_err().contains("invalid --task-limit"));
    }

    #[test]
    fn retain_named_filters_and_checks_names() {
        let mut items = vec!["a", "b", "c"];
        retain_named(&mut items, &[], |s| s, "item").unwrap();
        assert_eq!(items, ["a", "b", "c"]);
        retain_named(&mut items, &["c".into(), "a".into()], |s| s, "item").unwrap();
        assert_eq!(items, ["a", "c"]);
        let err = retain_named(&mut items, &["z".into()], |s| s, "item").unwrap_err();
        assert_eq!(err, "unknown item: z (see --list)");
    }
}
