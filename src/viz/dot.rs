/// Graphviz DOT backend.
///
/// Layout conventions:
/// - each class is a `cluster_c{id}` subgraph with a dotted border, and its nodes share one rank,
///   so a class reads as a row;
/// - each node is a solid box named `n{id}` (ids are stable, which later lets an animated
///   viewer match elements across frames);
/// - an edge leaves its node from a port that encodes argument position (left to right) and
///   points at the child class's border. It targets an invisible anchor `c{id}` on the class's
///   rank rather than a member node, so it always enters the class from above;
/// - a node whose argument is its own class gets a loop on the node itself, since Graphviz
///   can't clip an edge to the border of a cluster that contains its tail;
/// - clusters are emitted in depth-first order from the roots, following argument order. Dot
///   seeds its left-to-right ordering from declaration order, so this keeps arguments from
///   crossing in tree-like regions.
use crate::viz::snapshot::{Snapshot, VizClass, VizNode};
use crate::viz::style::Style;
use crate::egraph::core::ClassId;
use std::collections::HashSet;
use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct DotOptions {
    /// Prefix class labels with `c{id}`.
    pub show_class_ids: bool,
    /// Add `n{id}` under each node's label.
    pub show_node_ids: bool,
    pub font_name: String,
}

impl Default for DotOptions {
    fn default() -> Self {
        Self {
            show_class_ids: true,
            show_node_ids: false,
            font_name: "Helvetica".into(),
        }
    }
}

const NODE_BORDER: &str = "#222222";
const CLASS_BORDER: &str = "#555555";
const EDGE_COLOR: &str = "#444444";

impl Snapshot {
    /// Renders the snapshot as a Graphviz DOT graph. Output is deterministic.
    pub fn to_dot(&self, opts: &DotOptions) -> String {
        let font = quote(&opts.font_name);
        let mut out = String::new();
        let o = &mut out;
        line(o, "digraph egraph {");
        line(o, "  compound=true;");
        line(o, "  newrank=true;");
        line(o, "  ordering=out;");
        line(o, "  rankdir=TB;");
        line(o, "  nodesep=0.25;");
        line(o, "  ranksep=0.35;");
        line(o, &format!("  fontname={font};"));
        line(
            o,
            &format!(
                "  node [shape=box, style=\"rounded,filled\", fillcolor=white, \
                 color=\"{NODE_BORDER}\", fontname={font}, fontsize=12, \
                 height=0.3, width=0.4, margin=\"0.1,0.04\"];"
            ),
        );
        line(
            o,
            &format!("  edge [color=\"{EDGE_COLOR}\", arrowsize=0.6, fontname={font}, fontsize=9];"),
        );

        let targeted = self.targeted_classes();
        let order = self.layout_order(&targeted);
        for &class in &order {
            write_class(o, class, targeted.contains(&class.id), opts);
        }
        for &class in &order {
            for node in &class.nodes {
                write_edges(o, class, node);
            }
        }
        line(o, "}");
        out
    }

    /// Classes that some node of *another* class points into.
    fn targeted_classes(&self) -> HashSet<ClassId> {
        self.nodes()
            .flat_map(|(class, node)| node.children.iter().filter(move |&&c| c != class.id))
            .copied()
            .collect()
    }

    /// Classes in depth-first preorder: roots (untargeted classes) first by id, children in
    /// argument order, then anything only reachable through a cycle.
    fn layout_order(&self, targeted: &HashSet<ClassId>) -> Vec<&VizClass> {
        let roots = self.classes.iter().filter(|c| !targeted.contains(&c.id));
        let mut order = Vec::with_capacity(self.classes.len());
        let mut seen = HashSet::new();
        for start in roots.chain(&self.classes) {
            let mut stack = vec![start];
            while let Some(class) = stack.pop() {
                if !seen.insert(class.id) {
                    continue;
                }
                order.push(class);
                let children = class.nodes.iter().flat_map(|n| &n.children);
                let children: Vec<_> = children.filter_map(|&c| self.class(c)).collect();
                stack.extend(children.into_iter().rev());
            }
        }
        order
    }
}

