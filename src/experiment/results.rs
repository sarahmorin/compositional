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
    pub stop: StopReason,
    /// Metrics recorded by the explorer during exploration.
    pub metrics: Metrics,
    /// Wall-clock time of exploration and extraction together.
    pub total_time: Duration,
    pub initial_cost: D,
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
            vec![
                r.expr.clone(),
                r.strategy.clone(),
                format!("{:?}", r.stop),
                r.initial_cost.to_string(),
                r.cost.to_string(),
                millis(r.metrics.time_elapsed),
                millis(r.total_time),
                r.metrics.tasks_processed.to_string(),
                r.metrics.total_nodes.to_string(),
                r.metrics.total_classes.to_string(),
                r.term.to_string(),
            ]
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
