/* Frontier Analysis */

use std::ops::Mul;

use crate::querylang::*;
use egg::*;

/// MulteNodes are nodes whose arguments are annotated with cost properties.
/// They are used to maintain a non-dominated frontier of nodes per e-class.
#[derive(Debug, Clone)]
pub struct MulteNode<'a, L: Language> {
    id: Id,
    node: &'a L,
    args: Vec<(Id, CostProperties)>,
    cost: CostProperties,
}

impl<'a> MulteNode<'a, QueryLang> {}

// TODO: implement the e-class analysis that maintains a non-dominated frontier of multe-nodes per class
#[derive(Debug, Clone)]
struct FrontierAnalysis<'a>(Vec<MulteNode<'a, QueryLang>>);

impl<'a> FrontierAnalysis<'a> {
    fn new() -> Self {
        FrontierAnalysis(vec![])
    }

    /// Inserts a new MulteNode into the frontier if it is not dominated by any existing node.
    /// A node is dominated if there exists another node with a lower or equal cost.
    /// If the new node is inserted, any existing nodes that are dominated by it are removed.
    /// Returns true if the node was inserted, false otherwise.
    fn insert(&mut self, multe_node: MulteNode<'a, QueryLang>) -> bool {
        // Check if the new node is dominated by any existing node
        for existing in &self.0 {
            if existing.cost <= multe_node.cost {
                return false; // New node is dominated, do not insert
            }
        }

        // Remove any existing nodes that are dominated by the new node
        self.0.retain(|existing| multe_node.cost < existing.cost);

        // Insert the new node
        self.0.push(multe_node);
        true
    }

    /// Merges another FrontierAnalysis into this one.
    /// Returns true if the frontier was changed, false otherwise.
    fn merge(&mut self, other: Self) -> DidMerge {
        let mut changed = false;
        for multe_node in other.0 {
            if self.insert(multe_node) {
                changed = true;
            }
        }
        // Conservative, assume if self changed then both changed
        DidMerge(changed, changed)
    }
}

impl<'a> Analysis<QueryLang> for FrontierAnalysis<'a> {
    type Data = Self;

    fn make(egraph: &mut EGraph<QueryLang, Self>, enode: &QueryLang, id: Id) -> Self::Data {
        // When we add a new enode to the e-graph, we need to consider all possible combinations of all arguments
        // And create a new MulteNode for each combination, and insert it into the frontier
        // (which keeps only the non-dominated nodes).

        todo!()
    }

    fn merge(&mut self, to: &mut Self::Data, from: Self::Data) -> DidMerge {
        // Merge the data from `from` into `to`.
        // Returns true if the frontier was changed, false otherwise.
        to.merge(from)
    }

    fn modify(egraph: &mut EGraph<QueryLang, Self>, id: Id) {
        // let data = egraph[id].data.clone();
        // egraph[id].data = data;
    }
}