fn write_class(o: &mut String, class: &VizClass, anchored: bool, opts: &DotOptions) {
    let id = class.id;
    line(o, &format!("  subgraph cluster_c{id} {{"));

    let style = if class.style.fill.is_some() {
        "dotted,rounded,filled"
    } else {
        "dotted,rounded"
    };
    let label = match (opts.show_class_ids, &class.label) {
        (true, Some(text)) => format!("c{id}: {text}"),
        (true, None) => format!("c{id}"),
        (false, Some(text)) => text.clone(),
        (false, None) => String::new(),
    };
    let mut attrs = vec![
        ("style", quote(style)),
        ("color", quote(CLASS_BORDER)),
        ("penwidth", "1.4".to_string()),
        ("label", quote(&label)),
        ("labeljust", "l".to_string()),
        ("fontsize", "10".to_string()),
        ("fontcolor", quote(CLASS_BORDER)),
        ("margin", "8".to_string()),
    ];
    apply_style(&mut attrs, &class.style);
    for (key, value) in attrs {
        line(o, &format!("    {key}={value};"));
    }

    let mut row = Vec::new();
    if anchored {
        line(
            o,
            &format!("    c{id} [shape=point, style=invis, width=0, height=0, label=\"\"];"),
        );
        row.push(format!("c{id}"));
    }
    for node in &class.nodes {
        let label = if opts.show_node_ids {
            format!("{}\nn{}", node.label, node.id)
        } else {
            node.label.clone()
        };
        let mut attrs = vec![("label", quote(&label))];
        apply_style(&mut attrs, &node.style);
        line(o, &format!("    n{} [{}];", node.id, join(&attrs)));
        row.push(format!("n{}", node.id));
    }
    if row.len() > 1 {
        line(o, &format!("    {{rank=same; {};}}", row.join("; ")));
    }
    line(o, "  }");
}

fn write_edges(o: &mut String, class: &VizClass, node: &VizNode) {
    let arity = node.children.len();
    for (i, &child) in node.children.iter().enumerate() {
        if child == class.id {
            // A loop has no room for a port; label the argument when the position is ambiguous.
            let index = if arity > 1 {
                format!(" [taillabel=\"{i}\"]")
            } else {
                String::new()
            };
            line(o, &format!("  n{0} -> n{0}{index};", node.id));
            continue;
        }
        let edge = match tail_port(i, arity) {
            Some(port) => format!("  n{}:{port} -> c{child} [lhead=cluster_c{child}];", node.id),
            // Past three arguments, compass ports run out; number the edges instead.
            None => format!(
                "  n{} -> c{child} [lhead=cluster_c{child}, taillabel=\"{i}\"];",
                node.id
            ),
        };
        line(o, &edge);
    }
}

/// Compass point an argument edge leaves from, so argument order reads left to right.
fn tail_port(index: usize, arity: usize) -> Option<&'static str> {
    match (arity, index) {
        (1, _) => Some("s"),
        (2, 0) => Some("sw"),
        (2, 1) => Some("se"),
        (3, 0) => Some("sw"),
        (3, 1) => Some("s"),
        (3, 2) => Some("se"),
        _ => None,
    }
}

/// Overrides default attributes with the style's fields.
fn apply_style(attrs: &mut Vec<(&'static str, String)>, style: &Style) {
    let overrides = [
        ("fillcolor", style.fill.as_deref().map(quote)),
        ("color", style.border.as_deref().map(quote)),
        ("fontcolor", style.font.as_deref().map(quote)),
        ("penwidth", style.pen_width.map(|w| w.to_string())),
        ("tooltip", style.tooltip.as_deref().map(quote)),
    ];
    for (key, value) in overrides {
        let Some(value) = value else { continue };
        match attrs.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => attrs.push((key, value)),
        }
    }
}

