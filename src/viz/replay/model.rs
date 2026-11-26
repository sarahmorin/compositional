/// A lightweight reconstruction of an e-graph's structure from its event log.
///
/// The model only knows what the events say: classes, their member nodes, each node's operator
/// and children, analysis data, and the driver's current focus. It never touches an `EGraph`,
/// so a trace can be replayed long after the run that produced it.
use crate::egraph::core::{ClassId, NodeId};
use crate::egraph::trace::{Event, Target};
use crate::viz::replay::ReplayView;
use crate::viz::snapshot::{Snapshot, VizClass, VizNode};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct ModelNode<L> {
    pub op: L,
    /// Child ids as logged; canonicalize with `find`.
    pub children: Vec<ClassId>,
    /// The class the node was created in; canonicalize with `find`.
    class: ClassId,
}

#[derive(Clone, Debug)]
pub struct ModelClass<D> {
    /// Live member nodes, in no particular order.
    pub nodes: Vec<NodeId>,
    /// Latest analysis data, if any was logged.
    pub data: Option<D>,
}

#[derive(Clone, Debug)]
pub struct ReplayModel<L, D> {
    /// Union-find over classes; only non-canonical ids have an entry.
    class_parent: HashMap<ClassId, ClassId>,
    /// Dropped node -> node it was dropped in favor of.
    node_parent: HashMap<NodeId, NodeId>,
    /// Canonical classes.
    classes: BTreeMap<ClassId, ModelClass<D>>,
    /// Live nodes.
    nodes: HashMap<NodeId, ModelNode<L>>,
    focus: Option<Target>,
    note: Option<String>,
    rebuilding: bool,
}

impl<L: Clone, D: Clone> ReplayModel<L, D> {
    pub fn new() -> Self {
        Self {
            class_parent: HashMap::new(),
            node_parent: HashMap::new(),
            classes: BTreeMap::new(),
            nodes: HashMap::new(),
            focus: None,
            note: None,
            rebuilding: false,
        }
    }

    /// Updates the model with one event.
    ///
    /// Panics if the event refers to classes or nodes the model has never seen, which means
    /// the trace was not recorded from the start of the e-graph's life.
    pub fn apply(&mut self, event: &Event<L, D>) {
        match event {
            Event::AddNode {
                node,
                class,
                op,
                children,
            } => {
                self.classes.insert(
                    *class,
                    ModelClass {
                        nodes: vec![*node],
                        data: None,
                    },
                );
                self.nodes.insert(
                    *node,
                    ModelNode {
                        op: op.clone(),
                        children: children.clone(),
                        class: *class,
                    },
                );
            }
            Event::Union { leader, loser } => {
                let lost = self
                    .classes
                    .remove(loser)
                    .unwrap_or_else(|| panic!("union of unknown class c{loser}"));
                self.class_parent.insert(*loser, *leader);
                self.classes
                    .get_mut(leader)
                    .unwrap_or_else(|| panic!("union into unknown class c{leader}"))
                    .nodes
                    .extend(lost.nodes);
            }
            Event::DropNode { node, kept } => {
                let dropped = self
                    .nodes
                    .remove(node)
                    .unwrap_or_else(|| panic!("drop of unknown node n{node}"));
                let class = self.find(dropped.class);
                self.classes
                    .get_mut(&class)
                    .unwrap()
                    .nodes
                    .retain(|n| n != node);
                self.node_parent.insert(*node, *kept);
            }
            Event::SetData { class, data } => {
                let class = self.find(*class);
                self.classes.get_mut(&class).unwrap().data = Some(data.clone());
            }
            Event::RebuildStart => self.rebuilding = true,
            Event::RebuildEnd => self.rebuilding = false,
            Event::Focus(target) => self.focus = *target,
            Event::Note(text) => self.note = Some(text.clone()),
        }
    }

    /// Canonical id of a class.
    pub fn find(&self, mut class: ClassId) -> ClassId {
        while let Some(&parent) = self.class_parent.get(&class) {
            class = parent;
        }
        class
    }

    /// Canonical id of a node (differs only for nodes dropped by a congruence).
    pub fn find_node(&self, mut node: NodeId) -> NodeId {
        while let Some(&parent) = self.node_parent.get(&node) {
            node = parent;
        }
        node
    }

    /// Canonical class containing a live node.
    pub fn class_of(&self, node: NodeId) -> Option<ClassId> {
        let node = self.nodes.get(&self.find_node(node))?;
        Some(self.find(node.class))
    }

    pub fn class(&self, class: ClassId) -> Option<&ModelClass<D>> {
        self.classes.get(&self.find(class))
    }

    pub fn node(&self, node: NodeId) -> Option<&ModelNode<L>> {
        self.nodes.get(&self.find_node(node))
    }

    pub fn num_classes(&self) -> usize {
        self.classes.len()
    }

    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// The current focus, canonicalized.
    pub fn focus(&self) -> Option<Target> {
        self.focus.map(|target| self.canonical(target))
    }

    /// The most recent note.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Whether the log is between a `RebuildStart` and its `RebuildEnd`.
    pub fn is_rebuilding(&self) -> bool {
        self.rebuilding
    }

    pub fn canonical(&self, target: Target) -> Target {
        match target {
            Target::Class(class) => Target::Class(self.find(class)),
            Target::Node(node) => Target::Node(self.find_node(node)),
        }
    }

    /// The model as a snapshot, labeled and styled by `view` (no highlights).
    pub fn snapshot(&self, view: &ReplayView<L, D>) -> Snapshot {
        let classes = self
            .classes
            .iter()
            .map(|(&id, class)| {
                let data = class.data.as_ref();
                let mut node_ids = class.nodes.clone();
                node_ids.sort_unstable();
                let nodes = node_ids
                    .into_iter()
                    .map(|nid| {
                        let node = &self.nodes[&nid];
                        VizNode {
                            id: nid,
                            label: (view.node_label)(nid, &node.op),
                            children: node.children.iter().map(|&c| self.find(c)).collect(),
                            style: (view.node_style)(nid, &node.op, data),
                        }
                    })
                    .collect();
                VizClass {
                    id,
                    label: (view.class_label)(id, data),
                    nodes,
                    style: (view.class_style)(id, data),
                }
            })
            .collect();
        Snapshot { classes }
    }
}

impl<L: Clone, D: Clone> Default for ReplayModel<L, D> {
    fn default() -> Self {
        Self::new()
    }
}
