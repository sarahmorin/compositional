/// Rewrites for transforming expressions in the e-graph, and the e-matching they use.
use crate::egraph::analysis::Analysis;
use crate::egraph::core::{ClassId, EGraph, Node, NodeId};
use crate::egraph::language::{Language, OpOrVar, Pattern};
use std::collections::HashMap;

/// A substitution from pattern variables to the classes they matched.
pub type Subst = HashMap<String, ClassId>;

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
        assert!(
            apply.vars.is_subset(&search.vars),
            "rewrite {name}: apply pattern uses variables not bound by the search pattern"
        );
        Self {
            name: name.to_string(),
            search,
            apply,
        }
    }
}

impl<L, N> EGraph<L, N>
where
    L: Language,
    N: Analysis<L>,
{
    /// All substitutions under which `pattern` matches some node in `class`.
    pub fn match_class(&self, pattern: &Pattern<L>, class: ClassId) -> Vec<Subst> {
        self.match_class_with(pattern, class, &Subst::new())
    }

    /// All substitutions under which `pattern` matches with its root at `node`.
    pub fn match_node(&self, pattern: &Pattern<L>, node: NodeId) -> Vec<Subst> {
        self.match_node_with(pattern, node, &Subst::new())
    }

    fn match_class_with(&self, pattern: &Pattern<L>, class: ClassId, subst: &Subst) -> Vec<Subst> {
        let class = self.find(class);
        match &pattern.root {
            OpOrVar::Var(var) => match subst.get(var) {
                Some(&bound) => {
                    if self.find(bound) != class {
                        vec![]
                    } else {
                        vec![subst.clone()]
                    }
                }
                None => {
                    let mut subst = subst.clone();
                    subst.insert(var.clone(), class);
                    vec![subst]
                }
            },
            OpOrVar::Op(_) => self
                .class(class)
                .nodes
                .iter()
                .flat_map(|&node| self.match_node_with(pattern, node, subst))
                .collect(),
        }
    }

    fn match_node_with(&self, pattern: &Pattern<L>, node: NodeId, subst: &Subst) -> Vec<Subst> {
        // If pattern is a variable, we can match it against the class of the node.
        let OpOrVar::Op(op) = &pattern.root else {
            return self.match_class_with(pattern, self.node_class(node).class(), subst);
        };
        // Otherwise lookup the node and match operator and arity first
        let node = self.get_node(node);
        if !op.matches(&node.root) || node.children.len() != pattern.children.len() {
            return vec![];
        }
        // Match each child pattern against the corresponding child class, threading every
        // partial substitution through.
        pattern.children.iter().zip(&node.children).fold(
            vec![subst.clone()],
            |substs, (child, &class)| {
                substs
                    .iter()
                    .flat_map(|subst| self.match_class_with(child, class, subst))
                    .collect()
            },
        )
    }

    pub fn incremental_match<'a>(
        &self,
        pattern: &'a Pattern<L>,
        node: NodeId,
        subst: &Subst,
    ) -> (Option<Subst>, Vec<(ClassId, &'a Pattern<L>)>) {
        match &pattern.root {
            OpOrVar::Var(var) => match subst.get(var) {
                Some(&bound) => {
                    if self.find(bound) != self.node_class(node).class() {
                        (None, vec![])
                    } else {
                        (Some(subst.clone()), vec![])
                    }
                }
                None => {
                    let mut subst = subst.clone();
                    subst.insert(var.clone(), self.node_class(node).class());
                    (Some(subst), vec![])
                }
            },
            OpOrVar::Op(op) => {
                let node = self.get_node(node);
                if !op.matches(&node.root) || node.children.len() != pattern.children.len() {
                    return (None, vec![]);
                }
                let res_matches = pattern
                    .children
                    .iter()
                    .zip(&node.children)
                    .map(|(child, &class)| (class, child))
                    .collect();

                (Some(subst.clone()), res_matches)
            }
        }
    }

    /// Adds the term described by `pattern` under `subst` and returns its class.
    pub fn instantiate(&mut self, pattern: &Pattern<L>, subst: &Subst) -> ClassId {
        match &pattern.root {
            OpOrVar::Var(var) => self.find(subst[var]),
            OpOrVar::Op(op) => {
                let children = pattern
                    .children
                    .iter()
                    .map(|child| self.instantiate(child, subst))
                    .collect();
                let node_class = self.add_node(Node::new(op.clone(), children));
                node_class.class()
            }
        }
    }

    // TODO: Incremental matching with continuations

    /// Matches the search side of `rewrite` with its root at `node`, instantiates the apply
    /// side for every match, and unions the result into `node`'s class.
    ///
    /// Returns the ids of nodes created by this call that ended up in `node`'s class.
    /// Existing nodes that joined the class through a union are not included.
    /// Does not rebuild: call `rebuild` before the next search.
    pub fn try_rewrite_node(&mut self, rewrite: &Rewrite<L>, node: NodeId) -> Vec<NodeId> {
        // NodeIds are handed out in increasing order, so ids >= first_new were created here.
        let first_new = self.node_uf.size();
        let class = self.node_class(node).class();
        for subst in self.match_node(&rewrite.search, node) {
            let result = self.instantiate(&rewrite.apply, &subst);
            self.union(class, result);
        }
        self.class(class)
            .nodes
            .iter()
            .copied()
            .filter(|&n| n >= first_new)
            .collect()
    }
}
