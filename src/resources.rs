use std::{any::type_name, collections::HashSet, hash::Hash};

use thiserror::Error;

/// Trait for identifying accessed 'resources' that may conflict if used at the same time.
pub trait Resources: Default {
    /// Union this set of resources with the given set of resources.
    fn union(&mut self, other: &Self);
    /// Return true if any resource in this set may not be used at the same time with any resource
    /// in the other set.
    fn conflicts_with(&self, other: &Self) -> bool;
}

#[derive(Debug, Clone, Error)]
#[error("resource conflict in {type_name:?}")]
pub struct ResourceConflict {
    pub type_name: &'static str,
}

impl ResourceConflict {
    pub fn conflict_in<T>() -> Self {
        Self {
            type_name: type_name::<T>(),
        }
    }
}

/// A `Resources` implementation that describes R/W locks.
///
/// Two read locks for the same resource do not conflict, but a read and a write or two writes to
/// the same resource do.
#[derive(Debug, Clone)]
pub struct RwResources<R> {
    reads: HashSet<R>,
    writes: HashSet<R>,
}

impl<R> Default for RwResources<R>
where
    R: Eq + Hash,
{
    fn default() -> Self {
        RwResources {
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }
}

impl<R> RwResources<R>
where
    R: Eq + Hash,
{
    pub fn new() -> Self {
        Default::default()
    }

    pub fn from_iters(
        reads: impl IntoIterator<Item = R>,
        writes: impl IntoIterator<Item = R>,
    ) -> Self {
        let writes: HashSet<R> = writes.into_iter().collect();
        let reads: HashSet<R> = reads.into_iter().filter(|r| !writes.contains(r)).collect();
        RwResources { reads, writes }
    }

    pub fn reads(&self) -> impl Iterator<Item = &R> + '_ {
        self.reads.iter()
    }

    pub fn writes(&self) -> impl Iterator<Item = &R> + '_ {
        self.writes.iter()
    }

    pub fn map<R2, F>(&self, f: F) -> RwResources<R2>
    where
        R2: Eq + Hash,
        F: Fn(&R) -> R2,
    {
        RwResources {
            reads: self.reads.iter().map(&f).collect(),
            writes: self.writes.iter().map(&f).collect(),
        }
    }

    pub fn add_read(&mut self, r: R) {
        if !self.writes.contains(&r) {
            self.reads.insert(r);
        }
    }

    pub fn add_write(&mut self, r: R) {
        self.reads.remove(&r);
        self.writes.insert(r);
    }

    pub fn add_reads<I: IntoIterator<Item = R>>(&mut self, i: I) {
        for r in i.into_iter() {
            self.add_read(r);
        }
    }

    pub fn add_writes<I: IntoIterator<Item = R>>(&mut self, i: I) {
        for r in i.into_iter() {
            self.add_write(r);
        }
    }

    pub fn read(mut self, r: R) -> Self {
        self.add_read(r);
        self
    }

    pub fn write(mut self, r: R) -> Self {
        self.add_write(r);
        self
    }
}

impl<R: Eq + Hash + Clone> Resources for RwResources<R> {
    fn union(&mut self, other: &Self) {
        for w in &other.writes {
            self.writes.insert(w.clone());
        }

        for r in &other.reads {
            if !self.writes.contains(r) {
                self.reads.insert(r.clone());
            }
        }
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        self.writes.intersection(&other.reads).next().is_some()
            || self.writes.intersection(&other.writes).next().is_some()
            || other.writes.intersection(&self.reads).next().is_some()
            || other.writes.intersection(&self.writes).next().is_some()
    }
}
