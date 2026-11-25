use intarsia_build::compile_isle_dir;

pub mod runner;

use compositional::egraph::language::Term;
use runner::Arith;

fn main() {
    env_logger::init();

    // Compile all ISLE files in the `isle` directory
    compile_isle_dir("examples/arith-isle/isle").unwrap();

    // Create a new Isle runner
    let mut runner = runner::IsleRunner::new();

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
}
