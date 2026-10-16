use crate::lattice::{JoinLattice, MeetLattice, WithBot, WithTop};

/// Min lattice implementation
#[derive(Debug, Clone)]
pub struct MinLattice<T>
where
    T: PartialOrd + PartialEq + Clone,
{
    pub value: T,
}

impl<T> PartialOrd for MinLattice<T>
where
    T: PartialOrd + Clone,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T> PartialEq for MinLattice<T>
where
    T: PartialOrd + PartialEq + Clone,
{
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Ord for MinLattice<T>
where
    T: Ord + Clone,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

impl<T> Eq for MinLattice<T> where T: PartialOrd + Eq + Clone {}

impl<T> WithTop for MinLattice<T>
where
    T: WithTop + PartialOrd + PartialEq + Clone,
{
    fn top() -> Self {
        MinLattice { value: T::top() }
    }
}

/// Default MinLattice implementation for any totally ordered type T with a Top element.
impl<T> JoinLattice for MinLattice<T>
where
    T: Ord + PartialOrd + PartialEq + Clone,
{
    fn join(&self, other: &Self) -> Self {
        MinLattice {
            value: std::cmp::max(self.value.clone(), other.value.clone()),
        }
    }
}

impl<T> MeetLattice for MinLattice<T>
where
    T: Ord + PartialOrd + PartialEq + Clone,
{
    fn meet(&self, other: &Self) -> Self {
        MinLattice {
            value: std::cmp::min(self.value.clone(), other.value.clone()),
        }
    }
}

/// Implementing WithTop for primitive types like usize, f64, etc. can be done by defining a top value for each type.
/// Then we can derive SemiLattice for MinLattice<T> where T: WithTop + Ord + PartialOrd + PartialEq + Clone.

impl WithTop for usize {
    fn top() -> Self {
        usize::MAX
    }
}

impl WithBot for usize {
    fn bot() -> Self {
        usize::MIN
    }
}

impl WithTop for u32 {
    fn top() -> Self {
        u32::MAX
    }
}

impl WithBot for u32 {
    fn bot() -> Self {
        u32::MIN
    }
}

impl WithTop for u64 {
    fn top() -> Self {
        u64::MAX
    }
}

impl WithBot for u64 {
    fn bot() -> Self {
        u64::MIN
    }
}

impl WithTop for i32 {
    fn top() -> Self {
        i32::MAX
    }
}

impl WithBot for i32 {
    fn bot() -> Self {
        i32::MIN
    }
}

impl WithTop for i64 {
    fn top() -> Self {
        i64::MAX
    }
}

impl WithBot for i64 {
    fn bot() -> Self {
        i64::MIN
    }
}

impl WithTop for f32 {
    fn top() -> Self {
        f32::INFINITY
    }
}

impl WithBot for f32 {
    fn bot() -> Self {
        f32::NEG_INFINITY
    }
}

impl WithTop for f64 {
    fn top() -> Self {
        f64::INFINITY
    }
}

impl WithBot for f64 {
    fn bot() -> Self {
        f64::NEG_INFINITY
    }
}
