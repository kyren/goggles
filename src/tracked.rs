use hibitset::AtomicBitSet;

use crate::{
    component::{Component, RawStorage},
    join::Index,
};

pub trait TrackedStorage<C>: RawStorage<C> {
    fn set_track_modified(&mut self, flag: bool);
    fn tracking_modified(&self) -> bool;

    fn modified(&self) -> &AtomicBitSet;
    fn clear_modified(&mut self);
}

/// Storage that can optionally track the indexes of any changed components.
///
/// Any call to the `get_mut`, `insert`, or `remove` methods of `RawStorage` will set modification
/// bits for that index if tracking is turned on.
///
/// By default, tracking is *not* turned on, you must turn it on by calling
/// `set_track_modified(true)`.
#[derive(Default)]
pub struct Flagged<S> {
    tracking: bool,
    storage: S,
    modified: AtomicBitSet,
}

impl<C, S> RawStorage<C> for Flagged<S>
where
    C: Component,
    S: RawStorage<C>,
{
    unsafe fn get(&self, index: Index) -> &C {
        self.storage.get(index)
    }

    unsafe fn get_mut(&self, index: Index) -> &mut C {
        if self.tracking {
            self.modified.add_atomic(index);
        }
        self.storage.get_mut(index)
    }

    unsafe fn insert(&mut self, index: Index, value: C) {
        if self.tracking {
            self.modified.add(index);
        }
        self.storage.insert(index, value);
    }

    unsafe fn remove(&mut self, index: Index) -> C {
        if self.tracking {
            self.modified.add(index);
        }
        self.storage.remove(index)
    }
}

impl<C, S> TrackedStorage<C> for Flagged<S>
where
    C: Component,
    S: RawStorage<C>,
{
    fn set_track_modified(&mut self, flag: bool) {
        self.tracking = flag;
    }

    fn tracking_modified(&self) -> bool {
        self.tracking
    }

    fn modified(&self) -> &AtomicBitSet {
        &self.modified
    }

    fn clear_modified(&mut self) {
        self.modified.clear();
    }
}
