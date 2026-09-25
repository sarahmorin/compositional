/* A simple DB Query language to test with */

use egg::{CostFunction, Id, Language, define_language};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    usize,
};

const READ_COST: usize = 1; // Cost of reading a row from disk
const SELECTIVITY: f64 = 0.1; // Selectivity of a selection predicate
const SORT_SELECTION_FACTOR: f64 = 0.5; // Factor by which a sort selection reduces the number of rows scanned, assuming the selection predicate is on a sorted column

/// A simple query language for testing the e-graph and extractor.
define_language! {
    pub enum QueryLang {
        // Constants: Column, Table, and Index
        Name(String),
        "T-" = Table([Id; 1]),
        "C-" = Column([Id; 1]),
        "I-" = Index([Id; 2]),

        // Logical Operators
        "SCAN"   = Scan(Id),             // Scan(Table)
        "SELECT" = Select([Id; 2]), // Select([Table, Columns]) -> simplified model of selection where we just track the columns referenced in the selection predicate and elide the actual predicate itself.
        "JOIN"   = Join([Id; 3]),       // Join([Table, Table, Columns]) -> simplified model of join where we just track the tables and columns referenced in the join predicate and elide the actual predicate itself.

        // Physical Operators

        "SEQ_SCAN" = SeqScan([Id; 1]),       // SeqScan(Table)
        "INDEX_SCAN" = IndexScan([Id; 1]),   // IndexScan(Index)
        "NL_JOIN" = NestedLoopJoin([Id; 3]), // NestedLoopJoin([Table, Table, Columns])
        "MS_JOIN" = MergeSortJoin([Id; 3]), // MergeSortJoin([Table, Table, Columns])
        "X_SELECT" = ExhaustiveSelect([Id; 2]), // ExhaustiveSelect([Table, Columns])
        "S_SELECT" = SortSelect([Id; 2]), // SortSelect([Table, Columns])
    }
}

/// Physical Properties that can be used to annotate e-classes in the e-graph. These properties can be used to guide the extraction process and to prune the search space.
#[derive(Debug, Clone, Eq)]
pub struct CostProperties {
    // Tables that contribute to this result
    pub tables: HashSet<String>,
    // Columns that contribute to this result
    pub cols: HashSet<String>,
    // Number of rows in the result
    pub rows: usize,
    // Sort index of the result, if any. None means unsorted.
    pub index: Option<String>,
    // Whether the result is materialized or not.
    pub materialized: bool,
    // Cost of computation, in arbitrary units.
    pub cost: usize,
}

// Partial Ordering on the Cost Domain.
// The cost domain is partially ordered by the following rules:
// - Materialized results are always preferred over non-materialized results.
// - More sorted results are preferred over less sorted results.
// - Lower cost results are preferred over higher cost results.
// The rows property is only used to compute further costs of upstream results.
//
// Less means preferred. One result is less than another if it is at least as good on every
// property and strictly better on at least one; they are equal if they tie on every property.
// Otherwise (each is better on some property, or they are sorted on different indexes) they are incomparable.
impl PartialOrd for CostProperties {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::*;

        let materialized = other.materialized.cmp(&self.materialized);
        let index = match (&self.index, &other.index) {
            (a, b) if a == b => Equal,
            (Some(_), None) => Less,
            (None, Some(_)) => Greater,
            _ => return None, // Sorted on different indexes
        };
        let cost = self.cost.cmp(&other.cost);

        let props = [materialized, index, cost];
        if props.iter().all(|&o| o == Equal) {
            Some(Equal)
        } else if props.iter().all(|&o| o != Greater) {
            Some(Less)
        } else if props.iter().all(|&o| o != Less) {
            Some(Greater)
        } else {
            None
        }
    }
}

impl PartialEq for CostProperties {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
            && self.materialized == other.materialized
            && self.cost == other.cost
    }
}

impl CostProperties {
    fn new(
        tables: HashSet<String>,
        cols: HashSet<String>,
        rows: usize,
        index: Option<String>,
        materialized: bool,
        cost: usize,
    ) -> Self {
        Self {
            tables,
            cols,
            rows,
            index,
            materialized,
            cost,
        }
    }

