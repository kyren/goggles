use std::{
    any::Any,
    cell::UnsafeCell,
    collections::HashMap,
    mem::{self, MaybeUninit},
    ptr,
};

use hibitset::{BitIter, BitSet, BitSetLike};

use crate::{entity::Index, join::Join};

pub trait Component: Any + Sized {
    type Storage: RawStorage<Self> + Any;
}

pub trait RawStorage<C> {
    unsafe fn ptr(&self, index: Index) -> *mut C;
    unsafe fn insert(&mut self, index: Index, value: C);
    unsafe fn remove(&mut self, index: Index) -> C;
}

pub struct VecStorage<C>(Vec<UnsafeCell<MaybeUninit<C>>>);

unsafe impl<C: Send> Send for VecStorage<C> {}
unsafe impl<C: Sync> Sync for VecStorage<C> {}

impl<C> Default for VecStorage<C> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<C> RawStorage<C> for VecStorage<C> {
    unsafe fn ptr(&self, index: Index) -> *mut C {
        (*self.0.get_unchecked(index as usize).get()).as_mut_ptr()
    }

    unsafe fn insert(&mut self, index: Index, c: C) {
        let index = index as usize;
        if self.0.len() <= index {
            let delta = index + 1 - self.0.len();
            self.0.reserve(delta);
            self.0.set_len(index + 1);
        }
        *self.0.get_unchecked_mut(index as usize) = UnsafeCell::new(MaybeUninit::new(c));
    }

    unsafe fn remove(&mut self, index: Index) -> C {
        ptr::read((*self.0.get_unchecked(index as usize).get()).as_mut_ptr())
    }
}

pub struct DenseVecStorage<C> {
    data: Vec<MaybeUninit<Index>>,
    values: Vec<UnsafeCell<C>>,
    indexes: Vec<Index>,
}

unsafe impl<C: Send> Send for DenseVecStorage<C> {}
unsafe impl<C: Sync> Sync for DenseVecStorage<C> {}

impl<C> Default for DenseVecStorage<C> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            values: Vec::new(),
            indexes: Vec::new(),
        }
    }
}

impl<C> RawStorage<C> for DenseVecStorage<C> {
    unsafe fn ptr(&self, index: Index) -> *mut C {
        let dind = *self.data.get_unchecked(index as usize).as_ptr();
        self.values.get_unchecked(dind as usize).get()
    }

    unsafe fn insert(&mut self, index: Index, c: C) {
        if self.data.len() <= index as usize {
            let delta = index as usize + 1 - self.data.len();
            self.data.reserve(delta);
            self.data.set_len(index as usize + 1);
        }
        self.indexes.reserve(1);
        self.values.reserve(1);

        self.data
            .get_unchecked_mut(index as usize)
            .as_mut_ptr()
            .write(self.values.len() as Index);
        self.indexes.push(index);
        self.values.push(UnsafeCell::new(c));
    }

    unsafe fn remove(&mut self, index: Index) -> C {
        let dind = *self.data.get_unchecked(index as usize).as_ptr();
        let last_index = *self.indexes.get_unchecked(self.indexes.len() - 1);
        self.data
            .get_unchecked_mut(last_index as usize)
            .as_mut_ptr()
            .write(dind);
        self.indexes.swap_remove(dind as usize);
        self.values.swap_remove(dind as usize).into_inner()
    }
}

pub struct HashMapStorage<C>(HashMap<Index, UnsafeCell<C>>);

unsafe impl<C: Send> Send for HashMapStorage<C> {}
unsafe impl<C: Sync> Sync for HashMapStorage<C> {}

impl<C> Default for HashMapStorage<C> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

impl<C> RawStorage<C> for HashMapStorage<C> {
    unsafe fn ptr(&self, index: Index) -> *mut C {
        self.0.get(&index).unwrap().get()
    }

    unsafe fn insert(&mut self, index: Index, v: C) {
        self.0.insert(index, UnsafeCell::new(v));
    }

    unsafe fn remove(&mut self, index: Index) -> C {
        self.0.remove(&index).unwrap().into_inner()
    }
}

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

    pub fn storage(&self) -> &C::Storage {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut C::Storage {
        &mut self.storage
    }

    pub fn get(&self, index: Index) -> Option<&C> {
        if self.mask.contains(index) {
            Some(unsafe { &*self.storage.ptr(index) })
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, index: Index) -> Option<&mut C> {
        if self.mask.contains(index) {
            Some(unsafe { &mut *self.storage.ptr(index) })
        } else {
            None
        }
    }

    pub fn insert(&mut self, index: Index, mut c: C) -> Option<C> {
        if self.mask.contains(index) {
            mem::swap(&mut c, unsafe { &mut *self.storage.ptr(index) });
            Some(c)
        } else {
            self.mask.add(index);
            unsafe { self.storage.insert(index, c) };
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
}

impl<'a, C: Component> Join for &'a MaskedStorage<C> {
    type Item = &'a C;
    type Access = &'a C::Storage;
    type Mask = &'a BitSet;

    fn open(self) -> (Self::Mask, Self::Access) {
        (&self.mask, &self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        &*access.ptr(index)
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
        &mut *access.ptr(index)
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
