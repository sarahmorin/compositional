/// Extraction and cost function interfaces for the e-graph.
use crate::egraph::analysis::Analysis;
use crate::egraph::core::{ClassId, EGraph, NodeId};
use crate::egraph::language::{Language, Term};
use std::collections::HashMap;

/// A very simple cost function interface for composable cost functions.
/// The cost function is not inherently related to an e-graph, so it only depends on the language and the cost domain.
pub trait CostFunction<L: Language, D: PartialOrd + Clone> {
    fn cost(&self, op: &L, children: &[D]) -> D;

    /// Compute the cost of a term using the cost function.
    /// Default implementation is recursive and does not memoize, so it may be inefficient.
    /// It is mostly for comparing extracted results and for testing.
    fn term_cost(&self, term: &Term<L>) -> D {
        let costs: Vec<D> = term.children.iter().map(|c| self.term_cost(c)).collect();
        self.cost(&term.op, &costs)
    }
}

/// Index of a term in the extractor's arena.
type TermRef = usize;

/// Stack depths of the blocked nodes a result depends on, sorted ascending.
/// Empty means the result does not depend on the stack (it is "clean").
type Deps = Vec<usize>;

/// Result of computing the best term for a class or node in the current DFS context.
#[derive(Clone, Debug)]
struct Res<D> {
    /// Cost and term of the best candidate, or `None` if every candidate is cyclic.
    best: Option<(D, TermRef)>,
    deps: Deps,
}

/// Extracts lowest-cost terms from a fixed e-graph by a top-down DFS with memoization.
///
/// Cycles are broken by never re-entering a node whose cost computation is still outstanding
/// (i.e. that is on the DFS stack). Such a node is *blocked*; its class can still be entered
/// through its other nodes.
///
/// Memoization: a class result computed while some blocked node was skipped may be worse than
/// the class's true best, since that best might go through the blocked node. So each result
/// records the stack depths of the blocked nodes it depends on (`Deps`):
/// - A node frame at depth `k` discharges dependency `k` when it finishes: no term rooted at a
///   node needs that same node again below it.
/// - Clean results (no deps) are memoized permanently.
/// - Results with deps are memoized only until the deepest frame they depend on pops, which is
///   exactly how long the context they were computed in stays the same.
///
/// The returned term is finite and its cost equals `cost_fn.term_cost(&term)`. When the cost
/// function is monotone (cheaper children never make the parent more expensive), it is a
/// lowest-cost term. See `best_class` for how ties and incomparable costs are handled.
///
/// The e-graph should be rebuilt before extraction. Memoized results are reused across calls
/// on the same extractor.
pub struct Extractor<'a, L, N, C, D>
where
    L: Language,
    N: Analysis<L>,
    C: CostFunction<L, D>,
    D: PartialOrd + Clone,
{
    egraph: &'a EGraph<L, N>,
    cost_fn: &'a C,
    memo: HashMap<ClassId, Res<D>>,
    /// Nodes with an outstanding cost computation, mapped to their depth on the DFS stack.
    on_stack: HashMap<NodeId, usize>,
    /// `evict[d]` holds the memoized classes to drop when the frame at depth `d` pops.
    /// Its length is the current stack depth.
    evict: Vec<Vec<ClassId>>,
    /// Extracted terms as an arena of shared subterms; entries only refer to earlier entries,
    /// so terms are immutable snapshots and can never be cyclic. Entries of evicted results
    /// are not reclaimed.
    arena: Vec<(L, Vec<TermRef>)>,
}