    fn is_top(&self) -> bool {
        *self == Self::top()
    }

    fn top() -> Self {
        Self {
            tables: HashSet::new(),
            cols: HashSet::new(),
            rows: usize::MAX,
            index: None,
            materialized: false,
            cost: usize::MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    tables: HashMap<String, usize>,     // Map from table name to row count
    indexes: HashSet<(String, String)>, // Map from index name to row count
}

impl Catalog {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            indexes: HashSet::new(),
        }
    }

    pub fn with_table(&self, name: String, row_count: usize) -> Self {
        Catalog {
            tables: {
                let mut tables = self.tables.clone();
                tables.insert(name, row_count);
                tables
            },
            indexes: self.indexes.clone(),
        }
    }

    pub fn with_index(&self, table_name: String, index_name: String) -> Self {
        Catalog {
            tables: self.tables.clone(),
            indexes: {
                let mut indexes = self.indexes.clone();
                indexes.insert((table_name, index_name));
                indexes
            },
        }
    }

    fn get_rows(&self, table_name: &str) -> Option<usize> {
        self.tables.get(table_name).cloned()
    }

    fn index_exists(&self, table_name: &str, index_name: &str) -> bool {
        self.indexes
            .contains(&(table_name.to_string(), index_name.to_string()))
    }

    fn get_index_rows_if_exists(&self, table_name: &str, index_name: &str) -> Option<usize> {
        let exists = self
            .indexes
            .contains(&(table_name.to_string(), index_name.to_string()));
        if exists {
            self.get_rows(table_name)
        } else {
            None
        }
    }
}

