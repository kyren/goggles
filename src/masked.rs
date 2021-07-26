use std::mem;

use hibitset::{BitIter, BitSet, BitSetLike};

use crate::{
    join::{Index, Join},
    storage::{DenseStorage, RawStorage},
    tracked::{ModifiedBitSet, TrackedStorage},
};

/// Wraps a `RawStorage` for some component with a `BitSet` mask to provide a safe, `Join`-able
/// interface for component storage.
pub struct MaskedStorage<S: RawStorage> {
    mask: BitSet,
    storage: S,
}

impl<S: RawStorage + Default> Default for MaskedStorage<S> {
    fn default() -> Self {
        Self {
            mask: Default::default(),
            storage: Default::default(),
        }
    }
}

impl<S: RawStorage> MaskedStorage<S> {
    pub fn mask(&self) -> &BitSet {
        &self.mask
    }

    pub fn raw_storage(&self) -> &S {
        &self.storage
    }

    pub fn raw_storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn contains(&self, index: Index) -> bool {
        self.mask.contains(index)
    }

    pub fn get(&self, index: Index) -> Option<&S::Item> {
        if self.mask.contains(index) {
            Some(unsafe { self.storage.get(index) })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, index: Index) -> Option<&mut S::Item> {
        if self.mask.contains(index) {
            Some(unsafe { self.storage.get_mut(index) })
        } else {
            None
        }
    }

    /// Returns a `GuardedElement` which does not automatically call `RawStorage::get_mut` on the
    /// underlying storage, which can be useful to avoid flagging modification in a
    /// `FlaggedStorage`.
    ///
    /// This is the same type returned by a guarded join.
    pub fn get_guard<'a>(&'a mut self, index: Index) -> Option<GuardedElement<'a, S>> {
        if self.mask.contains(index) {
            Some(GuardedElement {
                storage: &self.storage,
                index,
            })
        } else {
            None
        }
    }

    pub fn get_or_insert_with(
        &mut self,
        index: Index,
        f: impl FnOnce() -> S::Item,
    ) -> &mut S::Item {
        if !self.mask.contains(index) {
            self.mask.add(index);
            unsafe { self.storage.insert(index, f()) };
        }
        unsafe { self.storage.get_mut(index) }
    }

    pub fn insert(&mut self, index: Index, mut v: S::Item) -> Option<S::Item> {
        if self.mask.contains(index) {
            mem::swap(&mut v, unsafe { self.storage.get_mut(index) });
            Some(v)
        } else {
            self.mask.add(index);
            unsafe { self.storage.insert(index, v) };
            None
        }
    }

    pub fn remove(&mut self, index: Index) -> Option<S::Item> {
        if self.mask.remove(index) {
            Some(unsafe { self.storage.remove(index) })
        } else {
            None
        }
    }

    /// Returns an `IntoJoin` type whose values are `GuardedJoin` wrappers.
    ///
    /// A `GuardedJoin` wrapper does not automatically call `RawStorage::get_mut`, so it can be
    /// useful to avoid flagging modifications with a `FlaggedStorage`.
    pub fn guard(&mut self) -> GuardedJoin<S> {
        GuardedJoin(self)
    }
}

impl<S: DenseStorage> MaskedStorage<S> {
    pub fn as_slice(&self) -> &[S::Item] {
        self.storage.as_slice()
    }

    pub fn as_mut_slice(&mut self) -> &mut [S::Item] {
        self.storage.as_mut_slice()
    }
}

impl<S: TrackedStorage> MaskedStorage<S> {
    pub fn tracking_modified(&self) -> bool {
        self.storage.tracking_modified()
    }

    pub fn modified_indexes(&self) -> &ModifiedBitSet {
        self.storage.modified_indexes()
    }

    pub fn set_track_modified(&mut self, flag: bool) {
        self.storage.set_track_modified(flag);
    }

    pub fn mark_modified(&self, index: Index) {
        self.storage.mark_modified(index);
    }

    pub fn clear_modified(&mut self) {
        self.storage.clear_modified();
    }

