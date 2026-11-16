#[derive(Clone, Debug)]
pub struct UnionFind {
    pub parents: Vec<usize>,
}

impl UnionFind {
    pub fn new() -> Self {
        Self {
            parents: Vec::new(),
        }
    }

    pub fn make_set(&mut self) -> usize {
        let id = self.parents.len();
        self.parents.push(id);
        id
    }

    pub fn size(&self) -> usize {
        self.parents.len()
    }

    fn parent(&self, query: usize) -> usize {
        self.parents[query]
    }

    fn parent_mut(&mut self, query: usize) -> &mut usize {
        &mut self.parents[query]
    }

    pub fn find(&self, mut current: usize) -> usize {
        while current != self.parent(current) {
            current = self.parent(current)
        }
        current
    }

    pub fn find_mut(&mut self, mut current: usize) -> usize {
        while current != self.parent(current) {
            let grandparent = self.parent(self.parent(current));
            *self.parent_mut(current) = grandparent;
            current = grandparent;
        }
        current
    }

    /// Given two leader ids, unions the two eclasses making root1 the leader.
    pub fn union(&mut self, root1: usize, root2: usize) -> usize {
        *self.parent_mut(root2) = root1;
        root1
    }
}