fn join(attrs: &[(&str, String)]) -> String {
    attrs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A DOT quoted string. Newlines become centered line breaks.
fn quote(text: &str) -> String {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn line(out: &mut String, text: &str) {
    writeln!(out, "{text}").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::snapshot::Capture;
    use crate::viz::test_lang::*;

    #[test]
    fn quoting_escapes_specials() {
        assert_eq!(quote(r#"Sym("x")"#), r#""Sym(\"x\")""#);
        assert_eq!(quote("a\\b\nc"), r#""a\\b\nc""#);
    }

    #[test]
    fn emits_clusters_nodes_and_compound_edges() {
        let mut eg = egraph();
        let root = eg.add_term(&term("+(a, b)")).0.class();
        let snap = Capture::new().display_labels().capture(&eg);
        let dot = snap.to_dot(&DotOptions::default());

        for class in &snap.classes {
            assert!(dot.contains(&format!("subgraph cluster_c{} {{", class.id)));
        }
        let plus = snap.nodes().find(|(_, n)| n.label == "+").unwrap().1;
        let [a, b] = plus.children[..] else { panic!() };
        assert!(dot.contains(&format!("n{}:sw -> c{a} [lhead=cluster_c{a}];", plus.id)));
        assert!(dot.contains(&format!("n{}:se -> c{b} [lhead=cluster_c{b}];", plus.id)));
        // The root class has no incoming edges, so no anchor.
        assert!(!dot.contains(&format!("c{root} [shape=point")));
        assert!(dot.contains(&format!("c{a} [shape=point")));
    }

    #[test]
    fn self_loop_points_at_own_node() {
        let mut eg = egraph();
        let a = eg.add_term(&term("a")).0.class();
        let times_one = eg.add_term(&term("*(a, 1)"));
        eg.union(a, times_one.0.class());
        eg.rebuild();
        let dot = Snapshot::capture(&eg).to_dot(&DotOptions::default());
        let n = times_one.0.node();
        assert!(dot.contains(&format!("n{n} -> n{n} [taillabel=\"0\"];")));
        // Nothing else points into the class, so no compound edge targets it.
        assert!(!dot.contains(&format!("-> c{} ", eg.find(a))));
    }

    #[test]
    fn styles_are_emitted() {
        let mut eg = egraph();
        eg.add_term(&term("neg(x)"));
        let snap = Capture::new()
            .node_style(|_, _| Style::new().fill("#abcdef").pen_width(2.0))
            .class_style(|_| Style::new().fill("lightyellow").border("orange"))
            .capture(&eg);
        let dot = snap.to_dot(&DotOptions::default());
        assert!(dot.contains("fillcolor=\"#abcdef\", penwidth=2"));
        assert!(dot.contains("style=\"dotted,rounded,filled\";"));
        assert!(dot.contains("fillcolor=\"lightyellow\";"));
        assert!(dot.contains("color=\"orange\";"));
    }

    #[test]
    fn clusters_follow_argument_order_from_roots() {
        let mut eg = egraph();
        // Ids are assigned bottom-up, so id order would put the root last.
        let root = eg.add_term(&term("*(+(x, y), z)")).0.class();
        let snap = Capture::new().display_labels().capture(&eg);
        let dot = snap.to_dot(&DotOptions::default());
        let class_of = |label: &str| snap.nodes().find(|(_, n)| n.label == label).unwrap().0.id;
        let positions: Vec<usize> = [root, class_of("+"), class_of("x"), class_of("y"), class_of("z")]
            .iter()
            .map(|c| dot.find(&format!("subgraph cluster_c{c} ")).unwrap())
            .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]), "{dot}");
    }

    #[test]
    fn class_members_share_a_rank() {
        let mut eg = egraph();
        let x = eg.add_term(&term("+(a, b)")).0.class();
        let y = eg.add_term(&term("+(b, a)")).0.class();
        eg.union(x, y);
        eg.rebuild();
        let snap = Snapshot::capture(&eg);
        let class = snap.class(eg.find(x)).unwrap();
        let [n1, n2] = &class.nodes[..] else { panic!() };
        let dot = snap.to_dot(&DotOptions::default());
        assert!(dot.contains(&format!("{{rank=same; n{}; n{};}}", n1.id, n2.id)));
    }

    #[test]
    fn style_overrides_defaults_without_duplicates() {
        let mut eg = egraph();
        eg.add_term(&term("x"));
        let snap = Capture::new()
            .class_style(|_| Style::new().border("orange"))
            .capture(&eg);
        let dot = snap.to_dot(&DotOptions::default());
        assert_eq!(dot.matches("    color=").count(), 1);
        assert!(dot.contains("    color=\"orange\";"));
    }

    #[test]
    fn output_is_deterministic() {
        let build = || {
            let mut eg = egraph();
            let x = eg.add_term(&term("*(+(x, y), +(y, x))")).0.class();
            let y = eg.add_term(&term("+(y, x)")).0.class();
            eg.union(x, y);
            eg.rebuild();
            Snapshot::capture(&eg).to_dot(&DotOptions::default())
        };
        assert_eq!(build(), build());
    }
}