    /// Returns an `IntoJoin` type which joins over all the modified elements.
    ///
    /// The items on the returned join are all `Option<&S::Item>`, removed elements will show up as
    /// None.
    pub fn modified(&self) -> ModifiedJoin<S> {
        ModifiedJoin(self)
    }

    /// Returns an `IntoJoin` type which joins over all the modified elements mutably.
    ///
    /// This is similar to `MaskedStorage::modified`, but returns mutable access to each item.
    pub fn modified_mut(&mut self) -> ModifiedJoinMut<S> {
        ModifiedJoinMut(self)
    }
}

impl<'a, S: RawStorage> Join for &'a MaskedStorage<S> {
    type Item = &'a S::Item;
    type Access = &'a S;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.mask, &self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        access.get(index)
    }
}

impl<'a, S: RawStorage> Join for &'a mut MaskedStorage<S> {
    type Item = &'a mut S::Item;
    type Access = &'a S;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.mask, &self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        access.get_mut(index)
    }
}

impl<S: RawStorage> Drop for MaskedStorage<S> {
    fn drop(&mut self) {
        struct DropGuard<'a, 'b, S: RawStorage>(Option<&'b mut BitIter<&'a BitSet>>, &'b mut S);

        impl<'a, 'b, S: RawStorage> Drop for DropGuard<'a, 'b, S> {
            fn drop(&mut self) {
                if let Some(iter) = self.0.take() {
                    let mut guard: DropGuard<S> = DropGuard(Some(&mut *iter), &mut *self.1);
                    while let Some(index) = guard.0.as_mut().unwrap().next() {
                        unsafe { S::remove(&mut guard.1, index) };
                    }
                    guard.0 = None;
                }
            }
        }

        let mut iter = (&self.mask).iter();
        DropGuard::<S>(Some(&mut iter), &mut self.storage);
    }
}

pub struct GuardedJoin<'a, S: RawStorage>(&'a mut MaskedStorage<S>);

impl<'a, S: RawStorage> Join for GuardedJoin<'a, S> {
    type Item = GuardedElement<'a, S>;
    type Access = &'a S;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.0.mask, &self.0.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        GuardedElement {
            storage: *access,
            index,
        }
    }
}

pub struct GuardedElement<'a, S> {
    storage: &'a S,
    index: Index,
}

impl<'a, S: RawStorage> GuardedElement<'a, S> {
    pub fn get(&self) -> &'a S::Item {
        unsafe { self.storage.get(self.index) }
    }

    pub fn get_mut(&mut self) -> &'a mut S::Item {
        unsafe { self.storage.get_mut(self.index) }
    }
}

impl<'a, S: TrackedStorage> GuardedElement<'a, S> {
    pub fn mark_modified(&self) {
        self.storage.mark_modified(self.index);
    }
}

pub struct ModifiedJoin<'a, S: RawStorage>(&'a MaskedStorage<S>);

impl<'a, S: TrackedStorage> Join for ModifiedJoin<'a, S> {
    type Item = Option<&'a S::Item>;
    type Access = (&'a BitSet, &'a S);
    type Mask = &'a ModifiedBitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (
            &self.0.storage.modified_indexes(),
            (&self.0.mask, &self.0.storage),
        )
    }

    unsafe fn get((mask, storage): &Self::Access, index: Index) -> Self::Item {
        if mask.contains(index) {
            Some(storage.get(index))
        } else {
            None
        }
    }
}

pub struct ModifiedJoinMut<'a, S: RawStorage>(&'a mut MaskedStorage<S>);

impl<'a, S: TrackedStorage> Join for ModifiedJoinMut<'a, S> {
    type Item = Option<&'a mut S::Item>;
    type Access = (&'a BitSet, &'a S);
    type Mask = &'a ModifiedBitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (
            &self.0.storage.modified_indexes(),
            (&self.0.mask, &self.0.storage),
        )
    }

    unsafe fn get((mask, storage): &Self::Access, index: Index) -> Self::Item {
        if mask.contains(index) {
            Some(storage.get_mut(index))
        } else {
            None
        }
    }
}
