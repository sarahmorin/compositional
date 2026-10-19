use crate::lattice::{MeetLattice, WithTop};
/// Top down analysis for egg

/// Current analysis is bottom up:
/// - When we insert a node, we compute its analysis value
/// - When we merge two classes, we merge their analysis values
/// - When the analysis value of a class changes, we propagate the change to its parents
///
/// In the frontier analysis, this means we compute the full non-dominated frontier of a class
/// even if no one is asking for elements of that frontier.
///
/// We want a top-down analysis where we only compute analysis values at extraction time and when someone asks.
/// So, instead of computing the analysis during rewriting, we do standard rewriting and then work on the fixed e-graph.
/// So we don't need to hook into the e-graph as it mutates, we just need to hook into the extraction process.
/// At extraction time, we are going to ask threshold questions and use the analysis to answer them
/// (and cache the answers for future questions).
/// Some starting limits:
/// - We fix computation order, so we will not cleverly pick which arguments to compute first, we will just compute them in the order they are given.
/// - We do the two versions of thresholds:
///     - once something satisfies the threshold, we return it. We don't look for the "best thing" that satisfies the threshold?
///     - all the things that satisfy the threshold, so we can pick among them later.
/// - We will still use the Multe-node analysis structure, so we don't need to materialize terms we just cache a way to quickly look them up for cost bounds
///
/// Interface: TopDownAnalysis trait
/// - Starting point: Fixed e-graph (Language), Cost domain and cost function
/// - Threshold questions: Given an e-class and value in the cost domain, return a multe-node (index into the analysis value of the class)
///     that satisfies the threshold (or None if no such multe-node exists)
/// - Once you are done asking threshold questions, you can extract from the multe-nodes very quickly
/// - Inverting the cost function is tricky, so we have two options:
///     1. We can require an "invertible" cost function that takes (cost bound, operator) -> (cost bound for each argument)
///         such that any combination of arguments that satisfies the argument cost bounds will satisfy the operator cost bound.
///         This is a strong requirement and may not be possible for all cost functions.
///    2. We can require a "threshold" function that takes (cost bound, operator) -> (cost bound for each argument)
///         such that any combination of arguments that satisfy their cost bound *might* satisfy the operator cost bound.
///         We can verify if they do satisfy the bound by calling the cost function.
///         This is a weaker requirement and may be possible for more cost functions.
/// - For now, we'll implement the second option, but we can add a config option or something to the trait that
///     allows the user to specify that the cost function is invertible and the check is unnecessary.
///
///
///
/// Threshold algorithm (E: e-class, C: cost bound):
/// 1. If E.analysis(C) is cached, return it
/// 1a. If E.analysis(C) is not cached, but E.analysis(C') is cached for some C' < C,
///     then we can use that to speed up the computation of E.analysis(C)?
/// 2. Otherwise, for each n in E:
///     queue = {([Top; k], 0)}
///     while costs, i = queue.pop():
///         if i == n.children.len():
///             cost = cost(n, costs)
///             if cost <= C:
///                E.analysis(C).push((MulteNode(n, costs)), cost))
///         costs_in = backwards_costs(n, C, costs)
///         - verify costs_in < costs -
///         arg_opts = threshold(n.children[i], costs_in[i])
///         for opt in arg_opts:
///             costs_in[i] = opt
///             queue.push((costs_in, i + 1))
/// 3. Return E.analysis(C)
///
///
///
/// query takes an Id a something and returns another something that is STRICTLY BETTER than that something
/// - you can always go back for more (i.e. with a more specific question)
/// frontier to frontier
///
///
/// ToDo list:
/// - [ ] We proabably want a "downset" of partial order cost domain generic implementation
/// - [ ] Define the generic top down analysis trait
/// - [ ] Do some boiler plate top down analysis implementations for min/max lattice and "down set" lattices
use egg::*;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Top-down E-class analysis
///
/// A top-down analysis only computes an analysis value for an e-class when it is queried.
/// We compute the partial analysis value for an e-class by querying the class with a threshold,
/// once the analysis value reaches that threshold, we return it and cache our analysis progress for future queries.
pub trait TopDownAnalysis<L, C>
where
    L: Language,
    C: MeetLattice + WithTop,
{
    /// Query an e-class with a given threshold.
    /// If we compute an analysis value that is less than or equal to the threshold, we return it.
    /// Otherwise return None
    fn query_class(&mut self, eclass: &Id, threshold: C) -> Option<C>;

    /// Query a node with a given threshold.
    /// Return an analysis value for the node.
    /// If the analysis value is less than or equal to the threshold, we may have stopped early.
    /// Otherwise, we have computed the full analysis value for the node.
    fn query_node(&mut self, enode: &L, threshold: C) -> C;
}

