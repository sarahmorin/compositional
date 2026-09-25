/* A simple DB Query language to test with */

use egg::{CostFunction, Id, define_language};
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
impl PartialOrd for CostProperties {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Order by 3rd property whenever 2 are equal, otherwise incomparable.
        match (
            self.materialized == other.materialized,
            self.index == other.index,
            self.cost == other.cost,
        ) {
            (true, true, true) => return Some(std::cmp::Ordering::Equal),
            (true, true, _) => return self.cost.partial_cmp(&other.cost),
            (true, _, true) => return self.index.partial_cmp(&other.index),
            (_, true, true) => return self.materialized.partial_cmp(&other.materialized),
            _ => return None,
        }
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

// TODO: Test this partial ordering and make sure it is correct.

struct Catalog {
    tables: HashMap<String, usize>,     // Map from table name to row count
    indexes: HashSet<(String, String)>, // Map from index name to row count
}

impl Catalog {
    fn new() -> Self {
        Self {
            tables: HashMap::new(),
            indexes: HashSet::new(),
        }
    }

    fn with_table(&self, name: String, row_count: usize) -> Self {
        Catalog {
            tables: {
                let mut tables = self.tables.clone();
                tables.insert(name, row_count);
                tables
            },
            indexes: self.indexes.clone(),
        }
    }

    fn with_index(&self, table_name: String, index_name: String) -> Self {
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
impl CostFunction<QueryLang> for Catalog {
    type Cost = CostProperties;

    fn cost<C>(&mut self, enode: &QueryLang, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        match enode {
            // Names load row info from the catalog
            QueryLang::Name(name) => {
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
            QueryLang::Column([c]) => {
                if costs(*c).cols.len() > 0 {
                    costs(*c)
                } else {
                    CostProperties::top()
                }
            }
            // Tables keep the info loaded from the table name, but do not compute a cost yet
            QueryLang::Table([t]) => {
                let table_cost = costs(*t);
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
            QueryLang::Index([t, c]) => {
                let table_cost = costs(*t);
                let col_cost = costs(*c);
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
            QueryLang::SeqScan([t]) => {
                let mut scan_cost = costs(*t).clone();
                scan_cost.cost += scan_cost.rows.saturating_mul(READ_COST); // Cost is proportional to the number of rows in the table
                scan_cost
            }
            // IndexScan is a physical operator that has a cost based on the number of rows in the input, assuming the index is materialized
            QueryLang::IndexScan([i]) => {
                let child_cost = costs(*i);
                let mut index_cost = child_cost.clone();
                // Can't index scan if we don't have an index
                if index_cost.index.is_none() {
                    CostProperties::top()
                } else {
                    if index_cost.materialized {
                        index_cost.cost += index_cost.rows.saturating_mul(READ_COST); // Cost is proportional to the number of rows in the table
                    } else {
                        // Manually create the index and add the cost of materializing it to the cost of the index scan
                        index_cost.materialized = true;
                        index_cost.cost += index_cost
                            .rows
                            .saturating_mul(READ_COST)
                            .saturating_mul(index_cost.rows);
                    }
                    index_cost
                }
            }
            QueryLang::NestedLoopJoin([left, right, cols]) => {
                let left_cost = costs(*left);
                let right_cost = costs(*right);
                let col_cost = costs(*cols);
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
                join_cost.cost += join_cost.rows.saturating_mul(READ_COST); // Cost is proportional to the number of rows in the result
                join_cost
            }
            QueryLang::MergeSortJoin([left, right, cols]) => {
                let left_cost = costs(*left);
                let right_cost = costs(*right);
                let col_cost = costs(*cols);
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
                join_cost.cost += left_cost
                    .rows
                    .saturating_add(right_cost.rows)
                    .saturating_mul(READ_COST);
                join_cost
            }
            QueryLang::ExhaustiveSelect([t, cols]) => {
                let table_cost = costs(*t);
                let col_cost = costs(*cols);
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
                select_cost.cost += table_cost.rows.saturating_mul(READ_COST);
                select_cost
            }
            QueryLang::SortSelect([t, cols]) => {
                let table_cost = costs(*t);
                let col_cost = costs(*cols);
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
                    .map_or(true, |idx| !idx.starts_with(&col_prefix))
                {
                    // If the input is sorted by an index that matches the selection predicate, we can scan only a fraction of the rows
                    select_cost.cost += ((table_cost.rows as f64 * SORT_SELECTION_FACTOR) as usize)
                        .saturating_mul(READ_COST);
                } else {
                    // If the input is not sorted by an index that matches the selection predicate, we have to scan all rows
                    select_cost.cost += table_cost.rows.saturating_mul(READ_COST);
                }

                select_cost
            }

            // Scan, Select, and Join are logical operators with unextractable costs
            _ => CostProperties::top(),
        }
    }
}

// TODO: Test the cost function
