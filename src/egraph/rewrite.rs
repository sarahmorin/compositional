use crate::egraph::core::{ClassId, Node};
/// Rewrites for transforming expressions in the e-graph.
use crate::egraph::language::{Language, Pattern};

pub struct Rewrite<L>
where
    L: Language,
{
    pub name: String,
    pub search: Pattern<L>,
    pub apply: Pattern<L>,
}

impl<L> Rewrite<L>
where
    L: Language,
{
    pub fn new(name: &str, search: Pattern<L>, apply: Pattern<L>) -> Self {
        Self {
            name: name.to_string(),
            search,
            apply,
        }
    }
}
