use rustc_index::vec::{Idx, IndexVec};

use ena::undo_log::{Rollback, UndoLogs};

pub enum Undo<T> {
    Move { index: T, old: usize },
    Extend { group_index: usize, len: usize },
    NewGroup { index: T },
}

pub struct UnifyLog<T: Idx> {
    unified_vars: IndexVec<T, usize>,
    groups: Vec<Vec<T>>,
    reference_counts: IndexVec<T, u32>,
}

fn pick2_mut<T, I: Idx>(self_: &mut [T], a: I, b: I) -> (&mut T, &mut T) {
    let (ai, bi) = (a.index(), b.index());
    assert!(ai != bi);

    if ai < bi {
        let (c1, c2) = self_.split_at_mut(bi);
        (&mut c1[ai], &mut c2[0])
    } else {
        let (c2, c1) = pick2_mut(self_, b, a);
        (c1, c2)
    }
}

impl<T: Idx> UnifyLog<T> {
    pub fn new() -> Self {
        UnifyLog {
            unified_vars: IndexVec::new(),
            groups: Vec::new(),
            reference_counts: IndexVec::new(),
        }
    }

    pub fn unify(&mut self, undo_log: &mut impl UndoLogs<Undo<T>>, root: T, other: T) {
        if !self.needs_log(other) {
            return;
        }
        self.unified_vars.ensure_contains_elem(root, usize::max_value);
        self.unified_vars.ensure_contains_elem(other, usize::max_value);
        let mut root_group = self.unified_vars[root];
        let other_group = self.unified_vars[other];

        if other_group == usize::max_value() {
            let root_vec = if root_group == usize::max_value() {
                root_group = self.groups.len();
                self.unified_vars[root] = root_group;
                self.groups.push(Vec::new());
                undo_log.push(Undo::NewGroup { index: root });
                self.groups.last_mut().unwrap()
            } else {
                let root_vec = &mut self.groups[root_group];
                undo_log.push(Undo::Extend { group_index: root_group, len: root_vec.len() });
                root_vec
            };
            root_vec.push(other);
        } else {
            if root_group == usize::max_value() {
                let group = &mut self.unified_vars[root];
                undo_log.push(Undo::Move { index: root, old: *group });
                *group = other_group;
                self.groups[other_group].push(other);
            } else {
                let (root_vec, other_vec) = pick2_mut(&mut self.groups, root_group, other_group);
                undo_log.push(Undo::Extend { group_index: root_group, len: root_vec.len() });
                root_vec.extend_from_slice(other_vec);

                if self.reference_counts.get(other).map_or(false, |c| *c != 0) {
                    root_vec.push(other);
                }
            }
        }
    }

    pub fn get(&self, root: T) -> &[T] {
        match self.unified_vars.get(root) {
            Some(group) => match self.groups.get(*group) {
                Some(v) => v,
                None => &[],
            },
            None => &[],
        }
    }

    pub fn needs_log(&self, vid: T) -> bool {
        !self.get(vid).is_empty() || self.reference_counts.get(vid).map_or(false, |c| *c != 0)
    }

    pub fn watch_variable(&mut self, index: T) {
        self.reference_counts.ensure_contains_elem(index, || 0);
        self.reference_counts[index] += 1;
    }

    pub fn unwatch_variable(&mut self, index: T) {
        self.reference_counts[index] -= 1;
    }
}

impl<I: Idx> Rollback<Undo<I>> for UnifyLog<I> {
    fn reverse(&mut self, undo: Undo<I>) {
        match undo {
            Undo::Extend { group_index, len } => self.groups[group_index].truncate(len as usize),
            Undo::Move { index, old } => self.unified_vars[index] = old,
            Undo::NewGroup { index } => {
                self.groups.pop();
                self.unified_vars[index] = usize::max_value();
            }
        }
    }
}
