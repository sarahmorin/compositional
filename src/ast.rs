use crate::egraph::{
    analysis::Analysis,
    core::{EGraph, Node, NodeId},
    language::Language,
};

/// MinAST analysis keeps the minimum cost for each e-class and the corresponding NodeId
pub(crate) struct MinAST;
impl<L> Analysis<L> for MinAST
where
    L: Language,
{
    type Data = (NodeId, usize); // (NodeId, cost)

    // Make is called when a new node is added to the e-graph.
    // Nodes are created in new classes with matching ids?
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
