use std::mem::{self};

use hibitset::{BitIter, BitSet, BitSetLike};

use crate::{
    component::{Component, RawStorage},
    join::{Index, Join},
};

/// Wraps a `RawStorage` for some component with a `BitSet` mask to provide a safe, `Join`-able
/// interface for component storage.
pub struct MaskedStorage<C: Component> {
    mask: BitSet,
    storage: C::Storage,
}

impl<C: Component> Default for MaskedStorage<C>
where
    C::Storage: Default,
{
    fn default() -> Self {
        Self {
            mask: Default::default(),
            storage: Default::default(),
        }
    }
}

impl<C: Component> MaskedStorage<C> {
    pub fn mask(&self) -> &BitSet {
        &self.mask
    }

    pub fn raw_storage(&self) -> &C::Storage {
        &self.storage
    }

    pub fn raw_storage_mut(&mut self) -> &mut C::Storage {
        &mut self.storage
    }

    pub fn contains(&self, index: Index) -> bool {
        self.mask.contains(index)
    }

    pub fn get(&self, index: Index) -> Option<&C> {
        if self.mask.contains(index) {
            Some(unsafe { self.storage.get(index) })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, index: Index) -> Option<&mut C> {
        if self.mask.contains(index) {
            Some(unsafe { self.storage.get_mut(index) })
        } else {
            None
        }
    }

    pub fn insert(&mut self, index: Index, mut c: C) -> Option<C> {
        if self.mask.contains(index) {
            mem::swap(&mut c, unsafe { self.storage.get_mut(index) });
            Some(c)
        } else {
            self.mask.add(index);
            unsafe { self.storage.insert(index, c) };
            None
        }
    }

    /// Update the value at this index only if it has changed.
    ///
    /// This is useful when combined with `FlaggedStorage`, which keeps track of modified
    /// components.  By using this method, you can avoid flagging changes unnecessarily when the new
    /// value of the component is equal to the old one.
    pub fn update(&mut self, index: Index, mut c: C) -> Option<C>
    where
        C: PartialEq,
    {
        if self.mask.contains(index) {
            unsafe {
                if &c != self.storage.get(index) {
                    mem::swap(&mut c, self.storage.get_mut(index));
                }
            }
            Some(c)
        } else {
            None
        }
    }

    pub fn remove(&mut self, index: Index) -> Option<C> {
        if self.mask.remove(index) {
            Some(unsafe { self.storage.remove(index) })
        } else {
            None
        }
    }

    pub fn guard(&mut self) -> GuardedJoin<C> {
        GuardedJoin(self)
    }
}

impl<'a, C: Component> Join for &'a MaskedStorage<C> {
    type Item = &'a C;
    type Access = &'a C::Storage;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.mask, &self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        access.get(index)
    }
}

impl<'a, C: Component> Join for &'a mut MaskedStorage<C> {
    type Item = &'a mut C;
    type Access = &'a C::Storage;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.mask, &self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        access.get_mut(index)
    }
}

impl<C: Component> Drop for MaskedStorage<C> {
    fn drop(&mut self) {
        struct DropGuard<'a, C: Component>(Option<BitIter<&'a BitSet>>, &'a mut C::Storage);

        impl<'a, C: Component> Drop for DropGuard<'a, C> {
            fn drop(&mut self) {
                let mut iter = self.0.take().unwrap();
                if let Some(index) = iter.next() {
                    let mut guard: DropGuard<C> = DropGuard(Some(iter), self.1);
                    unsafe { C::Storage::remove(&mut guard.1, index) };
                }
            }
        }

        DropGuard::<C>(Some((&self.mask).iter()), &mut self.storage);
    }
}

pub struct GuardedJoin<'a, C: Component>(&'a mut MaskedStorage<C>);

impl<'a, C: Component> Join for GuardedJoin<'a, C> {
    type Item = ElementGuard<'a, C>;
    type Access = &'a C::Storage;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.0.mask, &self.0.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        ElementGuard {
            storage: *access,
            index,
        }
    }
}

pub struct ElementGuard<'a, C: Component> {
    storage: &'a C::Storage,
    index: Index,
}

impl<'a, C: Component> ElementGuard<'a, C> {
    pub fn get(&self) -> &'a C {
        unsafe { self.storage.get(self.index) }
    }

    pub fn get_mut(&mut self) -> &'a mut C {
        unsafe { self.storage.get_mut(self.index) }
    }

    pub fn update(&mut self, mut c: C) -> C
    where
        C: PartialEq,
    {
        unsafe {
            if &c != self.storage.get(self.index) {
                mem::swap(&mut c, self.storage.get_mut(self.index));
            }
            c
        }
    }
}
