use crate::lattice::{JoinLattice, MeetLattice, WithTop};
use std::collections::HashSet;
use std::hash::Hash;

/// Backwards UpSet Lattice
///
/// An UpSet is a set of elements that is upwards closed under the partial order.
/// That is, if an element is in the set, then all elements greater than it are also in the set.
/// The lattice construction for an UpSet (or DownSet) is the subset lattice of Upsets (or DownSets) of a given set of elements.
/// We represent an UpSet by the set of its minimal elements.
///
/// We call this a backwards UpSet because we are inverting the lattice to the Top element is
/// the "worst" in terms of T's and the Bottom element is the "best" in terms of T's available.
/// Traditionally:
/// - Bot is the empty set
/// - Top is the set of all elements
/// - Meet is the intersection of two sets
/// - Join is the union of two sets
///
/// In the backwards UpSet lattice:
/// - Bot is the set of all elements
/// - Top is the empty set
/// - Meet is the union of two sets
/// - Join is the intersection of two sets
///
/// We do this so that "moving down" in the lattice corresponds to getting better (smaller) elements.
#[derive(Debug, Clone)]
pub struct BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    /// The set of elements representing the UpSet
    /// An UpSet is represented by a set of elements that are the "minimal" elements of the UpSet.
    pub elements: HashSet<T>,
}

impl<T> WithTop for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    /// Returns the top element of the UpSet lattice,
    /// although this is technically the empty set, we consider that set ill-formed
    /// and instead use the set containing only the top element of T
    fn top() -> Self {
        let mut elements = HashSet::new();
        elements.insert(T::top());
        BackwardUpSet { elements }
    }
}

impl<T> BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    /// Create a new UpSet from a set of elements
    /// Reduces the set of elements to the minimal elements of the UpSet
    pub fn new(elements: HashSet<T>) -> Self {
        // If we are given no elements, return the Top set
        if elements.is_empty() {
            return BackwardUpSet::top();
        }

        let mut minimal_elements = HashSet::new();
        for e in elements {
            if !minimal_elements.iter().any(|m| m <= &e) {
                minimal_elements.retain(|m| !(e <= *m));
                minimal_elements.insert(e);
            }
        }
        BackwardUpSet {
            elements: minimal_elements,
        }
    }

    /// Check if the UpSet is empty
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Check if the UpSet contains an element
    pub fn contains(&self, element: &T) -> bool {
        self.elements.iter().any(|e| e <= element)
    }
}

impl<T> From<Vec<T>> for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    fn from(vec: Vec<T>) -> Self {
        let elements: HashSet<T> = vec.into_iter().collect();
        BackwardUpSet::new(elements)
    }
}

impl<T> PartialEq for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    fn eq(&self, other: &Self) -> bool {
        self.elements == other.elements
    }
}

impl<T> Eq for BackwardUpSet<T> where T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop {}

impl<T> PartialOrd for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.elements == other.elements {
            Some(std::cmp::Ordering::Equal)
        } else if other
            .elements
            .iter()
            .all(|o| self.elements.iter().any(|e| e <= o))
        {
            Some(std::cmp::Ordering::Less)
        } else if self
            .elements
            .iter()
            .all(|e| other.elements.iter().any(|o| o <= e))
        {
            Some(std::cmp::Ordering::Greater)
        } else {
            None
        }
    }
}

impl<T> MeetLattice for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop,
{
    /// Meet is the union of two UpSets
    fn meet(&self, other: &Self) -> Self {
        let mut new_elements = self.elements.clone();
        new_elements.extend(other.elements.clone());
        BackwardUpSet::new(new_elements)
    }
}

impl<T> JoinLattice for BackwardUpSet<T>
where
    T: Hash + PartialOrd + PartialEq + Eq + Clone + WithTop + JoinLattice,
{
    /// Join is the intersection of two UpSets
    fn join(&self, other: &Self) -> Self {
        todo!("Implement join for BackwardUpSet");
    }
}
