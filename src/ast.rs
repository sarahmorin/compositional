use crate::egraph::{
    analysis::Analysis,
    core::{ClassId, EGraph, Node, NodeId},
    extract::CostFunction,
    language::{Language, Pattern, Term},
    rewrite::Subst,
};
use std::collections::HashMap;

/// MinAST analysis keeps the minimum cost for each e-class and the corresponding NodeId
#[derive(Clone, Copy, Debug, Default)]
pub struct MinAST;
impl<L> CostFunction<L, usize> for MinAST
where
    L: Language,
{
    fn cost(&self, _op: &L, children: &[usize]) -> usize {
        // The cost of a node is 1 plus the sum of the costs of its children.
        1 + children.iter().sum::<usize>()
    }
}

impl<L> Analysis<L> for MinAST
where
    L: Language,
{
    type Data = (NodeId, usize); // (NodeId, cost)

    // Make is called when a node is added and when its children's costs change.
    fn make(egraph: &mut EGraph<L, Self>, enode: &Node<L>, id: NodeId) -> Self::Data {
        // Compute the cost of the new node based on its children.
        let cost: usize = enode.children.iter().fold(1, |sum: usize, child_id| {
            let child_cost = egraph.class(*child_id).data.1;
            sum.saturating_add(child_cost)
        });

        (id, cost)
    }

    fn merge(&mut self, a: &mut Self::Data, b: Self::Data) -> (bool, bool) {
        // Keep the node with the smaller cost. If costs are equal, keep the existing one.
        if a.1 < b.1 {
            (false, true)
        } else if a.1 == b.1 {
            (false, false)
        } else {
            *a = b;
            (true, false)
        }
    }
}

impl<L> EGraph<L, MinAST>
where
    L: Language,
{
    /// The lowest-cost node in `class`, according to the analysis.
    pub fn best_node(&self, class: ClassId) -> NodeId {
        self.find_node(self.class(class).data.0)
    }

    /// Matches `pattern` at `node`, seeing every class below `node` only through its best node.
    pub fn match_restricted(&self, pattern: &Pattern<L>, node: NodeId) -> Option<Subst> {
        self.match_node_with_view(pattern, node, &|egraph, class| egraph.best_node(class))
    }

    /// The best term in `root` and its cost, read directly off the analysis data.
    /// Requires a rebuilt e-graph.
    pub fn extract_min_ast(&self, root: ClassId) -> (usize, Term<L>) {
        let choice: HashMap<ClassId, NodeId> = self
            .classes()
            .map(|class| (class.id, self.find_node(class.data.0)))
            .collect();
        let term = self.unfold(&choice, root);
        let cost = self.class(root).data.1;
        debug_assert_eq!(MinAST.term_cost(&term), cost, "MinAST data is stale");
        (cost, term)
    }

    // Terminates because each best node costs strictly more than any of its children's classes,
    // so following best nodes can never cycle.
    fn unfold(&self, choice: &HashMap<ClassId, NodeId>, class: ClassId) -> Term<L> {
        let node = self.get_node(choice[&self.find(class)]);
        let children = node.children.iter().map(|&c| self.unfold(choice, c)).collect();
        Term::new(node.root.clone(), children)
    }
}
