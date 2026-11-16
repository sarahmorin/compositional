/// Core e-graph data structures like Ids, Nodes, and Classes.

pub type NodeId = usize;
pub type ClassId = usize;
use crate::egraph::analysis::Analysis;
use crate::egraph::language::Language;
use crate::egraph::unionfind::UnionFind;
use bimap::BiMap;
use std::collections::{HashMap, HashSet};

/// Node structure carries its NodeId and the corresponding enode.
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

/// E-class structure representing a set of equivalent expressions (enodes).
#[derive(Clone, Debug)]
pub struct EClass<L, N> {
    pub id: ClassId,
    pub nodes: Vec<Node<L>>,
    pub data: N,
    pub parents: HashSet<(NodeId, ClassId)>,
}

impl<L, N> EClass<L, N> {
    pub fn new(id: ClassId, nodes: Vec<Node<L>>, data: N) -> Self {
        Self {
            id,
            nodes,
            data,
            parents: HashSet::new(),
        }
    }

    pub fn add_parent(&mut self, parent_node_id: NodeId, parent_class_id: ClassId) {
        self.parents.insert((parent_node_id, parent_class_id));
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
    /// Classes in the e-graph.
    pub classes: Vec<EClass<L, N>>,
    /// Unionfind of equivalence classes
    pub unionfind: UnionFind,
    /// Mapping from enodes to their corresponding class ids.
    pub hashcons: HashMap<Node<L>, ClassId>,
    /// Mapping from nodes to their unique NodeIds
    pub nodes: BiMap<Node<L>, NodeId>,
    /// Unionfind for tracking NodeIds of dropped nodes
    pub node_uf: UnionFind,

    /// Worklist of nodes that need to be processed for analysis updates.
    pub worklist: Vec<NodeId>,

    /// Analysis worklist of classes that need to be processed for analysis updates.
    pub analysis_worklist: Vec<ClassId>,
}

impl<L, N> EGraph<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    pub fn new(analysis: N) -> Self {
        Self {
            analysis,
            classes: Vec::new(),
            unionfind: UnionFind::new(),
            hashcons: HashMap::new(),
            nodes: BiMap::new(),
            node_uf: UnionFind::new(),
            worklist: Vec::new(),
            analysis_worklist: Vec::new(),
        }
    }
    // TODO: Implement rest of e-graph
}
