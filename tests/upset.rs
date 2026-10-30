use compositional::lattice::{JoinLattice, MeetLattice, WithTop};
use compositional::upset::BackwardUpSet;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
struct N(i32);

/// (int, int) under the product (componentwise) partial order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct P(i32, i32);

impl PartialOrd for P {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        match (self.0.cmp(&o.0), self.1.cmp(&o.1)) {
            (a, b) if a == b => Some(a),
            (Ordering::Equal, x) | (x, Ordering::Equal) => Some(x),
            _ => None,
        }
    }
}

impl WithTop for N {
    fn top() -> Self {
        N(i32::MAX)
    }
}

impl WithTop for P {
    fn top() -> Self {
        P(i32::MAX, i32::MAX)
    }
}

impl JoinLattice for N {
    fn join(&self, o: &Self) -> Self {
        N(self.0.max(o.0))
    }
}

impl JoinLattice for P {
    fn join(&self, o: &Self) -> Self {
        P(self.0.max(o.0), self.1.max(o.1))
    }
}

fn up<T>(v: Vec<T>) -> BackwardUpSet<T>
where
    T: std::hash::Hash + PartialOrd + Eq + Clone + WithTop,
{
    v.into()
}

fn set<T: std::hash::Hash + Eq>(v: Vec<T>) -> HashSet<T> {
    v.into_iter().collect()
}

#[test]
fn total_order_keeps_single_smallest() {
    assert_eq!(up(vec![5, 3, 8, 3]).elements, set(vec![3]));
    assert_eq!(up(vec![7]).meet(&up(vec![2])).elements, set(vec![2]));
    assert_eq!(up(vec![2]).meet(&up(vec![7])).elements, set(vec![2]));
}

#[test]
fn partial_order_keeps_minimal_frontier() {
    let u = up(vec![P(1, 4), P(4, 1), P(2, 2), P(3, 3), P(5, 5), P(1, 5)]);
    assert_eq!(u.elements, set(vec![P(1, 4), P(4, 1), P(2, 2)]));

    // Meeting two frontiers keeps all incomparable minima and drops dominated ones.
    let m = up(vec![P(0, 5), P(5, 0)]).meet(&up(vec![P(2, 2), P(6, 0)]));
    assert_eq!(m.elements, set(vec![P(0, 5), P(5, 0), P(2, 2)]));
}

#[test]
fn meet_with_dominating_element_collapses_frontier() {
    let frontier = up(vec![P(1, 4), P(4, 1), P(2, 2)]);
    let best = up(vec![P(0, 0)]);
    assert_eq!(frontier.meet(&best).elements, set(vec![P(0, 0)]));
    assert_eq!(best.meet(&frontier).elements, set(vec![P(0, 0)]));
}

#[test]
fn meet_with_dominated_element_is_noop() {
    // Every frontier element is <= (5,5), so the frontier is unchanged.
    let frontier = up(vec![P(1, 4), P(4, 1)]);
    assert_eq!(frontier.meet(&up(vec![P(5, 5)])), frontier);
    // (0,9) is incomparable to both frontier elements, so it is added.
    assert_eq!(
        frontier.meet(&up(vec![P(0, 9)])).elements,
        set(vec![P(1, 4), P(4, 1), P(0, 9)])
    );
}

#[test]
fn top_is_empty_construction_and_meet_identity() {
    let top = BackwardUpSet::<P>::top();
    assert_eq!(BackwardUpSet::new(HashSet::new()), top);
    let u = up(vec![P(1, 4), P(4, 1)]);
    assert_eq!(u.meet(&top), u);
    assert_eq!(top.meet(&u), u);
    assert_eq!(top.meet(&top), top);
}

#[test]
fn meet_lattice_laws() {
    let a = up(vec![P(1, 4), P(4, 1)]);
    let b = up(vec![P(2, 2)]);
    let c = up(vec![P(0, 6), P(6, 0)]);
    assert_eq!(a.meet(&a), a, "idempotent");
    assert_eq!(a.meet(&b), b.meet(&a), "commutative");
    assert_eq!(a.meet(&b).meet(&c), a.meet(&b.meet(&c)), "associative");

    // Meet is a lower bound of both inputs.
    let m = a.meet(&b);
    assert!(m <= a && m <= b);

    let mut x = a.clone();
    x.meet_into(&b);
    assert_eq!(x, m);
}

