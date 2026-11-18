/// E-class and E-node analysis for compositional optimization.
use crate::egraph::core::*;
use crate::egraph::language::Language;

pub trait Analysis<L: Language>: Sized {
    type Data: Clone;

    /// Create a new analysis data for a node.
    fn make(egraph: &mut EGraph<L, Self>, enode: &Node<L>, id: NodeId) -> Self::Data;

    fn merge(&mut self, a: &mut Self::Data, b: Self::Data) -> (bool, bool);
}
