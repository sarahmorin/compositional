/// Metrics recorded by the explorer during run.

#[derive(Debug, Clone)]
pub struct Metrics {
    pub tasks_processed: usize,
    pub time_elapsed: std::time::Duration,
    pub total_nodes: usize,
    pub total_classes: usize,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            tasks_processed: 0,
            time_elapsed: std::time::Duration::new(0, 0),
            total_nodes: 0,
            total_classes: 0,
        }
    }
}
