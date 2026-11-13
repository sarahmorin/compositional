/* Frontier Extraction */

use crate::analysis::*;
use crate::querylang::*;
use egg::*;

/// Extracts plans from an e-graph using only the frontiers maintained by [`FrontierAnalysis`].
///
/// Each frontier entry is a plan template: an e-node plus a cost threshold for each argument. To extract
/// an entry, we choose, for each argument, an entry of that argument's class that meets the threshold and
/// either has a strictly lower scalar cost than the threshold or a strictly lower rank than the parent, and
/// recurse. Since the cost function is monotone, the extracted plan costs at most the entry's cost, and since
/// (scalar cost, rank) strictly decreases at every step, extraction terminates even on cyclic e-graphs.
///
/// Each argument position is extracted independently, so the same class can get different plans in
/// different positions and the result is a tree rather than a DAG.
pub struct FrontierExtractor<'a> {
    egraph: &'a EGraph<QueryLang, FrontierAnalysis>,
}

impl<'a> FrontierExtractor<'a> {
    pub fn new(egraph: &'a EGraph<QueryLang, FrontierAnalysis>) -> Self {
        Self { egraph }
    }

    /// Extracts every non-dominated plan for `root`, paired with the cost bound from its frontier entry.
    /// Entries whose cost is top (e.g. logical operators) are skipped.
    pub fn find_frontier(&self, root: Id) -> Vec<(CostProperties, RecExpr<QueryLang>)> {
        let root = self.egraph.find(root);
        self.egraph[root]
            .data
            .0
            .iter()
            .filter(|entry| !entry.cost.is_top())
            .map(|entry| (entry.cost.clone(), self.extract(entry)))
            .collect()
    }

    /// Extracts the plan for a single frontier entry.
    pub fn extract(&self, entry: &MulteNode<QueryLang>) -> RecExpr<QueryLang> {
        let mut expr = RecExpr::default();
        self.build(entry, &mut expr);
        expr
    }

    fn build(&self, entry: &MulteNode<QueryLang>, expr: &mut RecExpr<QueryLang>) -> Id {
        let children: Vec<Id> = entry
            .args
            .iter()
            .map(|(class, threshold)| {
                let child = self.choose(*class, threshold, entry.rank);
                self.build(child, expr)
            })
            .collect();
        // Arguments are stored in child order, so replace the node's children positionally
        let mut children = children.into_iter();
        let node = entry
            .node
            .clone()
            .map_children(|_| children.next().unwrap());
        expr.add(node)
    }

    /// Chooses an entry of `class` that meets `threshold` and either has a strictly lower scalar cost than
    /// `threshold` or a rank below `rank`. Among those, prefers the lowest rank, which gives the shallowest plan.
    fn choose(
        &self,
        class: Id,
        threshold: &CostProperties,
        rank: usize,
    ) -> &'a MulteNode<QueryLang> {
        let class = self.egraph.find(class);
        self.egraph[class]
            .data
            .0
            .iter()
            .filter(|entry| {
                entry.cost <= *threshold && (entry.cost.cost < threshold.cost || entry.rank < rank)
            })
            .min_by_key(|entry| entry.rank)
            .expect("frontier invariant violated: no entry meets the threshold")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Graph = EGraph<QueryLang, FrontierAnalysis>;

    fn graph(cat: &Catalog) -> Graph {
        EGraph::new(FrontierAnalysis::new(cat.clone()))
    }

    fn name(eg: &mut Graph, n: &str) -> Id {
        eg.add(QueryLang::Name(n.into()))
    }

    /// Every entry of every class extracts to a plan whose actual cost is at least as good as the entry's bound
    fn assert_sound(eg: &Graph, cat: &Catalog) {
        let ex = FrontierExtractor::new(eg);
        for class in eg.classes() {
            for (bound, plan) in ex.find_frontier(class.id) {
                let actual = cat.clone().cost_rec(&plan);
                assert!(
                    actual <= bound,
                    "{plan}: actual {actual:?} exceeds bound {bound:?}"
                );
            }
        }
    }

    fn plans(eg: &Graph, root: Id) -> Vec<String> {
        let mut plans: Vec<String> = FrontierExtractor::new(eg)
            .find_frontier(root)
            .into_iter()
            .map(|(_, plan)| plan.to_string())
            .collect();
        plans.sort();
        plans
    }

