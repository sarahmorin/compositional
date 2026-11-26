use intarsia_build::compile_isle_dir;

pub mod runner;

use compositional::egraph::core::NodeId;
use compositional::egraph::language::Term;
use compositional::viz::replay::{PlayerOptions, Replay, ReplayView, Step};
use runner::Arith;

/// Usage: cargo run --example arith-isle [-- --replay out.html]
fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let replay_path = match (args.next().as_deref(), args.next()) {
        (None, _) => None,
        (Some("--replay"), Some(path)) => Some(path),
        _ => {
            eprintln!("usage: arith-isle [--replay OUT.html]");
            std::process::exit(2);
        }
    };

    // Compile all ISLE files in the `isle` directory
    compile_isle_dir("examples/arith-isle/isle").unwrap();

    // Create a new Isle runner
    let mut runner = runner::IsleRunner::new();
    if replay_path.is_some() {
        // Record from the start so the replay also shows the initial term being built.
        runner.egraph.start_recording();
    }

    // Add a term to the e-graph
    let id = runner.add_term(&"Add(x, Add(1, 2))".parse::<Term<Arith>>().unwrap());

    // Print the e-graph before running ISLE
    println!("EGraph before running ISLE:");
    println!("Total nodes: {}", runner.egraph.num_nodes());
    println!("Total classes: {}", runner.egraph.num_classes());

    // Run the ISLE program
    runner.run(id);

    // Print the result
    println!("EGraph after running ISLE:");
    println!("Total nodes: {}", runner.egraph.num_nodes());
    println!("Total classes: {}", runner.egraph.num_classes());

    if let Some(path) = replay_path {
        let trace = runner.egraph.take_recording().unwrap();
        println!("Recorded {} events", trace.len());
        let view = ReplayView::new()
            .display_labels()
            .class_label(|_, data: Option<&(NodeId, usize)>| data.map(|(_, cost)| format!("cost {cost}")));
        let opts = PlayerOptions {
            title: "arith-isle: Add(x, Add(1, 2))".into(),
            ..PlayerOptions::default()
        };
        Replay::new(trace)
            .write_html(&path, &Step::Event, &view, &opts)
            .expect("failed to write replay");
        println!("Wrote replay to {path}");
    }
}
