/* Frontier Analysis */

use crate::querylang::*;
use egg::*;

/// MulteNodes are nodes whose arguments are annotated with cost properties.
/// They are used to maintain a non-dominated frontier of nodes per e-class.
///
/// The argument costs are thresholds: choosing, for each argument, any entry of that class with a cost
/// at least as good as the threshold gives this node a cost at least as good as `cost`.
///
/// `rank` is the derivation depth: leaves have rank 0, and any other node has one more than the highest
/// rank among the argument entries it was computed from. Rank breaks ties in scalar cost: every threshold is
/// met by an entry of the argument class that either has a strictly lower scalar cost than the threshold or
/// has a strictly lower rank than this node (see `covers`). Since the scalar cost of a node is never lower
/// than that of any of its arguments, extraction strictly decreases (scalar cost, rank) lexicographically at
/// every step, so it always terminates, even on cyclic e-graphs.
#[derive(Debug, Clone)]
pub struct MulteNode<L: Language> {
    pub(crate) id: Id,
    pub(crate) node: L,
    pub(crate) args: Vec<(Id, CostProperties)>,
    pub(crate) cost: CostProperties,
    pub(crate) rank: usize,
}

impl MulteNode<QueryLang> {
    /// Rank-aware dominance: at least as good a cost, and either a strictly lower scalar cost or a
    /// derivation that is no deeper. Rank only matters between entries with the same scalar cost, which
    /// is what happens around zero-cost cycles (e.g. X = IndexScan(X) over an empty table).
    fn covers(&self, other: &Self) -> bool {
        self.cost <= other.cost && (self.cost.cost < other.cost.cost || self.rank <= other.rank)
    }
}

#[derive(Debug, Clone)]
pub struct FrontierAnalysis(pub(crate) Vec<MulteNode<QueryLang>>, pub(crate) Catalog);

impl FrontierAnalysis {
    pub fn new(catalog: Catalog) -> Self {
        FrontierAnalysis(vec![], catalog)
    }

    /// Inserts a new MulteNode into the frontier if it is not dominated by any existing node.
    /// A node is dominated if another node covers it (see `MulteNode::covers`).
    /// If the new node is inserted, any existing nodes that are dominated by it are removed.
    /// Returns true if the node was inserted, false otherwise.
    ///
    /// An entry is only ever replaced by one that covers it, and covering is transitive, so every threshold
    /// stays met by an entry with either a lower scalar cost than the threshold or a lower rank than the node
    /// that set it. An entry that is better but has the same scalar cost and a greater rank (e.g. one derived
    /// through a zero-cost cycle back into this class) is kept alongside the entry it would otherwise replace.
    fn insert(&mut self, multe_node: MulteNode<QueryLang>) -> bool {
        // Check if the new node is dominated by any existing node
        for existing in &self.0 {
            if existing.covers(&multe_node) {
                return false; // New node is dominated, do not insert
            }
        }

        // Remove any existing nodes that are dominated by the new node
        self.0.retain(|existing| !multe_node.covers(existing));

        // Insert the new node
        self.0.push(multe_node);
        true
    }