#[test]
fn total_order_join_keeps_single_largest() {
    assert_eq!(up(vec![N(7)]).join(&up(vec![N(2)])).elements, set(vec![N(7)]));
    assert_eq!(up(vec![N(2)]).join(&up(vec![N(7)])).elements, set(vec![N(7)]));
}

#[test]
fn partial_order_join_keeps_minimal_frontier() {
    // Joining two frontiers keeps all incomparable minima and drops dominated ones.
    let j = up(vec![P(0, 5), P(5, 0)]).join(&up(vec![P(2, 2), P(6, 0)]));
    assert_eq!(j.elements, set(vec![P(2, 5), P(5, 2), P(6, 0)]));
}

#[test]
fn join_with_dominated_element_collapses_frontier() {
    let frontier = up(vec![P(1, 4), P(4, 1), P(2, 2)]);
    let worst = up(vec![P(5, 5)]);
    assert_eq!(frontier.join(&worst).elements, set(vec![P(5, 5)]));
    assert_eq!(worst.join(&frontier).elements, set(vec![P(5, 5)]));
}

#[test]
fn join_with_dominating_element_is_noop() {
    // (0,0) is <= every frontier element, so the frontier is unchanged.
    let frontier = up(vec![P(1, 4), P(4, 1)]);
    assert_eq!(frontier.join(&up(vec![P(0, 0)])), frontier);
    // (0,9) is incomparable to both frontier elements, so it is joined with each: (1,9) and (4,9),
    // of which only (1,9) is minimal.
    assert_eq!(
        frontier.join(&up(vec![P(0, 9)])).elements,
        set(vec![P(1, 9)])
    );
}

#[test]
fn top_is_join_absorbing() {
    let top = BackwardUpSet::<P>::top();
    let u = up(vec![P(1, 4), P(4, 1)]);
    assert_eq!(u.join(&top), top);
    assert_eq!(top.join(&u), top);
    assert_eq!(top.join(&top), top);
}

#[test]
fn join_lattice_laws() {
    let a = up(vec![P(1, 4), P(4, 1)]);
    let b = up(vec![P(2, 2)]);
    let c = up(vec![P(0, 6), P(6, 0)]);
    assert_eq!(a.join(&a), a, "idempotent");
    assert_eq!(a.join(&b), b.join(&a), "commutative");
    assert_eq!(a.join(&b).join(&c), a.join(&b.join(&c)), "associative");

    // Join is an upper bound of both inputs.
    let j = a.join(&b);
    assert!(j >= a && j >= b);

    let mut x = a.clone();
    x.join_into(&b);
    assert_eq!(x, j);
}

#[test]
fn partial_cmp_matches_lattice_order() {
    let a = up(vec![P(0, 5), P(5, 0)]);
    // Equal sets compare Equal.
    assert_eq!(a.partial_cmp(&a.clone()), Some(Ordering::Equal));
    assert_eq!(up(vec![3]).partial_cmp(&up(vec![3])), Some(Ordering::Equal));
    // a's upset contains (0,6), so a is lower (better), even though (5,0) !<= (0,6).
    assert_eq!(a.partial_cmp(&up(vec![P(0, 6)])), Some(Ordering::Less));
    assert_eq!(up(vec![P(0, 6)]).partial_cmp(&a), Some(Ordering::Greater));
    // Neither upset contains the other.
    assert_eq!(a.partial_cmp(&up(vec![P(1, 1)])), None);
    // Everything is below top.
    assert!(a < BackwardUpSet::top());
}

#[test]
fn contains_is_upward_closed() {
    let u = up(vec![P(1, 4), P(4, 1)]);
    assert!(u.contains(&P(1, 4)));
    assert!(u.contains(&P(9, 9)));
    assert!(u.contains(&P(1, 9)));
    assert!(!u.contains(&P(2, 2)));
    assert!(!u.contains(&P(0, 9)));
}
