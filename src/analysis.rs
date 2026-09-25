/* Frontier Analysis */

use crate::querylang::*;
use egg::*;

/// MulteNodes are nodes whose arguments are annotated with cost properties.
/// They are used to maintain a non-dominated frontier of nodes per e-class.
#[derive(Debug, Clone)]
pub struct MulteNode<L: Language> {
    id: Id,
    node: L,
    args: Vec<(Id, CostProperties)>,
    cost: CostProperties,
}

impl MulteNode<QueryLang> {}

// TODO: implement the e-class analysis that maintains a non-dominated frontier of multe-nodes per class
#[derive(Debug, Clone)]
struct FrontierAnalysis(Vec<MulteNode<QueryLang>>, Catalog);

impl FrontierAnalysis {
    fn new(catalog: Catalog) -> Self {
        FrontierAnalysis(vec![], catalog)
    }

    /// Inserts a new MulteNode into the frontier if it is not dominated by any existing node.
    /// A node is dominated if there exists another node with a lower or equal cost.
    /// If the new node is inserted, any existing nodes that are dominated by it are removed.
    /// Returns true if the node was inserted, false otherwise.
    fn insert(&mut self, multe_node: MulteNode<QueryLang>) -> bool {
        // Check if the new node is dominated by any existing node
        for existing in &self.0 {
            if existing.cost <= multe_node.cost {
                return false; // New node is dominated, do not insert
            }
        }

        // Remove any existing nodes that are dominated by the new node
        self.0.retain(|existing| !(multe_node.cost <= existing.cost));

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

impl Analysis<QueryLang> for FrontierAnalysis {
    type Data = Self;

    fn make(egraph: &mut EGraph<QueryLang, Self>, enode: &QueryLang, id: Id) -> Self::Data {
        // When we add a new enode to the e-graph, we need to consider all possible combinations of all arguments
        // And create a new MulteNode for each combination, and insert it into the frontier
        // (which keeps only the non-dominated nodes).

        // Canonicalize children, since they may not be canonical when make is called.
        // Children are kept per argument position (not deduplicated), so an argument class that
        // appears in several positions can take a different cost from its frontier in each position.
        let children: Vec<Id> = enode.children().iter().map(|&c| egraph.find(c)).collect();

        // 1. Collect the frontier of costs for each argument position
        let arg_costs: Vec<Vec<CostProperties>> = children
            .iter()
            .map(|&c| egraph[c].data.0.iter().map(|m| m.cost.clone()).collect())
            .collect();

        // 2. Build every combination of argument costs (cartesian product).
        // A leaf has no arguments, so it gets exactly one (empty) combination.
        let mut combos: Vec<Vec<CostProperties>> = vec![vec![]];
        for costs in &arg_costs {
            combos = combos
                .iter()
                .flat_map(|prefix| {
                    costs.iter().map(move |c| {
                        let mut next = prefix.clone();
                        next.push(c.clone());
                        next
                    })
                })
                .collect();
        }

        // 3. Compute the cost of this enode for each combination and insert it into the frontier
        let mut frontier = FrontierAnalysis::new(egraph.analysis.1.clone());
        for combo in combos {
            let cost = frontier.1.op_cost(enode, &combo);
            let args: Vec<(Id, CostProperties)> = children.iter().copied().zip(combo).collect();
            frontier.insert(MulteNode {
                id,
                node: enode.clone(),
                args,
                cost,
            });
        }
        frontier
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
