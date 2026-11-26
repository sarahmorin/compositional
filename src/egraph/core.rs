/// Core e-graph data structures like Ids, Nodes, and Classes.

pub type NodeId = usize;
pub type ClassId = usize;
use crate::egraph::analysis::Analysis;
use crate::egraph::language::{Language, Term};
use crate::egraph::trace::{Event, Target, Trace};
use crate::egraph::unionfind::UnionFind;
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

/// An e-node: an operator applied to child e-classes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node<L> {
    pub root: L,
    pub children: Vec<ClassId>,
    // TODO: add analysis info?
}

impl<L> Node<L> {
    pub fn new(root: L, children: Vec<ClassId>) -> Self {
        Self { root, children }
    }
}

/// NodeClass is a pair of NodeId and ClassId, representing a node and the class it belongs to.
/// We don't make persistent use of the NodeClass anywhere, but we do return NodeClass from insertions
/// and some lookups as a convenience during exploration. The NodeClass returned by one of these operations
/// is only up-to-date at the time of the call. It is not maintained by the e-graph and will become stale
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeClass(NodeId, ClassId);

impl NodeClass {
    pub fn new(node: NodeId, class: ClassId) -> Self {
        Self(node, class)
    }

    pub fn node(&self) -> NodeId {
        self.0
    }

    pub fn class(&self) -> ClassId {
        self.1
    }
}

/// E-class structure representing a set of equivalent expressions (enodes).
#[derive(Clone, Debug)]
pub struct EClass<D: Clone> {
    pub id: ClassId,
    /// Ids of the nodes in this class. After a rebuild, all of them are canonical.
    pub nodes: Vec<NodeId>,
    pub data: D,
    /// Nodes that have this class as a child, with the class each node was in when it was added.
    /// Both ids may be stale; canonicalize them with `find_node` and `find`.
    pub parents: HashSet<NodeClass>,
}

impl<D: Clone> EClass<D> {
    pub fn new(id: ClassId, nodes: Vec<NodeId>, data: D) -> Self {
        Self {
            id,
            nodes,
            data,
            parents: HashSet::new(),
        }
    }

    pub fn add_parent(&mut self, parent: NodeClass) {
        self.parents.insert(parent);
    }
}

#[derive(Clone, Debug)]
pub struct EGraph<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    /// Analysis associated with the e-graph.
    pub analysis: N,
    /// Class Ids to EClass
    pub classes: HashMap<ClassId, EClass<N::Data>>,
    /// Unionfind of equivalence classes
    pub unionfind: UnionFind,
    /// Mapping from enodes to their corresponding class ids.
    pub hashcons: HashMap<Node<L>, ClassId>,
    /// Mapping from nodes to their unique NodeIds
    pub nodes: BiMap<NodeId, Node<L>>,
    /// Unionfind for tracking NodeIds of dropped nodes
    pub node_uf: UnionFind,

    /// Worklist of nodes whose children may have changed and need to be re-canonicalized.
    pub worklist: Vec<NodeClass>,

    /// Analysis worklist of classes whose data changed, so their parents need updating.
    pub analysis_worklist: Vec<ClassId>,

    /// Event log for replay; `None` unless recording was started. See `trace`.
    trace: Option<Trace<L, N::Data>>,
}

