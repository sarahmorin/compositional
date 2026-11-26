/// Event log of e-graph changes, for replaying an execution.
///
/// Recording is off by default and switched on at runtime with `EGraph::start_recording`. The
/// log lives beside the e-graph, never inside its classes or nodes: the e-graph only appends
/// events, and `viz::replay` interprets them.
///
/// Structural events record the *outcome* of each change (which class won a union, which node a
/// congruence kept), not the call that caused it, because `rebuild` repairs nodes in hash-set
/// order and re-running the calls would not reproduce the same ids.
///
/// Annotation events (`Focus`, `Note`) come from the code driving the e-graph, such as a search
/// strategy, and are interleaved with the structural events in execution order.
use crate::egraph::core::{ClassId, NodeId};

/// Something an annotation can point at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Class(ClassId),
    Node(NodeId),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event<L, D> {
    /// A new node was created in a new class.
    AddNode {
        node: NodeId,
        class: ClassId,
        op: L,
        children: Vec<ClassId>,
    },
    /// `loser` was merged into `leader`; both were canonical at the time.
    Union { leader: ClassId, loser: ClassId },
    /// During a rebuild, `node` became congruent to `kept` and was dropped in its favor.
    DropNode { node: NodeId, kept: NodeId },
    /// A class's analysis data changed.
    SetData { class: ClassId, data: D },
    /// A rebuild with pending work started.
    RebuildStart,
    /// The rebuild started by the last `RebuildStart` finished.
    RebuildEnd,
    /// The driver is now looking at this target (`None` clears the focus).
    Focus(Option<Target>),
    /// Free-form message from the driver, e.g. the rule being applied.
    Note(String),
}

impl<L, D> Event<L, D> {
    /// Whether this event changes the e-graph's structure (as opposed to data or annotations).
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Event::AddNode { .. } | Event::Union { .. } | Event::DropNode { .. }
        )
    }
}

/// A recorded sequence of events.
#[derive(Clone, Debug)]
pub struct Trace<L, D> {
    pub events: Vec<Event<L, D>>,
}

impl<L, D> Trace<L, D> {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl<L, D> Default for Trace<L, D> {
    fn default() -> Self {
        Self::new()
    }
}