/// Cost function for QueryLang language
///
/// `op_cost` computes the cost of an operator from the costs of its arguments, given in argument order.
/// Since each argument position gets its own cost, the same e-class can take different costs in different positions.
///
/// The cost model is a simplified model of query execution cost.
/// We compute the costs of each operator as follows:
/// - Name: No cost, load rows and columns from the catalog
/// - Column: No cost, verify that the child was a column name, otherwise go to top
/// - Table: No cost, verify that the child was a table name, otherwise go to top
/// - Index: No cost, verify that the left child was a table and the right child was a column name, otherwise go to top.
///     Check if the index is materialized in the catalog.
/// - Scan, Select, and Join: Logical operators with unextractable costs, go to top
/// - SeqScan: Cost is proportional to the number of rows in the input table, maintain all other properties
/// - IndexScan: Cost is proportional to the number of rows in the input table, assuming the index is materialized.
///     If not, add the cost of materializing the index to the cost of the index scan.
/// - NestedLoopJoin: Added cost is proportional to the product of the number of rows in the left and right input tables.
///     Rows of output are a worst case upper bound: left rows * right rows.
///     Maintain index of the left child, if any.
/// - MergeSortJoin: Added cost is proportional to the sum of the number of rows in the left and right input tables.
///     Rows of output are a worst case upper bound: left rows * right rows.
///     Maintain index of the left child, if any.
///     If the left and right child are not both sorted by an index better than the columns in the join predicate, cost is top.
/// - ExhaustiveSelect: Added cost is proportional to the number of rows in the input table.
///     Rows of output are a worst case upper bound: input rows * SELECTIVITY.
/// - SortSelect: Added cost is proportional to the number of rows in the input table.
///     If the input is sorted by an index that matches the selection predicate,
///     we can scan only a fraction of the rows, otherwise we have to scan all rows.
///     Rows of output are a worst case upper bound: input rows * SELECTIVITY.
///
/// For all cost computations, we use a saturating operation to avoid overflow.
impl Catalog {
    pub fn op_cost(&self, enode: &QueryLang, args: &[CostProperties]) -> CostProperties {
        // Top is absorbing: if any argument is top, so is the result
        if args.iter().any(|a| a.is_top()) {
            return CostProperties::top();
        }

        match (enode, args) {
            // Names load row info from the catalog
            (QueryLang::Name(name), []) => {
                // Get catalog information for the table into the properties, but do not compute a cost yet
                // If we don't find a table with that name in the catalog, assume its a column for now
                if let Some(row_count) = self.get_rows(name) {
                    CostProperties {
                        tables: HashSet::from([name.clone()]),
                        cols: HashSet::new(),
                        rows: row_count,
                        index: None,
                        materialized: false,
                        cost: 0, // No cost for just referencing a table
                    }
                } else {
                    CostProperties {
                        tables: HashSet::new(),
                        cols: HashSet::from([name.clone()]),
                        rows: 0,
                        index: None,
                        materialized: false,
                        cost: 0, // No cost for just referencing a table
                    }
                }
            }
            // If we got a column name from the name child, use it, otherwise go to top
            (QueryLang::Column(_), [c]) => {
                if c.cols.len() > 0 {
                    c.clone()
                } else {
                    CostProperties::top()
                }
            }
            // Tables keep the info loaded from the table name, but do not compute a cost yet
            (QueryLang::Table(_), [t]) => {
                let table_cost = t.clone();
                // Ensure we got a table name, otherwise go to top
                if table_cost.tables.len() != 1 {
                    return CostProperties::top();
                }
                CostProperties {
                    tables: table_cost.tables.clone(),
                    cols: HashSet::new(),
                    rows: table_cost.rows,
                    index: None,
                    materialized: true,
                    cost: 0, // No cost for just referencing a table
                }
            }
            // Indexes keep the info loaded from the table name, but do not compute a cost yet and check if they are, in fact, materialized
            (QueryLang::Index(_), [t, c]) => {
                let table_cost = t.clone();
                let col_cost = c.clone();
                // Ensure we got a table and a column set, otherwise go to top
                if table_cost.tables.len() != 1 || col_cost.cols.len() != 1 {
                    CostProperties::top()
                } else {
                    let materialized = self.index_exists(
                        &table_cost.tables.iter().next().unwrap(),
                        &col_cost.cols.iter().next().unwrap(),
                    );
                    CostProperties {
                        tables: table_cost.tables.clone(),
                        cols: col_cost.cols.clone(),
                        rows: table_cost.rows,
                        index: Some(col_cost.cols.iter().next().unwrap().clone()),
                        materialized: materialized,
                        cost: 0, // No cost for just referencing a table
                    }
                }
            }
            // Sequential Scan is a physical operator that has a cost based on the number of rows in the input
            // We accumulate the cost of reading all the rows in our input, and maintain all other properties
            (QueryLang::SeqScan(_), [t]) => {
                let mut scan_cost = t.clone();
                scan_cost.cost = scan_cost
                    .cost
                    .saturating_add(scan_cost.rows.saturating_mul(READ_COST)); // Cost is proportional to the number of rows in the table
                scan_cost
            }
            // IndexScan is a physical operator that has a cost based on the number of rows in the input, assuming the index is materialized
            (QueryLang::IndexScan(_), [i]) => {
                let child_cost = i.clone();
                let mut index_cost = child_cost.clone();
                // Can't index scan if we don't have an index
                if index_cost.index.is_none() {
                    CostProperties::top()
                } else {
                    if index_cost.materialized {
                        index_cost.cost = index_cost
                            .cost
                            .saturating_add(index_cost.rows.saturating_mul(READ_COST)); // Cost is proportional to the number of rows in the table
                    } else {
                        // Manually create the index and add the cost of materializing it to the cost of the index scan
                        index_cost.materialized = true;
                        index_cost.cost = index_cost.cost.saturating_add(
                            index_cost
                                .rows
                                .saturating_mul(READ_COST)
                                .saturating_mul(index_cost.rows),
                        );
                    }
                    index_cost
                }
            }
            (QueryLang::NestedLoopJoin(_), [left, right, cols]) => {
                let left_cost = left.clone();
                let right_cost = right.clone();
                let col_cost = cols.clone();
                // Take union of tables and columns from each side of the join
                let tables: HashSet<String> = left_cost
                    .tables
                    .union(&right_cost.tables)
                    .cloned()
                    .collect();
                let mut cols: HashSet<String> =
                    left_cost.cols.union(&right_cost.cols).cloned().collect();
                for col in col_cost.cols {
                    cols.insert(col);
                }
                let mut join_cost = CostProperties {
                    tables,
                    cols,
                    rows: left_cost.rows.saturating_mul(right_cost.rows), // Simplified worst case model of join cardinality
                    // Maintain the index of the left child, if any, as the join result will be sorted on that index
                    index: left_cost.index.clone(),
                    materialized: false,
                    cost: left_cost.cost.saturating_add(right_cost.cost),
                };
                // Cost = cost of left + cost of right + cost of reading all rows in the result
                join_cost.cost = join_cost
                    .cost
                    .saturating_add(join_cost.rows.saturating_mul(READ_COST)); // Cost is proportional to the number of rows in the result
                join_cost
            }
            (QueryLang::MergeSortJoin(_), [left, right, cols]) => {
                let left_cost = left.clone();
                let right_cost = right.clone();
                let col_cost = cols.clone();
                // If the left and right child are not both sorted by an index better than the columns in the join predicate, cost is top
                let col_prefix = col_cost
                    .cols
                    .iter()
                    .next()
                    .unwrap_or(&"".to_string())
                    .clone();
                if left_cost
                    .index
                    .as_ref()
                    .map_or(true, |idx| !idx.starts_with(&col_prefix))
                    || right_cost
                        .index
                        .as_ref()
                        .map_or(true, |idx| !idx.starts_with(&col_prefix))
                {
                    return CostProperties::top();
                }

                // Take union of tables and columns from each side of the join
                let tables: HashSet<String> = left_cost
                    .tables
                    .union(&right_cost.tables)
                    .cloned()
                    .collect();
                let mut cols: HashSet<String> =
                    left_cost.cols.union(&right_cost.cols).cloned().collect();
                for col in col_cost.cols {
                    cols.insert(col);
                }
                let mut join_cost = CostProperties {
                    tables,
                    cols,
                    rows: left_cost.rows.saturating_mul(right_cost.rows), // Simplified worst case model of join cardinality
                    // Maintain the index of the left child, if any, as the join result will be sorted on that index
                    index: left_cost.index.clone(),
                    materialized: false,
                    cost: left_cost.cost.saturating_add(right_cost.cost),
                };
                // Cost = cost of left + cost of right + cost of reading all rows in left and right child once each
                join_cost.cost = join_cost.cost.saturating_add(
                    left_cost
                        .rows
                        .saturating_add(right_cost.rows)
                        .saturating_mul(READ_COST),
                );
                join_cost
            }
            (QueryLang::ExhaustiveSelect(_), [t, cols]) => {
                let table_cost = t.clone();
                let col_cost = cols.clone();
                // Take union of tables and columns from each side of the select
                let mut cols: HashSet<String> = table_cost.cols.clone();
                for col in col_cost.cols {
                    cols.insert(col);
                }
                let mut select_cost = CostProperties {
                    tables: table_cost.tables.clone(),
                    cols,
                    rows: (table_cost.rows as f64 * SELECTIVITY) as usize, // Simplified model of selection cardinality
                    index: table_cost.index.clone(),
                    materialized: false,
                    cost: table_cost.cost,
                };
                // Cost = cost of reading all rows in the input table once
                select_cost.cost = select_cost
                    .cost
                    .saturating_add(table_cost.rows.saturating_mul(READ_COST));
                select_cost
            }
            (QueryLang::SortSelect(_), [t, cols]) => {
                let table_cost = t.clone();
                let col_cost = cols.clone();
                let col_prefix = col_cost
                    .cols
                    .iter()
                    .next()
                    .unwrap_or(&"".to_string())
                    .clone();
                // Take union of tables and columns from each side of the select
                let mut cols: HashSet<String> = table_cost.cols.clone();
                for col in col_cost.cols {
                    cols.insert(col);
                }
                let mut select_cost = CostProperties {
                    tables: table_cost.tables.clone(),
                    cols,
                    rows: (table_cost.rows as f64 * SELECTIVITY) as usize, // Simplified model of selection cardinality with sort factor
                    index: table_cost.index.clone(),
                    materialized: false,
                    cost: table_cost.cost,
                };
                // Cost = cost of reading all rows in the input table once
                if select_cost
                    .index
                    .as_ref()
                    .map_or(false, |idx| idx.starts_with(&col_prefix))
                {
                    // If the input is sorted by an index that matches the selection predicate, we can scan only a fraction of the rows
                    select_cost.cost = select_cost.cost.saturating_add(
                        ((table_cost.rows as f64 * SORT_SELECTION_FACTOR) as usize)
                            .saturating_mul(READ_COST),
                    );
                } else {
                    // If the input is not sorted by an index that matches the selection predicate, we have to scan all rows
                    select_cost.cost = select_cost
                        .cost
                        .saturating_add(table_cost.rows.saturating_mul(READ_COST));
                }

                select_cost
            }

            // Scan, Select, and Join are logical operators with unextractable costs
            _ => CostProperties::top(),
        }
    }
}