impl<L, N> EGraph<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    pub fn new(analysis: N) -> Self {
        Self {
            analysis,
            classes: HashMap::new(),
            unionfind: UnionFind::new(),
            hashcons: HashMap::new(),
            nodes: BiMap::new(),
            node_uf: UnionFind::new(),
            worklist: Vec::new(),
            analysis_worklist: Vec::new(),
            trace: None,
        }
    }

    // ---------------------------------------------------------- recording
    //
    // Recording is a runtime switch: when it is off, each emission point costs one `Option`
    // check. TODO(perf): if that ever shows up in benchmarks, gate the bodies of `record`,
    // `focus` and `note` behind a `replay` cargo feature so they compile to nothing. All
    // emission goes through those three methods, so the switch only touches them.

    /// Starts logging events. Has no effect if already recording.
    pub fn start_recording(&mut self) {
        if self.trace.is_none() {
            self.trace = Some(Trace::new());
        }
    }

    /// Stops recording and returns the events logged so far.
    pub fn take_recording(&mut self) -> Option<Trace<L, N::Data>> {
        self.trace.take()
    }

    pub fn is_recording(&self) -> bool {
        self.trace.is_some()
    }

    /// The events logged so far, without stopping the recording.
    pub fn recording(&self) -> Option<&Trace<L, N::Data>> {
        self.trace.as_ref()
    }

    /// Logs that the driver is now looking at `target` (`None` clears the focus).
    pub fn focus(&mut self, target: Option<Target>) {
        self.record(|_| Event::Focus(target));
    }

    pub fn focus_class(&mut self, class: ClassId) {
        self.focus(Some(Target::Class(class)));
    }

    pub fn focus_node(&mut self, node: NodeId) {
        self.focus(Some(Target::Node(node)));
    }

    /// Logs a message. The closure only runs while recording, so formatting is free otherwise.
    pub fn note(&mut self, message: impl FnOnce() -> String) {
        self.record(|_| Event::Note(message()));
    }

    /// Appends the event built by `event`, which only runs while recording.
    #[inline]
    fn record(&mut self, event: impl FnOnce(&Self) -> Event<L, N::Data>) {
        if self.trace.is_some() {
            let event = event(self);
            self.trace.as_mut().unwrap().events.push(event);
        }
    }

    /// Logs the current analysis data of `class`.
    fn record_data(&mut self, class: ClassId) {
        self.record(|eg| Event::SetData {
            class,
            data: eg.classes[&class].data.clone(),
        });
    }

    // ------------------------------------------------------------- metrics

    pub fn num_classes(&self) -> usize {
        self.classes.len()
    }

    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    // ---------------------------------------------------------------- ids

    /// Canonical id of a class.
    pub fn find(&self, class: ClassId) -> ClassId {
        self.unionfind.find(class)
    }

    /// Canonical id of a node. Differs from `node` only if a rebuild dropped it as a duplicate.
    pub fn find_node(&self, node: NodeId) -> NodeId {
        self.node_uf.find(node)
    }

    /// The (canonical) class with the given id.
    pub fn class(&self, class: ClassId) -> &EClass<N::Data> {
        &self
            .classes
            .get(&self.find(class))
            .expect("class does not exist")
    }

    fn class_mut(&mut self, class: ClassId) -> &mut EClass<N::Data> {
        let class = self.find(class);
        self.classes
            .get_mut(&class)
            .expect("class id is canonical but not in egraph")
    }

    /// Iterates over all canonical classes.
    pub fn classes(&self) -> impl Iterator<Item = &EClass<N::Data>> {
        self.classes.iter().map(|(_, class)| class)
    }

    /// The node with the given id.
    pub fn get_node(&self, node: NodeId) -> &Node<L> {
        self.nodes.get_by_left(&self.find_node(node)).unwrap()
    }

    /// The canonical NodeClass containing the given node.
    pub fn node_class(&self, node: NodeId) -> NodeClass {
        NodeClass(
            self.find_node(node),
            self.find(self.hashcons[&self.get_node(node)]),
        )
    }

    fn canonicalize(&self, mut node: Node<L>) -> Node<L> {
        for child in &mut node.children {
            *child = self.find(*child);
        }
        node
    }

    // ---------------------------------------------------------- insertion

    /// Adds a node in a new class and returns its id.
    /// If an equal node is already in the e-graph, returns that node's id instead and sets `added` to false.
    /// Otherwise returns the new node's id and sets `added` to true.
    pub fn add_node(&mut self, node: Node<L>) -> (NodeClass, bool) {
        let node = self.canonicalize(node);
        if let Some(&id) = self.nodes.get_by_right(&node) {
            return (NodeClass::new(id, self.hashcons[&node]), false);
        }
        let id = self.node_uf.make_set();
        let class = self.unionfind.make_set();
        for &child in &node.children {
            self.class_mut(child).add_parent(NodeClass::new(id, class));
        }
        self.nodes.insert(id, node.clone());
        self.hashcons.insert(node.clone(), class);
        // The new class does not exist yet, so `make` may only look at the node's children.
        let data = N::make(self, &node, id);
        // Check that the class id matches the classes len
        self.classes
            .insert(class, EClass::new(class, vec![id], data));
        self.record(|_| Event::AddNode {
            node: id,
            class,
            op: node.root.clone(),
            children: node.children.clone(),
        });
        self.record_data(class);
        (NodeClass::new(id, class), true)
    }

    /// Adds a node and merges it into `class`. Call `rebuild` before the next search.
    pub fn add_node_to(&mut self, node: Node<L>, class: ClassId) -> NodeClass {
        let (node_class, _added) = self.add_node(node);
        self.union(node_class.class(), class);
        // Return the node id and the canonical class id
        NodeClass::new(node_class.node(), self.find(class))
    }

    /// Adds a term bottom-up and returns the id of its root node.
    pub fn add_term(&mut self, term: &Term<L>) -> (NodeClass, bool) {
        let children = term
            .children
            .iter()
            .map(|child| self.add_term(child).0.class())
            .collect();
        self.add_node(Node::new(term.op.clone(), children))
    }

    // ------------------------------------------------------------- lookup
    // These are only exact on a rebuilt e-graph.

    /// Id of a node equal to `node`, if there is one.
    pub fn lookup_node(&self, node: &Node<L>) -> Option<NodeClass> {
        if let Some(node_id) = self
            .nodes
            .get_by_right(&self.canonicalize(node.clone()))
            .copied()
        {
            Some(self.node_class(node_id))
        } else {
            None
        }
    }

    /// Id of the root node representing `term`, if the e-graph represents it.
    pub fn lookup_term(&self, term: &Term<L>) -> Option<NodeClass> {
        let children = term
            .children
            .iter()
            .map(|child| Some(self.lookup_term(child)?.class()))
            .collect::<Option<Vec<_>>>()?;
        self.lookup_node(&Node::new(term.op.clone(), children))
    }

    /// Whether `class` represents `term`.
    pub fn class_contains(&self, class: ClassId, term: &Term<L>) -> bool {
        self.lookup_term(term)
            .is_some_and(|node_class| node_class.class() == self.find(class))
    }

    /// Whether `node` is the root node of `term`.
    pub fn node_represents(&self, node: NodeId, term: &Term<L>) -> bool {
        self.lookup_term(term)
            .is_some_and(|found| found.node() == self.find_node(node))
    }

    // ------------------------------------------------- union and rebuild

    /// Merges two classes and returns false if they were already the same class.
    /// Congruence and analysis data are restored by the next `rebuild`.
    pub fn union(&mut self, a: ClassId, b: ClassId) -> bool {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return false;
        }
        // Keep the class with more parents as the leader, so fewer parents move.
        if self.class(a).parents.len() < self.class(b).parents.len() {
            std::mem::swap(&mut a, &mut b);
        }
        self.unionfind.union(a, b);
        let loser = self.classes.get(&b).unwrap().clone();
        // The loser's parents now have a non-canonical child.
        self.worklist
            .extend(loser.parents.iter().map(|&node_class| node_class));

        let leader = self.classes.get_mut(&a).unwrap();
        leader.nodes.extend(loser.nodes);
        leader.parents.extend(loser.parents);
        let (a_changed, b_changed) = self.analysis.merge(&mut leader.data, loser.data);
        if a_changed || b_changed {
            self.analysis_worklist.push(a);
        }

        // Remove the loser class from the e-graph
        self.classes.remove(&b);
        self.record(|_| Event::Union {
            leader: a,
            loser: b,
        });
        if a_changed {
            self.record_data(a);
        }
        true
    }

    /// Restores congruence closure and analysis data after unions.
    pub fn rebuild(&mut self) {
        let pending = !self.worklist.is_empty() || !self.analysis_worklist.is_empty();
        if pending {
            self.record(|_| Event::RebuildStart);
        }
        while let Some(node_class) = self.worklist.pop() {
            self.repair_node(node_class);
        }
        // Analysis repair never unions, so it cannot add to the node worklist.
        while let Some(class) = self.analysis_worklist.pop() {
            self.repair_analysis(class);
        }
        if pending {
            self.record(|_| Event::RebuildEnd);
        }
    }

    /// Re-canonicalizes a node's children. If the node now equals an existing node, the two
    /// are congruent: drop this node (forwarding its id to the existing one) and union
    /// their classes.
    fn repair_node(&mut self, node_class: NodeClass) {
        let id = self.find_node(node_class.node());
        let (_, old) = self.nodes.remove_by_left(&id).unwrap();
        let class = self.hashcons.remove(&old).unwrap();
        let new = self.canonicalize(old);

        if let Some(&existing) = self.nodes.get_by_right(&new) {
            self.record(|_| Event::DropNode {
                node: id,
                kept: existing,
            });
            self.node_uf.union(existing, id);
            self.class_mut(class).nodes.retain(|&n| n != id);
            let other = self.hashcons[&new];
            self.union(class, other);
        } else {
            self.nodes.insert(id, new.clone());
            self.hashcons.insert(new, class);
        }
    }

    /// Recomputes the data of every parent of `class` and merges it into the parent's class.
    fn repair_analysis(&mut self, class: ClassId) {
        let parents: Vec<_> = self.class(class).parents.iter().copied().collect();
        for node_class in parents {
            let id = self.find_node(node_class.node());
            let node = self.get_node(id).clone();
            let parent = self.find(node_class.class());
            let data = N::make(self, &node, id);
            let parent_class = self.classes.get_mut(&parent).unwrap();
            if self.analysis.merge(&mut parent_class.data, data).0 {
                self.analysis_worklist.push(parent);
                self.record_data(parent);
            }
        }
    }
}
