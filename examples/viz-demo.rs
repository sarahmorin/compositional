//! Renders the classic `(a * 2) / 2` e-graph before and after a few rewrites.
//!
//! cargo run --example viz-demo [out_dir]   (default: target/viz)

use compositional::define_language;
use compositional::egraph::analysis::Analysis;
use compositional::egraph::core::{ClassId, EGraph, Node, NodeId};
use compositional::egraph::language::Term;
use compositional::egraph::parse::parse_term;
use compositional::viz::{Capture, DotOptions, Style, render};
use std::path::Path;

define_language! {
    pub enum Math {
        "+" = Add(2),
        "*" = Mul(2),
        "/" = Div(2),
        "<<" = Shl(2),
        Num(i64),
        Sym(String),
    }
}

/// Size of the smallest term in each class.
#[derive(Clone, Debug)]
struct Size;

impl Analysis<Math> for Size {
    type Data = usize;
    fn make(egraph: &mut EGraph<Math, Self>, enode: &Node<Math>, _: NodeId) -> usize {
        1 + enode.children.iter().map(|&c| egraph.class(c).data).sum::<usize>()
    }
    fn merge(&mut self, a: &mut usize, b: usize) -> (bool, bool) {
        let changed = (b < *a, *a < b);
        *a = (*a).min(b);
        changed
    }
}

fn term(s: &str) -> Term<Math> {
    parse_term(s).unwrap()
}

fn save(egraph: &EGraph<Math, Size>, root: ClassId, path: &Path) -> std::io::Result<()> {
    let root = egraph.find(root);
    let dot = Capture::new()
        .display_labels()
        .class_label(|class| Some(format!("size {}", class.data)))
        .class_style(|class| {
            if class.id == root {
                Style::new().fill("#fef3c7").border("#b45309")
            } else {
                Style::new()
            }
        })
        .node_style(|_, node| match node.root {
            Math::Num(_) | Math::Sym(_) => Style::new().fill("#dbeafe"),
            _ => Style::new(),
        })
        .capture(egraph)
        .to_dot(&DotOptions::default());
    std::fs::write(path.with_extension("dot"), &dot)?;
    render::write_svg(&dot, path.with_extension("svg"))?;
    println!("wrote {}", path.with_extension("svg").display());
    Ok(())
}

fn main() -> std::io::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "target/viz".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;

    let mut eg = EGraph::new(Size);
    let root = eg.add_term(&term("/(*(a, 2), 2)")).0.class();
    save(&eg, root, &out.join("before"))?;

    // a * 2 = a << 1
    let times_two = eg.lookup_term(&term("*(a, 2)")).unwrap().class();
    let shl = eg.add_term(&term("<<(a, 1)")).0.class();
    eg.union(times_two, shl);
    // (a * 2) / 2 = a * (2 / 2)
    let reassoc = eg.add_term(&term("*(a, /(2, 2))")).0.class();
    eg.union(root, reassoc);
    // 2 / 2 = 1
    let two_over_two = eg.lookup_term(&term("/(2, 2)")).unwrap().class();
    let one = eg.add_term(&term("1")).0.class();
    eg.union(two_over_two, one);
    // a * 1 = a
    let a = eg.lookup_term(&term("a")).unwrap().class();
    let times_one = eg.add_term(&term("*(a, 1)")).0.class();
    eg.union(a, times_one);
    eg.rebuild();
    save(&eg, root, &out.join("after"))?;
    Ok(())
}