    /// Merges another FrontierAnalysis into this one.
    /// Returns DidMerge(self changed, result differs from other), as egg requires.
    fn merge(&mut self, other: Self) -> DidMerge {
        let mut inserted = 0;
        let mut rejected = false;
        for multe_node in other.0 {
            if self.insert(multe_node) {
                inserted += 1;
            } else {
                rejected = true;
            }
        }
        // Entries of `other` are mutually non-dominated, so none of them evicts another:
        // every entry beyond the inserted ones is left over from the original `self`.
        let kept_original = self.0.len() > inserted;
        // Conservative: an entry of `other` rejected as equal to an existing one also counts as a difference
        DidMerge(inserted > 0, rejected || kept_original)
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

        // 1. Collect the frontier of (cost, rank) for each argument position
        let arg_costs: Vec<Vec<(CostProperties, usize)>> = children
            .iter()
            .map(|&c| {
                egraph[c]
                    .data
                    .0
                    .iter()
                    .map(|m| (m.cost.clone(), m.rank))
                    .collect()
            })
            .collect();

        // 2. Build every combination of argument costs (cartesian product).
        // A leaf has no arguments, so it gets exactly one (empty) combination.
        let mut combos: Vec<Vec<(CostProperties, usize)>> = vec![vec![]];
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
            let rank = combo.iter().map(|(_, r)| r + 1).max().unwrap_or(0);
            let (combo, _): (Vec<CostProperties>, Vec<usize>) = combo.into_iter().unzip();
            let cost = frontier.1.op_cost(enode, &combo);
            let args: Vec<(Id, CostProperties)> = children.iter().copied().zip(combo).collect();
            frontier.insert(MulteNode {
                id,
                node: enode.clone(),
                args,
                cost,
                rank,
            });
        }
        frontier
    }

    fn merge(&mut self, to: &mut Self::Data, from: Self::Data) -> DidMerge {
        // Merge the data from `from` into `to`.
        // Returns true if the frontier was changed, false otherwise.
        to.merge(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn c(materialized: bool, index: Option<&str>, cost: usize) -> CostProperties {
        CostProperties {
            tables: HashSet::new(),
            cols: HashSet::new(),
            rows: 0,
            index: index.map(String::from),
            materialized,
            cost,
        }
    }

    fn mn(cost: CostProperties) -> MulteNode<QueryLang> {
        mnr(cost, 0)
    }

    fn mnr(cost: CostProperties, rank: usize) -> MulteNode<QueryLang> {
        MulteNode {
            id: Id::from(0usize),
            node: QueryLang::Name("n".into()),
            args: vec![],
            cost,
            rank,
        }
    }

    fn frontier(costs: &[CostProperties]) -> FrontierAnalysis {
        let mut f = FrontierAnalysis::new(Catalog::new());
        for k in costs {
            f.insert(mn(k.clone()));
        }
        f
    }

    fn costs(f: &FrontierAnalysis) -> Vec<CostProperties> {
        f.0.iter().map(|m| m.cost.clone()).collect()
    }

    /// No frontier entry covers another, and every inserted entry is covered by some frontier entry
    fn assert_valid_frontier(f: &FrontierAnalysis, inserted: &[MulteNode<QueryLang>]) {
        let entries = || {
            f.0.iter()
                .map(|m| (m.cost.clone(), m.rank))
                .collect::<Vec<_>>()
        };
        for (i, a) in f.0.iter().enumerate() {
            for (j, b) in f.0.iter().enumerate() {
                assert!(
                    i == j || !a.covers(b),
                    "entry covers another in frontier: {:?}",
                    entries()
                );
            }
        }
        for k in inserted {
            assert!(
                f.0.iter().any(|m| m.covers(k)),
                "{:?} not covered by frontier {:?}",
                (&k.cost, k.rank),
                entries()
            );
        }
    }

    #[test]
    fn insert_rejects_dominated_and_equal() {
        let mut f = frontier(&[c(true, None, 1)]);
        assert!(!f.insert(mn(c(false, None, 5))), "dominated");
        assert!(!f.insert(mn(c(true, None, 1))), "equal");
        assert_eq!(f.0.len(), 1);
    }

    #[test]
    fn insert_removes_dominated_entries() {
        let mut f = frontier(&[c(false, None, 5), c(false, Some("a"), 7), c(true, None, 9)]);
        assert_eq!(f.0.len(), 3);
        assert!(f.insert(mn(c(true, Some("a"), 1))));
        assert_eq!(costs(&f), vec![c(true, Some("a"), 1)]);
    }

    #[test]
    fn insert_keeps_incomparable_entries() {
        let f = frontier(&[
            c(true, None, 9),
            c(false, None, 1),
            c(false, Some("a"), 5),
            c(false, Some("b"), 5),
        ]);
        assert_eq!(f.0.len(), 4);
    }

    #[test]
    fn insert_is_rank_aware() {
        let ranks = |f: &FrontierAnalysis| {
            f.0.iter()
                .map(|m| (m.cost.cost, m.rank))
                .collect::<Vec<_>>()
        };
        // A strictly cheaper entry replaces a dominated one regardless of rank
        let mut f = frontier(&[]);
        assert!(f.insert(mnr(c(false, None, 5), 1)));
        assert!(f.insert(mnr(c(false, None, 1), 3)));
        assert_eq!(ranks(&f), vec![(1, 3)]);

        // With the same scalar cost, a better but deeper entry is kept alongside the shallower one
        let mut f = frontier(&[]);
        assert!(f.insert(mnr(c(false, None, 5), 1)));
        assert!(f.insert(mnr(c(true, None, 5), 2)));
        assert_eq!(f.0.len(), 2);
        // ...and one that is no worse and no deeper replaces both
        assert!(f.insert(mnr(c(true, None, 5), 1)));
        assert_eq!(ranks(&f), vec![(5, 1)]);

        // A deeper entry with the same cost as a shallower one is rejected
        assert!(!f.insert(mnr(c(true, None, 5), 2)));
    }

    #[test]
    fn insert_maintains_frontier_invariant() {
        // Deterministic pseudo-random insertion sequences over a small cost domain
        let mut seed: u64 = 0x2545F4914F6CDD1D;
        let mut next = move |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        let indexes = [None, Some("a"), Some("b")];
        for _ in 0..200 {
            let mut f = FrontierAnalysis::new(Catalog::new());
            let mut inserted = vec![];
            for _ in 0..30 {
                let k = c(next(2) == 1, indexes[next(3) as usize], next(6) as usize);
                let k = mnr(k, next(4) as usize);
                f.insert(k.clone());
                inserted.push(k);
                assert_valid_frontier(&f, &inserted);
            }
        }
    }

    #[test]
    fn merge_combines_frontiers() {
        let a = [c(true, None, 5), c(false, Some("a"), 5)];
        let b = [c(true, None, 3), c(false, Some("b"), 1)];
        let mut f = frontier(&a);
        let changed = f.merge(frontier(&b));
        assert!(changed.0);
        let all: Vec<_> = a.iter().chain(&b).cloned().map(mn).collect();
        assert_valid_frontier(&f, &all);
        assert_eq!(f.0.len(), 3); // (true, None, 5) is dominated by (true, None, 3)

        // Merging in nothing new reports no change
        let mut g = frontier(&a);
        assert!(!g.merge(frontier(&[c(false, None, 9)])).0);
        assert_eq!(costs(&g), a.to_vec());
    }

    #[test]
    fn merge_flags() {
        let (cheap, dear) = (c(true, None, 1), c(true, None, 9));
        let (sorted, other) = (c(false, Some("a"), 5), c(false, Some("b"), 5));
        let flags = |to: &[CostProperties], from: &[CostProperties]| {
            let d = frontier(to).merge(frontier(from));
            (d.0, d.1)
        };
        assert_eq!(
            flags(&[cheap.clone()], &[dear.clone()]),
            (false, true),
            "result differs from `from`"
        );
        assert_eq!(
            flags(&[dear.clone()], &[cheap.clone()]),
            (true, false),
            "result is `from`"
        );
        assert_eq!(
            flags(&[sorted.clone()], &[other.clone()]),
            (true, true),
            "incomparable: both change"
        );
        assert_eq!(flags(&[], &[cheap.clone()]), (true, false));
        assert_eq!(flags(&[cheap.clone()], &[]), (false, true));
        assert_eq!(
            flags(&[dear.clone(), sorted.clone()], &[cheap.clone()]),
            (true, true),
            "one original survives"
        );
    }

    #[test]
    fn union_repairs_parents_of_absorbed_class() {
        // The cheap class has more parents, so egg keeps it and absorbs the expensive class. The parent
        // of the expensive class must still be recomputed with the cheaper argument.
        let cat = Catalog::new()
            .with_table("A".into(), 100)
            .with_table("A2".into(), 100);
        let mut eg: EGraph<QueryLang, FrontierAnalysis> = EGraph::new(FrontierAnalysis::new(cat));
        let col = |eg: &mut EGraph<QueryLang, FrontierAnalysis>, n: &str| {
            let n = eg.add(QueryLang::Name(n.into()));
            eg.add(QueryLang::Column([n]))
        };
        let (cx, cy, cz) = (col(&mut eg, "x"), col(&mut eg, "y"), col(&mut eg, "z"));
        let a = eg.add(QueryLang::Name("A".into()));
        let cheap = eg.add(QueryLang::Table([a])); // cost 0
        eg.add(QueryLang::ExhaustiveSelect([cheap, cy]));
        eg.add(QueryLang::ExhaustiveSelect([cheap, cz]));
        let a2 = eg.add(QueryLang::Name("A2".into()));
        let ta2 = eg.add(QueryLang::Table([a2]));
        let dear = eg.add(QueryLang::SeqScan([ta2])); // cost 100
        let parent = eg.add(QueryLang::ExhaustiveSelect([dear, cx])); // cost 100 + 100
        assert_eq!(eg[parent].data.0[0].cost.cost, 200);

        eg.union(cheap, dear);
        eg.rebuild();
        let parent = eg.find(parent);
        assert_eq!(costs(&eg[parent].data), vec![c(false, None, 100)]);
    }

    #[test]
    fn make_builds_frontier_per_argument_position() {
        let cat = Catalog::new().with_table("B".into(), 10);
        let mut eg: EGraph<QueryLang, FrontierAnalysis> = EGraph::new(FrontierAnalysis::new(cat));
        let b = eg.add(QueryLang::Name("B".into()));
        let x = eg.add(QueryLang::Name("x".into()));
        let cx = eg.add(QueryLang::Column([x]));
        assert_eq!(eg[b].data.0.len(), 1, "leaf gets exactly one entry");

        // A class holding two incomparable costs: Table(B) (materialized) and Index(B, x) (sorted)
        let tb = eg.add(QueryLang::Table([b]));
        let ib = eg.add(QueryLang::Index([b, x]));
        eg.union(tb, ib);
        eg.rebuild();
        let tb = eg.find(tb);
        assert_eq!(eg[tb].data.0.len(), 2);

        // The same class in both join positions: the best entry uses a different cost in each
        let j = eg.add(QueryLang::NestedLoopJoin([tb, tb, cx]));
        let f = &eg[j].data.0;
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].cost.index.as_deref(), Some("x"));
        assert_ne!(f[0].args[0].1, f[0].args[1].1);
        assert!(f[0].args.iter().map(|(i, _)| *i).eq([tb, tb, cx]));
    }
}
