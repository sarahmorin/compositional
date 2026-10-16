/// Lattice traits and some default implementations.

/// WithTop trait for types that have a top (maximal) element.
pub trait WithTop {
    fn top() -> Self;
}

/// WithBot trait for types that have a bottom (minimal) element.
pub trait WithBot {
    fn bot() -> Self;
}

/// JoinLattice trait for types that have a join (least upper bound) operation.
pub trait JoinLattice: PartialOrd + PartialEq + Clone {
    /// Join: Least upper bound of two elements
    /// The result is guaranteed to be greater than or equal to both inputs.
    fn join(&self, other: &Self) -> Self;

    /// Join in place
    fn join_into(&mut self, other: &Self) {
        *self = self.join(other);
    }
}

/// MeetLattice trait for types that have a meet (greatest lower bound) operation.
pub trait MeetLattice: PartialOrd + PartialEq + Clone {
    /// Meet: Greatest lower bound of two elements
    /// The result is guaranteed to be less than or equal to both inputs.
    fn meet(&self, other: &Self) -> Self;

    /// Meet in place
    fn meet_into(&mut self, other: &Self) {
        *self = self.meet(other);
    }
}

/// A Lattice is a type that has both join and meet operations.
pub trait Lattice: JoinLattice + MeetLattice {}