pub trait ComputeAnalysis<L, C>
where
    L: Language,
    C: MeetLattice + WithTop,
{
    /// Given a node and its children's analysis values, return the node's analysis value.
    fn compute_analysis(&mut self, enode: &L, child_analyses: &[C]) -> C;
}

pub trait InvertibleCostFunction<L, C>
where
    L: Language,
    C: MeetLattice + WithTop,
{
    /// Given a cost bound and an operator, return a cost bound for each argument such that any combination of arguments that satisfies the argument cost bounds will satisfy the operator cost bound.
    fn invert_cost(&self, cost_bound: C, operator: &L) -> Vec<C>;
}

pub struct BasicTopDownAnalysis<L, C>
where
    L: Language,
    C: MeetLattice + WithTop + Eq + Hash,
{
    egraph: EGraph<L, ()>,
    analysis: HashMap<Id, C>,
    completed: HashMap<Id, bool>,
    nodes: HashMap<Id, Vec<L>>,
    outstanding_queries: HashMap<Id, Vec<C>>,
}

impl<L, C> BasicTopDownAnalysis<L, C>
where
    L: Language,
    C: MeetLattice + WithTop + Eq + Hash,
{
    pub fn new(egraph: EGraph<L, ()>) -> Self {
        Self {
            egraph,
            analysis: HashMap::new(),
            completed: HashMap::new(),
            nodes: HashMap::new(),
            outstanding_queries: HashMap::new(),
        }
    }

    fn egraph(&self) -> &EGraph<L, ()> {
        &self.egraph
    }

    // Get the analysis value for an e-class, if it exists.
    pub fn get_analysis(&self, eclass: &Id) -> Option<&C> {
        self.analysis.get(eclass)
    }

    /// Set the analysis value for an e-class.
    fn set_analysis(&mut self, eclass: &Id, value: C) {
        // TODO: Add checks to ensure that the value is valid for the eclass and that it is not worse than the current value.
        self.analysis.insert(*eclass, value);
    }

    /// Mark an e-class as completed.
    fn set_completed(&mut self, eclass: &Id, completed: bool) {
        self.completed.insert(*eclass, completed);
    }

    /// Check if an e-class is completed.
    fn is_completed(&self, eclass: &Id) -> bool {
        self.completed.get(eclass).cloned().unwrap_or(false)
    }

    /// Check if there is an outstanding query for the given eclass and threshold.
    fn has_outstanding_query(&self, eclass: &Id, threshold: C) -> bool {
        if let Some(queries) = self.outstanding_queries.get(eclass) {
            for query in queries {
                // QUESTION: Should we check for equality or less than or equal to?
                // If we check for less than or equal to, we might miss some queries that are strictly better than the threshold.
                if *query == threshold {
                    return true;
                }
            }
            false
        } else {
            false
        }
    }

    fn add_outstanding_query(&mut self, eclass: &Id, threshold: C) {
        self.outstanding_queries
            .entry(*eclass)
            .or_insert_with(Vec::new)
            .push(threshold);
    }

    fn remove_outstanding_query(&mut self, eclass: &Id, threshold: C) {
        if let Some(queries) = self.outstanding_queries.get_mut(&eclass) {
            queries.remove(queries.iter().position(|x| *x == threshold).unwrap());
        } else {
            panic!("Trying to remove an outstanding query that doesn't exist");
        }
    }

    fn push_node(&mut self, eclass: &Id, node: L) {
        self.nodes
            .entry(*eclass)
            .or_insert_with(Vec::new)
            .push(node);
    }

    fn pop_node(&mut self, eclass: &Id) -> Option<L> {
        if let Some(nodes) = self.nodes.get_mut(&eclass) {
            nodes.pop()
        } else {
            None
        }
    }
}