    #[test]
    fn extracts_plan_matching_frontier() {
        let cat = Catalog::new().with_table("A".into(), 100);
        let mut eg = graph(&cat);
        let a = name(&mut eg, "A");
        let x = name(&mut eg, "x");
        let ta = eg.add(QueryLang::Table([a]));
        let cx = eg.add(QueryLang::Column([x]));
        let scan = eg.add(QueryLang::SeqScan([ta]));
        let select = eg.add(QueryLang::ExhaustiveSelect([scan, cx]));
        eg.rebuild();
        assert_eq!(
            plans(&eg, select),
            vec!["(X_SELECT (SEQ_SCAN (T- A)) (C- x))"]
        );
        assert_sound(&eg, &cat);
    }

    #[test]
    fn skips_top_entries() {
        let cat = Catalog::new().with_table("A".into(), 100);
        let mut eg = graph(&cat);
        let a = name(&mut eg, "A");
        let ta = eg.add(QueryLang::Table([a]));
        let scan = eg.add(QueryLang::Scan(ta));
        assert!(
            plans(&eg, scan).is_empty(),
            "logical operators are not extractable"
        );
        let seq = eg.add(QueryLang::SeqScan([ta]));
        eg.union(scan, seq);
        eg.rebuild();
        assert_eq!(plans(&eg, scan), vec!["(SEQ_SCAN (T- A))"]);
    }

    #[test]
    fn extracts_different_plans_for_the_same_class() {
        // Table(B) (materialized) and Index(B, x) (sorted) are incomparable entries of one class,
        // and the best self-join uses the sorted entry on the left and the materialized one on the right
        let cat = Catalog::new().with_table("B".into(), 10);
        let mut eg = graph(&cat);
        let b = name(&mut eg, "B");
        let x = name(&mut eg, "x");
        let cx = eg.add(QueryLang::Column([x]));
        let tb = eg.add(QueryLang::Table([b]));
        let ib = eg.add(QueryLang::Index([b, x]));
        eg.union(tb, ib);
        eg.rebuild();
        let join = eg.add(QueryLang::NestedLoopJoin([tb, tb, cx]));
        eg.rebuild();
        assert_eq!(plans(&eg, join), vec!["(NL_JOIN (I- B x) (T- B) (C- x))"]);
        assert_sound(&eg, &cat);
    }

    #[test]
    fn terminates_on_zero_cost_cycle() {
        // X = Index(E, x) = IndexScan(X) over an empty table: the IndexScan entry is cheaper than the Index entry
        // it was derived from, but deeper, so both are kept and extraction bottoms out at the Index entry
        let cat = Catalog::new().with_table("E".into(), 0);
        let mut eg = graph(&cat);
        let e = name(&mut eg, "E");
        let x = name(&mut eg, "x");
        let ix = eg.add(QueryLang::Index([e, x]));
        let scan = eg.add(QueryLang::IndexScan([ix]));
        eg.union(ix, scan);
        eg.rebuild();
        assert_eq!(plans(&eg, ix), vec!["(I- E x)", "(INDEX_SCAN (I- E x))"]);
        assert_sound(&eg, &cat);
    }

    #[test]
    fn sound_on_cyclic_egraph() {
        // Several physical alternatives per class, with unions that create cycles through SeqScan and IndexScan
        let cat = Catalog::new()
            .with_table("A".into(), 100)
            .with_table("B".into(), 10)
            .with_index("A".into(), "x".into());
        let mut eg = graph(&cat);
        let (a, b, x) = (name(&mut eg, "A"), name(&mut eg, "B"), name(&mut eg, "x"));
        let cx = eg.add(QueryLang::Column([x]));
        let ta = eg.add(QueryLang::Table([a]));
        let tb = eg.add(QueryLang::Table([b]));
        let ia = eg.add(QueryLang::Index([a, x]));
        let ib = eg.add(QueryLang::Index([b, x]));
        let seq_a = eg.add(QueryLang::SeqScan([ta]));
        let iscan_a = eg.add(QueryLang::IndexScan([ia]));
        let iscan_b = eg.add(QueryLang::IndexScan([ib]));
        eg.union(ta, seq_a); // A = SeqScan(A)
        eg.union(ta, ia);
        eg.union(ta, iscan_a); // A = IndexScan(A)
        eg.union(tb, ib);
        eg.union(tb, iscan_b); // B = IndexScan(B)
        eg.rebuild();
        let nl = eg.add(QueryLang::NestedLoopJoin([ta, tb, cx]));
        let ms = eg.add(QueryLang::MergeSortJoin([ta, tb, cx]));
        let sel = eg.add(QueryLang::SortSelect([ta, cx]));
        let xsel = eg.add(QueryLang::ExhaustiveSelect([ta, cx]));
        eg.union(nl, ms);
        eg.union(sel, xsel);
        eg.rebuild();
        let sel_join = eg.add(QueryLang::SortSelect([nl, cx]));
        eg.rebuild();
        assert!(!plans(&eg, sel_join).is_empty());
        assert_sound(&eg, &cat);
    }
}
