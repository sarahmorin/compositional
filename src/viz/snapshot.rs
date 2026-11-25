/// A language-agnostic, plain-data picture of an e-graph at one point in time.
///
/// Renderers only ever see a `Snapshot`, never an `EGraph`. Labels and styles are resolved when
/// the snapshot is captured, so a snapshot can be stored, compared with another one or rendered
/// later without the e-graph (or its language and analysis types) around.
use crate::egraph::analysis::Analysis;
use crate::egraph::core::{ClassId, EClass, EGraph, Node, NodeId};
use crate::egraph::language::Language;
use crate::viz::style::Style;
use std::fmt::Display;

#[derive(Clone, Debug, PartialEq)]
pub struct VizNode {
    /// Canonical node id.
    pub id: NodeId,
    pub label: String,
    /// Canonical ids of the argument classes, in argument order.
    pub children: Vec<ClassId>,
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VizClass {
    /// Canonical class id.
    pub id: ClassId,
    /// Extra text shown on the class border (e.g. analysis data).
    pub label: Option<String>,
    /// Member nodes, sorted by id.
    pub nodes: Vec<VizNode>,
    pub style: Style,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// All canonical classes, sorted by id.
    pub classes: Vec<VizClass>,
}

impl Snapshot {
    /// Captures `egraph` with default labels (`Debug` of each operator) and no styling.
    pub fn capture<L: Language, N: Analysis<L>>(egraph: &EGraph<L, N>) -> Self {
        Capture::new().capture(egraph)
    }

    /// The class with the given canonical id.
    pub fn class(&self, id: ClassId) -> Option<&VizClass> {
        self.classes
            .binary_search_by_key(&id, |class| class.id)
            .ok()
            .map(|i| &self.classes[i])
    }

    /// All nodes, paired with the class that contains them.
    pub fn nodes(&self) -> impl Iterator<Item = (&VizClass, &VizNode)> {
        self.classes
            .iter()
            .flat_map(|class| class.nodes.iter().map(move |node| (class, node)))
    }
}

type NodeFn<'a, L, T> = Box<dyn Fn(NodeId, &Node<L>) -> T + 'a>;
type ClassFn<'a, D, T> = Box<dyn Fn(&EClass<D>) -> T + 'a>;

/// Configures how an e-graph is turned into a `Snapshot`.
///
/// ```ignore
/// let snapshot = Capture::new()
///     .display_labels()
///     .class_label(|class| Some(format!("size {}", class.data)))
///     .class_style(|class| if class.id == root { Style::new().fill("#fde68a") } else { Style::new() })
///     .capture(&egraph);
/// ```
///
/// Node callbacks receive the canonical node id; class callbacks receive the canonical class, so
/// they can look at its analysis data.
pub struct Capture<'a, L, D: Clone> {
    node_label: NodeFn<'a, L, String>,
    node_style: NodeFn<'a, L, Style>,
    class_label: ClassFn<'a, D, Option<String>>,
    class_style: ClassFn<'a, D, Style>,
}

impl<'a, L: Language, D: Clone> Capture<'a, L, D> {
    pub fn new() -> Self {
        Self {
            node_label: Box::new(|_, node| format!("{:?}", node.root)),
            node_style: Box::new(|_, _| Style::default()),
            class_label: Box::new(|_| None),
            class_style: Box::new(|_| Style::default()),
        }
    }

    /// Labels nodes with the operator's `Display` (e.g. `+` instead of `Add`).
    pub fn display_labels(self) -> Self
    where
        L: Display,
    {
        self.node_label(|_, node| node.root.to_string())
    }

    pub fn node_label(mut self, f: impl Fn(NodeId, &Node<L>) -> String + 'a) -> Self {
        self.node_label = Box::new(f);
        self
    }

    pub fn node_style(mut self, f: impl Fn(NodeId, &Node<L>) -> Style + 'a) -> Self {
        self.node_style = Box::new(f);
        self
    }

    pub fn class_label(mut self, f: impl Fn(&EClass<D>) -> Option<String> + 'a) -> Self {
        self.class_label = Box::new(f);
        self
    }

    pub fn class_style(mut self, f: impl Fn(&EClass<D>) -> Style + 'a) -> Self {
        self.class_style = Box::new(f);
        self
    }

