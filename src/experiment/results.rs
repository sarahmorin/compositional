/// Experiment results, printable as a table or writable as CSV.
use crate::egraph::language::{Language, Term};
use crate::explorer::StopReason;
use crate::metrics::Metrics;
use std::fmt::{self, Display};
use std::path::Path;
use std::time::Duration;
use std::{fs, io};

/// The result of running one strategy on one expression.
pub struct Record<L: Language, D> {
    pub expr: String,
    pub strategy: String,
    pub initial_cost: D,
    /// Wall-clock time of exploration and extraction together.
    pub total_time: Duration,
    /// The completed run, or the panic message if the strategy panicked.
    pub result: Result<Run<L, D>, String>,
}

/// A run that completed, with its extracted term scored by the experiment's cost function.
pub struct Run<L: Language, D> {
    pub stop: StopReason,
    /// Metrics recorded by the explorer during exploration.
    pub metrics: Metrics,
    pub cost: D,
    pub term: Term<L>,
}

pub struct Results<L: Language, D> {
    pub records: Vec<Record<L, D>>,
}

const COLUMNS: [&str; 11] = [
    "expr",
    "strategy",
    "stop",
    "initial_cost",
    "cost",
    "explore_ms",
    "total_ms",
    "tasks",
    "nodes",
    "classes",
    "term",
];

fn millis(time: Duration) -> String {
    format!("{:.3}", time.as_secs_f64() * 1000.0)
}

/// Quotes a CSV field if it contains a separator, quote, or newline.
fn csv_field(field: &str) -> String {
    if field.contains([',', '"', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

impl<L, D> Results<L, D>
where
    L: Language + Display,
    D: Display,
{
    /// The header followed by one row per record, in `COLUMNS` order.
    fn rows(&self) -> Vec<Vec<String>> {
        let header = COLUMNS.iter().map(|c| c.to_string()).collect();
        let records = self.records.iter().map(|r| {
            let mut row = vec![r.expr.clone(), r.strategy.clone()];
            match &r.result {
                Ok(run) => row.extend([
                    format!("{:?}", run.stop),
                    r.initial_cost.to_string(),
                    run.cost.to_string(),
                    millis(run.metrics.time_elapsed),
                    millis(r.total_time),
                    run.metrics.tasks_processed.to_string(),
                    run.metrics.total_nodes.to_string(),
                    run.metrics.total_classes.to_string(),
                    run.term.to_string(),
                ]),
                // A panicked run has no metrics or term; report its message in the term column.
                Err(msg) => row.extend([
                    "Panicked".to_string(),
                    r.initial_cost.to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    millis(r.total_time),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    msg.clone(),
                ]),
            }
            row
        });
        std::iter::once(header).chain(records).collect()
    }

    /// Writes the results as CSV, creating parent directories as needed.
    pub fn write_csv(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = String::new();
        for row in self.rows() {
            let fields: Vec<String> = row.iter().map(|f| csv_field(f)).collect();
            out.push_str(&fields.join(","));
            out.push('\n');
        }
        fs::write(path, out)
    }
}

/// An aligned plain-text table.
impl<L, D> Display for Results<L, D>
where
    L: Language + Display,
    D: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows = self.rows();
        let widths: Vec<usize> = (0..COLUMNS.len())
            .map(|i| rows.iter().map(|row| row[i].len()).max().unwrap_or(0))
            .collect();
        for row in &rows {
            let cells: Vec<String> = row
                .iter()
                .zip(&widths)
                .map(|(cell, &width)| format!("{cell:<width$}"))
                .collect();
            writeln!(f, "{}", cells.join("  ").trim_end())?;
        }
        Ok(())
    }
}
