use std::{cell::UnsafeCell, collections::HashMap, mem::MaybeUninit, ptr};

use crate::join::Index;

/// A trait for storing components in memory based on low valued indexes.
///
/// Is not required to keep track of whether the component is present or not for a given index, it
/// is up to the user of a `RawStorage` to keep track of this.
///
/// Because of this, a type that implements `RawStorage` is allowed to leak *all* component values
/// on drop.  In order to prevent this, the storage must have only empty indexes at the time of
/// drop.
pub trait RawStorage {
    type Item;

    /// Return a reference to the component at the given index.
    ///
    /// You *must* only call `get` with index values that are non-empty (have been previously had
    /// components inserted with `insert`).
    unsafe fn get(&self, index: Index) -> &Self::Item;

    /// Return a mutable reference to the component at the given index.
    ///
    /// You *must* only call `get_mut` with index values that are non-empty (have been previously
    /// had components inserted with `insert`).
    ///
    /// Returns a *mutable* reference to the previously inserted component.  You must follow Rust's
    /// aliasing rules here, so you must not call this method if there is any other live reference
    /// to the same component.
    unsafe fn get_mut(&self, index: Index) -> &mut Self::Item;

    /// Insert a new component value in the given index.
    ///
    /// You must only call `insert` on indexes that are empty.  All indexes start empty, but become
    /// non-empty once `insert` is called on them.
    unsafe fn insert(&mut self, index: Index, value: Self::Item);

    /// Remove a component previously inserted in the given index.
    ///
    /// You must only call `remove` on a non-empty index (after you have inserted a value with
    /// `insert`).  After calling `remove` the index becomes empty.
    unsafe fn remove(&mut self, index: Index) -> Self::Item;
}

pub struct VecStorage<T>(Vec<UnsafeCell<MaybeUninit<T>>>);

unsafe impl<T: Send> Send for VecStorage<T> {}
unsafe impl<T: Sync> Sync for VecStorage<T> {}

impl<T> Default for VecStorage<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> RawStorage for VecStorage<T> {
    type Item = T;

    unsafe fn get(&self, index: Index) -> &T {
        &*(*self.0.get_unchecked(index as usize).get()).as_ptr()
    }

    unsafe fn get_mut(&self, index: Index) -> &mut T {
        &mut *(*self.0.get_unchecked(index as usize).get()).as_mut_ptr()
    }

    unsafe fn insert(&mut self, index: Index, c: T) {
        let index = index as usize;
        if self.0.len() <= index {
            let delta = index + 1 - self.0.len();
            self.0.reserve(delta);
            self.0.set_len(index + 1);
        }
        *self.0.get_unchecked_mut(index as usize) = UnsafeCell::new(MaybeUninit::new(c));
    }

    unsafe fn remove(&mut self, index: Index) -> T {
        ptr::read((*self.0.get_unchecked(index as usize).get()).as_mut_ptr())
    }
}

pub struct DenseVecStorage<T> {
    data: Vec<MaybeUninit<Index>>,
    values: Vec<UnsafeCell<T>>,
    indexes: Vec<Index>,
}

unsafe impl<T: Send> Send for DenseVecStorage<T> {}
unsafe impl<T: Sync> Sync for DenseVecStorage<T> {}

impl<T> Default for DenseVecStorage<T> {
    fn default() -> Self {
        Self {
            data: Vec::new(),
            values: Vec::new(),
            indexes: Vec::new(),
        }
    }
}

impl<T> RawStorage for DenseVecStorage<T> {
    type Item = T;

    unsafe fn get(&self, index: Index) -> &T {
        let dind = *self.data.get_unchecked(index as usize).as_ptr();
        &*self.values.get_unchecked(dind as usize).get()
    }

    unsafe fn get_mut(&self, index: Index) -> &mut T {
        let dind = *self.data.get_unchecked(index as usize).as_ptr();
        &mut *self.values.get_unchecked(dind as usize).get()
    }

    unsafe fn insert(&mut self, index: Index, c: T) {
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

    unsafe fn remove(&mut self, index: Index) -> T {
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

pub struct HashMapStorage<T>(HashMap<Index, UnsafeCell<T>>);

unsafe impl<T: Send> Send for HashMapStorage<T> {}
unsafe impl<T: Sync> Sync for HashMapStorage<T> {}

impl<T> Default for HashMapStorage<T> {
    fn default() -> Self {
        Self(HashMap::default())
    }
}

impl<T> RawStorage for HashMapStorage<T> {
    type Item = T;

    unsafe fn get(&self, index: Index) -> &T {
        &*self.0.get(&index).unwrap().get()
    }

    unsafe fn get_mut(&self, index: Index) -> &mut T {
        &mut *self.0.get(&index).unwrap().get()
    }

    unsafe fn insert(&mut self, index: Index, v: T) {
        self.0.insert(index, UnsafeCell::new(v));
    }

    unsafe fn remove(&mut self, index: Index) -> T {
        self.0.remove(&index).unwrap().into_inner()
    }
}