/// Adapter for egg's extraction infrastructure: looks up each child's cost with the cost function
/// that egg provides, in argument order, and delegates to [`Catalog::op_cost`].
impl CostFunction<QueryLang> for Catalog {
    type Cost = CostProperties;

    fn cost<C>(&mut self, enode: &QueryLang, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        let args: Vec<CostProperties> = enode.children().iter().map(|&c| costs(c)).collect();
        self.op_cost(enode, &args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    /// A cost with only the ordered properties set
    fn c(materialized: bool, index: Option<&str>, cost: usize) -> CostProperties {
        CostProperties::new(
            HashSet::new(),
            HashSet::new(),
            0,
            index.map(String::from),
            materialized,
            cost,
        )
    }

    fn is_top(c: &CostProperties) -> bool {
        *c == CostProperties::top() && c.rows == usize::MAX
    }

    /// Every combination of materialized x index x cost over a small domain
    fn domain() -> Vec<CostProperties> {
        let mut out = vec![];
        for m in [false, true] {
            for i in [None, Some("a"), Some("b")] {
                for k in [0, 1, 2] {
                    out.push(c(m, i, k));
                }
            }
        }
        out
    }

    // ---------- Partial ordering ----------

    #[test]
    fn order_single_property() {
        let base = c(false, None, 5);
        assert!(c(true, None, 5) < base, "materialized is preferred");
        assert!(c(false, Some("a"), 5) < base, "sorted is preferred");
        assert!(c(false, None, 1) < base, "cheaper is preferred");
        assert_eq!(base.partial_cmp(&c(false, None, 5)), Some(Equal));
    }

    #[test]
    fn order_multiple_properties() {
        let base = c(false, None, 5);
        assert!(c(true, Some("a"), 1) < base, "better on every property");
        assert!(c(true, None, 1) < base, "better on some, equal on the rest");
        assert!(base > c(true, None, 1));
        assert_eq!(
            c(true, None, 9).partial_cmp(&base),
            None,
            "trade-off is incomparable"
        );
        assert_eq!(c(false, Some("a"), 9).partial_cmp(&base), None);
        assert_eq!(
            c(true, Some("a"), 0).partial_cmp(&c(false, Some("b"), 9)),
            None,
            "different indexes are incomparable"
        );
    }

    #[test]
    fn order_ignores_unordered_properties() {
        let mut a = c(true, None, 3);
        a.tables.insert("T".into());
        a.rows = 42;
        assert_eq!(a.partial_cmp(&c(true, None, 3)), Some(Equal));
        assert_eq!(a, c(true, None, 3));
    }

    #[test]
    fn order_laws() {
        let d = domain();
        for a in &d {
            assert_eq!(a.partial_cmp(a), Some(Equal), "reflexive: {a:?}");
            for b in &d {
                let ab = a.partial_cmp(b);
                assert_eq!(
                    ab,
                    b.partial_cmp(a).map(|o| o.reverse()),
                    "antisymmetric: {a:?} {b:?}"
                );
                assert_eq!(ab == Some(Equal), a == b, "consistent with eq: {a:?} {b:?}");
                for x in &d {
                    if a < b && b < x {
                        assert!(a < x, "transitive: {a:?} < {b:?} < {x:?}");
                    }
                }
            }
        }
    }

    // ---------- Cost function ----------

    fn id() -> Id {
        Id::from(0usize)
    }

    fn catalog() -> Catalog {
        Catalog::new()
            .with_table("A".into(), 100)
            .with_table("B".into(), 10)
            .with_index("A".into(), "x".into())
    }

    fn name(cat: &Catalog, n: &str) -> CostProperties {
        cat.op_cost(&QueryLang::Name(n.into()), &[])
    }

    fn table(cat: &Catalog, n: &str) -> CostProperties {
        cat.op_cost(&QueryLang::Table([id()]), &[name(cat, n)])
    }

    fn index(cat: &Catalog, t: &str, col: &str) -> CostProperties {
        cat.op_cost(
            &QueryLang::Index([id(); 2]),
            &[name(cat, t), name(cat, col)],
        )
    }

    /// Every physical operator paired with a valid (non-top) set of arguments
    fn valid_cases(cat: &Catalog) -> Vec<(QueryLang, Vec<CostProperties>)> {
        let (tbl, col, idx) = (table(cat, "A"), name(cat, "x"), index(cat, "A", "x"));
        vec![
            (QueryLang::Column([id()]), vec![col.clone()]),
            (QueryLang::Table([id()]), vec![name(cat, "A")]),
            (
                QueryLang::Index([id(); 2]),
                vec![name(cat, "A"), col.clone()],
            ),
            (QueryLang::SeqScan([id()]), vec![tbl.clone()]),
            (QueryLang::IndexScan([id()]), vec![idx.clone()]),
            (
                QueryLang::NestedLoopJoin([id(); 3]),
                vec![tbl.clone(), tbl.clone(), col.clone()],
            ),
            (
                QueryLang::MergeSortJoin([id(); 3]),
                vec![idx.clone(), idx.clone(), col.clone()],
            ),
            (
                QueryLang::ExhaustiveSelect([id(); 2]),
                vec![tbl.clone(), col.clone()],
            ),
            (
                QueryLang::SortSelect([id(); 2]),
                vec![idx.clone(), col.clone()],
            ),
        ]
    }

    #[test]
    fn cost_values() {
        let cat = catalog();
        let (a, b, x) = (table(&cat, "A"), table(&cat, "B"), name(&cat, "x"));
        let op = |n: QueryLang, args: &[CostProperties]| cat.op_cost(&n, args);

        let seq = op(QueryLang::SeqScan([id()]), &[a.clone()]);
        assert_eq!((seq.cost, seq.rows), (100, 100));

        // Materialized index: read every row; unmaterialized: pay rows^2 to build it
        let iscan = op(QueryLang::IndexScan([id()]), &[index(&cat, "A", "x")]);
        assert_eq!((iscan.cost, iscan.materialized), (100, true));
        let iscan = op(QueryLang::IndexScan([id()]), &[index(&cat, "B", "x")]); // 10 rows -> 10^2 to build
        assert_eq!((iscan.cost, iscan.materialized), (100, true));
        assert!(
            is_top(&op(QueryLang::IndexScan([id()]), &[a.clone()])),
            "no index"
        );

        let nl = op(
            QueryLang::NestedLoopJoin([id(); 3]),
            &[a.clone(), b.clone(), x.clone()],
        );
        assert_eq!((nl.rows, nl.cost), (1000, 1000));
        assert_eq!(nl.tables, HashSet::from(["A".into(), "B".into()]));

        let sorted = index(&cat, "A", "x");
        let ms = op(
            QueryLang::MergeSortJoin([id(); 3]),
            &[sorted.clone(), sorted.clone(), x.clone()],
        );
        assert_eq!(
            (ms.rows, ms.cost, ms.index.as_deref()),
            (10000, 200, Some("x"))
        );
        assert!(
            is_top(&op(
                QueryLang::MergeSortJoin([id(); 3]),
                &[a.clone(), sorted.clone(), x.clone()]
            )),
            "unsorted input"
        );

        let xs = op(
            QueryLang::ExhaustiveSelect([id(); 2]),
            &[a.clone(), x.clone()],
        );
        assert_eq!((xs.rows, xs.cost), (10, 100));

        // Sorting on the predicate column lets SortSelect scan only a fraction of the rows
        let ss_sorted = op(
            QueryLang::SortSelect([id(); 2]),
            &[sorted.clone(), x.clone()],
        );
        let ss_unsorted = op(QueryLang::SortSelect([id(); 2]), &[a.clone(), x.clone()]);
        assert_eq!(ss_sorted.cost, 50, "sorted input scans half the rows");
        assert_eq!(ss_unsorted.cost, 100, "unsorted input scans every row");
    }

    #[test]
    fn cost_rejects_ill_typed_arguments() {
        let cat = catalog();
        let (col, tname) = (name(&cat, "x"), name(&cat, "A"));
        assert!(
            is_top(&cat.op_cost(&QueryLang::Column([id()]), &[tname.clone()])),
            "Column of a table name"
        );
        assert!(
            is_top(&cat.op_cost(&QueryLang::Table([id()]), &[col.clone()])),
            "Table of a column name"
        );
        assert!(
            is_top(&cat.op_cost(&QueryLang::Index([id(); 2]), &[col.clone(), tname.clone()])),
            "Index with swapped arguments"
        );
    }

    #[test]
    fn logical_operators_are_top() {
        let cat = catalog();
        let t = table(&cat, "A");
        let x = name(&cat, "x");
        assert!(is_top(&cat.op_cost(&QueryLang::Scan(id()), &[t.clone()])));
        assert!(is_top(&cat.op_cost(
            &QueryLang::Select([id(); 2]),
            &[t.clone(), x.clone()]
        )));
        assert!(is_top(&cat.op_cost(
            &QueryLang::Join([id(); 3]),
            &[t.clone(), t.clone(), x.clone()]
        )));
    }

    /// Runs `op_cost` on each case, collecting panics and failed checks instead of stopping at the first
    fn check_cases(
        cases: Vec<(String, QueryLang, Vec<CostProperties>)>,
        check: impl Fn(&[CostProperties], &CostProperties) -> bool,
    ) {
        let cat = catalog();
        let failures: Vec<String> = cases
            .into_iter()
            .filter_map(|(label, node, args)| {
                match catch_unwind(AssertUnwindSafe(|| cat.op_cost(&node, &args))) {
                    Err(_) => Some(format!("{label}: panicked")),
                    Ok(r) if !check(&args, &r) => Some(format!("{label}: got {r:?}")),
                    Ok(_) => None,
                }
            })
            .collect();
        assert!(
            failures.is_empty(),
            "{} failing cases:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn valid_arguments_are_not_top() {
        let cases = valid_cases(&catalog())
            .into_iter()
            .map(|(n, args)| (format!("{n}"), n, args))
            .collect();
        check_cases(cases, |_, r| !is_top(r));
    }

    #[test]
    fn top_argument_propagates_top() {
        // Replace each argument of each operator, in turn, with top
        let mut cases = vec![];
        for (node, args) in valid_cases(&catalog()) {
            for i in 0..args.len() {
                let mut args = args.clone();
                args[i] = CostProperties::top();
                cases.push((format!("{node} with arg {i} = top"), node.clone(), args));
            }
        }
        check_cases(cases, |_, r| is_top(r));
    }

    #[test]
    fn huge_arguments_do_not_overflow() {
        // Non-top arguments close to the maximum must saturate, not panic
        let mut cases = vec![];
        for (node, args) in valid_cases(&catalog()) {
            let huge: Vec<CostProperties> = args
                .iter()
                .map(|a| CostProperties {
                    rows: usize::MAX - 1,
                    cost: usize::MAX - 1,
                    ..a.clone()
                })
                .collect();
            cases.push((format!("{node} with huge args"), node, huge));
        }
        check_cases(cases, |_, _| true);
    }

    #[test]
    fn egg_cost_function_matches_op_cost() {
        let mut cat = catalog();
        let node =
            QueryLang::NestedLoopJoin([Id::from(0usize), Id::from(1usize), Id::from(2usize)]);
        let args = [table(&cat, "A"), table(&cat, "B"), name(&cat, "x")];
        let expected = cat.op_cost(&node, &args);
        let got = cat.cost(&node, |i| args[usize::from(i)].clone());
        assert_eq!(got, expected);
        assert_eq!(got.rows, expected.rows);
    }
}