impl<L, C> TopDownAnalysis<L, C> for BasicTopDownAnalysis<L, C>
where
    L: Language,
    C: MeetLattice + WithTop + Eq + Hash,
    Self: ComputeAnalysis<L, C>,
{
    /// Query an e-class with a given threshold.
    /// If we compute an analysis value that is less than or equal to the threshold, we return it.
    /// Otherwise return None
    fn query_class(&mut self, eclass: &Id, threshold: C) -> Option<C> {
        // Check for a cycle in the query graph.
        if self.has_outstanding_query(eclass, threshold.clone()) {
            // If we have already queried this eclass with this threshold, return None to indicate a cycle.
            return None;
        }

        // Lookup the current analysis value and unprocessed nodes for the eclass, or initialize them if they don't exist.
        let mut analysis = self.get_analysis(eclass).cloned().unwrap_or_else(|| {
            self.analysis.insert(*eclass, C::top());
            self.nodes
                .insert(*eclass, self.egraph()[*eclass].nodes.clone());
            C::top()
        });

        // If we have already computed an analysis value that is less than the threshold, return it.
        if analysis < threshold {
            return Some(analysis.clone());
        }

        if self.completed.get(&eclass).cloned().unwrap_or(false) {
            // If we have already completed the analysis for this eclass, return None to indicate that no such value exists.
            return None;
        }

        // Add the current query to the set of outstanding queries.
        self.add_outstanding_query(eclass, threshold.clone());

        // Otherwise, process the remaining unprocessed nodes in th eclass until we find an analysis value
        // that is less than or equal to the threshold
        while let Some(node) = self.pop_node(eclass) {
            // Query the node with the given threshold to get its analysis value.
            let node_analysis = self.query_node(&node, threshold.clone());
            // Update the analysis value for the eclass with the new node analysis value.
            analysis.meet_into(&node_analysis);
            self.set_analysis(eclass, analysis.clone());

            // If the new analysis value is less than or equal to the threshold, we can return it.
            // NOTE: This only happens when the most recently processed node has an analysis value that
            // is less than or equal to the threshold. There is a chance that we got that value without
            // processing all the children of the node, so we might have missed some better analysis values.
            // So we push the current node back onto the stack in case we need to finish it for a future query.
            // If the node didn't beat the threshold, then we must have complete it and there's no need to push
            // it back onto the stack for future queries.
            if analysis < threshold {
                // Push the potentially incomplete node back onto the stack for future queries.
                self.push_node(eclass, node);
                // Remove the current query from the set of outstanding queries.
                self.remove_outstanding_query(eclass, threshold);
                // Return the current analysis value for the eclass, which is less than or equal to the threshold.
                return Some(analysis.clone());
            }
        }

        // If we have processed all nodes and still haven't found an analysis value that is less than or equal to the threshold,
        // we return None to indicate that no such value exists.
        self.remove_outstanding_query(eclass, threshold);
        self.set_completed(eclass, true);
        None
    }

    /// Query a node with a given threshold.
    /// This is the most naive implementation of query_node where we simply get some analysis value for each child,
    /// Check if we satisfy the threshold, and if not, continue to ask for better analysis values for each child until
    /// we either satisfy the threshold or exhaust all options.
    fn query_node(&mut self, enode: &L, threshold: C) -> C {
        // If the node has no children, we can just return the analysis value for the node.
        // Other than leaves, we don't call the usual make since it would be bottom up
        if enode.children().is_empty() {
            return self.compute_analysis(&enode, &[]);
        }

        // If the node has children, we need to query each child and combine their analysis values.
        let mut child_costs = Vec::new();
        for child in enode.children() {
            child_costs.push(
                self.query_class(child, C::top())
                    .unwrap_or_else(|| C::top()),
            );
        }

        let mut analysis = self.compute_analysis(&enode, &child_costs);
        let mut child_queue: Vec<(usize, &Id)> = enode.children().iter().enumerate().collect();
        while !(analysis < threshold) {
            if let Some((i, child)) = child_queue.pop() {
                // If the child has no more options, we can't improve the analysis value for the node.
                if self.is_completed(child) {
                    continue;
                }
                // Otherwise, we query the child with a better threshold to try to improve the analysis value for the node.
                let child_analysis = self.query_class(child, child_costs[i].clone());
                // If we got a new analysis value for the child, we need to recompute the analysis value for the node.
                if let Some(new_child_analysis) = child_analysis {
                    child_costs[i] = new_child_analysis.clone();
                    analysis = self.compute_analysis(&enode, &child_costs);
                    // If the child is not completed, we need to push it back onto the queue to try again later
                    if !self.is_completed(child) {
                        child_queue.push((i, child));
                    }
                }
            } else {
                // If we have exhausted all children and still haven't found an analysis value that is less than or equal to the threshold,
                // we return the current analysis value for the node, which is greater than the threshold.
                break;
            }
        }

        analysis
    }
}