impl<'a, L, N, C, D> Extractor<'a, L, N, C, D>
where
    L: Language,
    N: Analysis<L>,
    C: CostFunction<L, D>,
    D: PartialOrd + Clone,
{
    pub fn new(egraph: &'a EGraph<L, N>, cost_fn: &'a C) -> Self {
        Self {
            egraph,
            cost_fn,
            memo: HashMap::new(),
            on_stack: HashMap::new(),
            evict: Vec::new(),
            arena: Vec::new(),
        }
    }

    /// The lowest-cost term in `class` and its cost.
    ///
    /// Returns `None` only if every term in the class is cyclic. This can't happen for an
    /// e-graph built with `add_node`/`add_term`/`union`: each class's first node only refers
    /// to classes that existed before it, so every class has a finite term. (`None` does occur
    /// internally, for classes whose candidates are all blocked in the current DFS context.)
    pub fn extract_class(&mut self, class: ClassId) -> Option<(D, Term<L>)> {
        let res = self.best_class(class);
        debug_assert!(res.deps.is_empty());
        let (cost, term) = res.best?;
        Some((cost, self.build(term)))
    }

    /// The lowest-cost term rooted at `node` and its cost. `None` under the same conditions as
    /// `extract_class`.
    pub fn extract_node(&mut self, node: NodeId) -> Option<(D, Term<L>)> {
        let res = self.best_node(node);
        debug_assert!(res.deps.is_empty());
        let (cost, term) = res.best?;
        Some((cost, self.build(term)))
    }

    // NOTE: `best_class` and `best_node` are mutually recursive, so the Rust call stack grows
    // with the depth of the extracted term (plus detours through cycles). That is fine for the
    // e-graph sizes we use now; if deep e-graphs overflow the stack, revisit this and make the
    // DFS iterative with an explicit stack.

    fn best_class(&mut self, class: ClassId) -> Res<D> {
        let egraph = self.egraph;
        let class = egraph.find(class);
        if let Some(res) = self.memo.get(&class) {
            return res.clone();
        }
        let mut best: Option<(D, TermRef)> = None;
        let mut deps = Deps::new();
        for &node in &egraph.class(class).nodes {
            let node = egraph.find_node(node);
            if let Some(&depth) = self.on_stack.get(&node) {
                merge(&mut deps, &[depth]);
                continue;
            }
            let res = self.best_node(node);
            merge(&mut deps, &res.deps);
            if let Some((cost, term)) = res.best {
                // DESIGN DECISION: replace the incumbent only if the new cost is strictly
                // less. Ties and incomparable costs keep the first candidate found, so the
                // result is deterministic but depends on the order of `class.nodes`.
                if best.as_ref().is_none_or(|(incumbent, _)| cost < *incumbent) {
                    best = Some((cost, term));
                }
            }
        }
        let res = Res { best, deps };
        if let Some(&deepest) = res.deps.last() {
            self.evict[deepest].push(class);
        }
        self.memo.insert(class, res.clone());
        res
    }

    fn best_node(&mut self, node: NodeId) -> Res<D> {
        let egraph = self.egraph;
        let node = egraph.find_node(node);
        let depth = self.evict.len();
        self.on_stack.insert(node, depth);
        self.evict.push(Vec::new());

        let enode = egraph.get_node(node);
        let mut deps = Deps::new();
        let mut costs = Vec::with_capacity(enode.children.len());
        let mut terms = Vec::with_capacity(enode.children.len());
        for &child in &enode.children {
            let res = self.best_class(child);
            merge(&mut deps, &res.deps);
            match res.best {
                Some((cost, term)) => {
                    costs.push(cost);
                    terms.push(term);
                }
                None => break,
            }
        }
        let best = (terms.len() == enode.children.len()).then(|| {
            let cost = self.cost_fn.cost(&enode.root, &costs);
            self.arena.push((enode.root.clone(), terms));
            (cost, self.arena.len() - 1)
        });

        // Pop this frame: results that depended on it being blocked are now stale.
        self.on_stack.remove(&node);
        for class in self.evict.pop().unwrap() {
            let evicted = self.memo.remove(&class);
            debug_assert!(evicted.is_some_and(|r| r.deps.last() == Some(&depth)));
        }
        // Discharge the dependency on this frame. Deeper ones were discharged by their frames.
        if deps.last() == Some(&depth) {
            deps.pop();
        }
        debug_assert!(deps.last().is_none_or(|&d| d < depth));
        Res { best, deps }
    }

    fn build(&self, term: TermRef) -> Term<L> {
        let (op, children) = &self.arena[term];
        Term::new(
            op.clone(),
            children.iter().map(|&c| self.build(c)).collect(),
        )
    }
}

/// Sorted-set union of `other` into `into`.
fn merge(into: &mut Deps, other: &[usize]) {
    if !other.is_empty() {
        into.extend_from_slice(other);
        into.sort_unstable();
        into.dedup();
    }
}
