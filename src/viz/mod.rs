/// E-graph visualization.
///
/// The pipeline has three stages, each usable on its own:
///
/// 1. [`Capture`] reads an `EGraph` and resolves labels and styles into a [`Snapshot`];
/// 2. [`Snapshot`] is plain data, independent of the language and analysis types;
/// 3. a backend renders it: [`Snapshot::to_dot`] for Graphviz text, then [`render`] for SVG.
///
/// Quick debugging:
///
/// ```ignore
/// compositional::viz::write_svg(&egraph, "egraph.svg")?;
/// ```
///
/// With labels and colors:
///
/// ```ignore
/// let dot = Capture::new()
///     .display_labels()
///     .class_label(|class| Some(format!("cost {}", class.data)))
///     .class_style(|class| if class.id == root { Style::new().fill("#fde68a") } else { Style::new() })
///     .capture(&egraph)
///     .to_dot(&DotOptions::default());
/// render::write_svg(&dot, "egraph.svg")?;
/// ```
mod dot;
pub mod render;
mod snapshot;
mod style;

pub use dot::DotOptions;
pub use snapshot::{Capture, Snapshot, VizClass, VizNode};
pub use style::Style;

use crate::egraph::analysis::Analysis;
use crate::egraph::core::EGraph;
use crate::egraph::language::Language;
use std::io;
use std::path::Path;

/// Writes an SVG of `egraph` with default labels and styling.
pub fn write_svg<L: Language, N: Analysis<L>>(
    egraph: &EGraph<L, N>,
    path: impl AsRef<Path>,
) -> io::Result<()> {
    let dot = Snapshot::capture(egraph).to_dot(&DotOptions::default());
    render::write_svg(&dot, path)
}

#[cfg(test)]
mod test_lang {
    use crate::egraph::analysis::Analysis;
    use crate::egraph::core::{EGraph, Node, NodeId};
    use crate::egraph::language::Term;
    use crate::egraph::parse::parse_term;

    crate::define_language! {
        pub enum Math {
            "+" = Add(2),
            "*" = Mul(2),
            "neg" = Neg(1),
            Num(i64),
            Sym(String),
        }
    }

    #[derive(Clone, Debug)]
    pub struct NoAnalysis;

    impl Analysis<Math> for NoAnalysis {
        type Data = ();
        fn make(_: &mut EGraph<Math, Self>, _: &Node<Math>, _: NodeId) {}
        fn merge(&mut self, _: &mut (), _: ()) -> (bool, bool) {
            (false, false)
        }
    }

    pub fn egraph() -> EGraph<Math, NoAnalysis> {
        EGraph::new(NoAnalysis)
    }

    pub fn term(s: &str) -> Term<Math> {
        parse_term(s).unwrap()
    }
}