    /// Reads the e-graph into a snapshot.
    ///
    /// Works on an e-graph that has not been rebuilt: child ids are canonicalized here, so
    /// congruent nodes awaiting a rebuild simply show up as separate nodes.
    pub fn capture<N: Analysis<L, Data = D>>(&self, egraph: &EGraph<L, N>) -> Snapshot {
        let mut class_ids: Vec<ClassId> = egraph.classes.keys().copied().collect();
        class_ids.sort_unstable();

        let classes = class_ids
            .into_iter()
            .map(|id| {
                let class = &egraph.classes[&id];
                let mut node_ids: Vec<NodeId> =
                    class.nodes.iter().map(|&n| egraph.find_node(n)).collect();
                node_ids.sort_unstable();
                node_ids.dedup();

                let nodes = node_ids
                    .into_iter()
                    .map(|nid| {
                        let node = egraph.get_node(nid);
                        VizNode {
                            id: nid,
                            label: (self.node_label)(nid, node),
                            children: node.children.iter().map(|&c| egraph.find(c)).collect(),
                            style: (self.node_style)(nid, node),
                        }
                    })
                    .collect();

                VizClass {
                    id,
                    label: (self.class_label)(class),
                    nodes,
                    style: (self.class_style)(class),
                }
            })
            .collect();

        Snapshot { classes }
    }
}

impl<'a, L: Language, D: Clone> Default for Capture<'a, L, D> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::test_lang::*;

    #[test]
    fn classes_and_nodes_are_sorted_and_complete() {
        let mut eg = egraph();
        eg.add_term(&term("+(*(a, 2), *(b, 2))"));
        let snap = Snapshot::capture(&eg);

        assert_eq!(snap.classes.len(), eg.num_classes());
        assert_eq!(snap.nodes().count(), eg.num_nodes());
        assert!(snap.classes.windows(2).all(|w| w[0].id < w[1].id));
        for class in &snap.classes {
            assert!(!class.nodes.is_empty());
            assert!(class.nodes.windows(2).all(|w| w[0].id < w[1].id));
        }
    }

    #[test]
    fn children_are_canonical_before_rebuild() {
        let mut eg = egraph();
        let a = eg.add_term(&term("a")).0.class();
        let b = eg.add_term(&term("b")).0.class();
        eg.add_term(&term("+(a, b)"));
        eg.union(a, b);
        // No rebuild: the `+` node still stores the stale child id.
        let snap = Snapshot::capture(&eg);
        for (_, node) in snap.nodes() {
            for &child in &node.children {
                assert!(snap.class(child).is_some(), "dangling child {child}");
            }
        }
        let leader = eg.find(a);
        let plus = snap.nodes().find(|(_, n)| n.label == "Add").unwrap().1;
        assert_eq!(plus.children, vec![leader, leader]);
    }

    #[test]
    fn self_loop_is_kept() {
        let mut eg = egraph();
        let a = eg.add_term(&term("a")).0.class();
        let times_one = eg.add_term(&term("*(a, 1)")).0.class();
        eg.union(a, times_one);
        eg.rebuild();
        let snap = Snapshot::capture(&eg);
        let class = snap.class(eg.find(a)).unwrap();
        assert!(
            class
                .nodes
                .iter()
                .any(|n| n.children.contains(&class.id))
        );
    }

    #[test]
    fn labels_and_styles_are_applied() {
        let mut eg = egraph();
        let root = eg.add_term(&term("+(x, 1)")).0.class();
        let snap = Capture::new()
            .display_labels()
            .class_label(|c| Some(format!("class {}", c.id)))
            .class_style(move |c| {
                if c.id == root {
                    Style::new().fill("gold")
                } else {
                    Style::new()
                }
            })
            .node_style(|_, n| match n.root {
                Math::Num(_) => Style::new().border("red"),
                _ => Style::new(),
            })
            .capture(&eg);

        let root_class = snap.class(root).unwrap();
        assert_eq!(root_class.style.fill.as_deref(), Some("gold"));
        assert_eq!(root_class.label.as_deref(), Some(&*format!("class {root}")));
        assert_eq!(root_class.nodes[0].label, "+");
        let one = snap.nodes().find(|(_, n)| n.label == "1").unwrap().1;
        assert_eq!(one.style.border.as_deref(), Some("red"));
    }
}
