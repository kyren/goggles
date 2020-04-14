use hibitset::AtomicBitSet;

use crate::{join::Index, storage::RawStorage};

pub trait TrackedStorage: RawStorage {
    /// If this is true, then calls to `get_mut`, `insert`, and `remove` will automatically set
    /// modified bits.
    fn set_track_modified(&mut self, flag: bool);
    fn tracking_modified(&self) -> bool;

    /// Manually mark an index as modified.
    fn mark_modified(&self, index: Index);

    fn modified(&self) -> &AtomicBitSet;

    /// Clear the modified bitset.
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

impl<S> RawStorage for Flagged<S>
where
    S: RawStorage,
{
    type Item = S::Item;

    unsafe fn get(&self, index: Index) -> &Self::Item {
        self.storage.get(index)
    }

    unsafe fn get_mut(&self, index: Index) -> &mut Self::Item {
        if self.tracking {
            self.modified.add_atomic(index);
        }
        self.storage.get_mut(index)
    }

    unsafe fn insert(&mut self, index: Index, value: Self::Item) {
        if self.tracking {
            self.modified.add(index);
        }
        self.storage.insert(index, value);
    }

    unsafe fn remove(&mut self, index: Index) -> Self::Item {
        if self.tracking {
            self.modified.add(index);
        }
        self.storage.remove(index)
    }
}

impl<S> TrackedStorage for Flagged<S>
where
    S: RawStorage,
{
    fn set_track_modified(&mut self, flag: bool) {
        self.tracking = flag;
    }

    fn tracking_modified(&self) -> bool {
        self.tracking
    }

    fn mark_modified(&self, index: Index) {
        self.modified.add_atomic(index);
    }

    fn modified(&self) -> &AtomicBitSet {
        &self.modified
    }

    fn clear_modified(&mut self) {
        self.modified.clear();
    }
}
