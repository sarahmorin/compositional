/// Simple trait for a language of operators that can be used in an e-graph.
use std::fmt::Debug;
use std::hash::Hash;
use std::{collections::HashSet, ops::Sub};

use crate::egraph::core::{ClassId, Node};
pub trait Language: Clone + Debug + Hash + Eq {
    fn matches(&self, other: &Self) -> bool;
    fn arity(&self) -> usize;
}

/// A term in the language is a fully grounded expression
pub struct Term<L>
where
    L: Language,
{
    pub op: L,
    pub children: Vec<Term<L>>,
}

/// Op or var distinguishes between an operator and a variable in a pattern.
pub enum OpOrVar<L>
where
    L: Language,
{
    Op(L),
    Var(String),
}

impl<L> OpOrVar<L>
where
    L: Language,
{
    pub fn is_var(&self) -> bool {
        matches!(self, OpOrVar::Var(_))
    }

    pub fn matches(&self, other: &L) -> bool {
        match self {
            OpOrVar::Op(op) => op.matches(other),
            OpOrVar::Var(_) => true,
        }
    }
}

/// A pattern is a tree of operators and variables that can be matched against terms in the language.
pub struct Pattern<L>
where
    L: Language,
{
    pub root: OpOrVar<L>,
    pub children: Vec<Pattern<L>>,
    pub vars: HashSet<String>,
}

impl<L> Pattern<L>
where
    L: Language,
{
    /// Create a new pattern with the given root operator and children patterns.
    pub fn new(root: L, children: Vec<Pattern<L>>) -> Self {
        let mut vars = HashSet::new();
        for child in &children {
            for var in &child.vars {
                vars.insert(var.clone());
            }
        }
        Self {
            root: OpOrVar::Op(root),
            children,
            vars,
        }
    }

    /// Create a new pattern that is just a variable.
    pub fn new_empty() -> Self {
        Self {
            root: OpOrVar::Var(String::new()),
            children: Vec::new(),
            vars: HashSet::new(),
        }
    }
}
