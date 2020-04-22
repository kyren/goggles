use std::{
    any::{type_name, TypeId},
    iter,
    ops::{Deref, DerefMut},
};

use anymap::{any::Any, Map};
use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};

use crate::{
    fetch_resources::FetchResources,
    make_sync::MakeSync,
    resources::{ResourceConflict, RwResources},
};

/// Store a set of arbitrary types inside `AtomicRefCell`s, and then access them for either reading
/// or writing.
pub struct ResourceSet {
    resources: Map<dyn Any + Send + Sync>,
}

impl Default for ResourceSet {
    fn default() -> Self {
        ResourceSet {
            resources: Map::new(),
        }
    }
}

impl ResourceSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T>(&mut self, r: T) -> Option<T>
    where
        T: Send + 'static,
    {
        self.resources
            .insert::<Resource<T>>(AtomicRefCell::new(MakeSync::new(r)))
            .map(|r| r.into_inner().into_inner())
    }

    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: Send + 'static,
    {
        self.resources
            .remove::<Resource<T>>()
            .map(|r| r.into_inner().into_inner())
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Send + 'static,
    {
        self.resources.contains::<Resource<T>>()
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn borrow<T>(&self) -> AtomicRef<T>
    where
        T: Send + Sync + 'static,
    {
        if let Some(r) = self.resources.get::<Resource<T>>() {
            AtomicRef::map(r.borrow(), |r| r.get())
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Borrow the given resource mutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed.
    pub fn borrow_mut<T>(&self) -> AtomicRefMut<T>
    where
        T: Send + 'static,
    {
        if let Some(r) = self.resources.get::<Resource<T>>() {
            AtomicRefMut::map(r.borrow_mut(), |r| r.get_mut())
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// # Panics
    /// Panics if the resource has not been inserted.
    pub fn get_mut<T>(&mut self) -> &mut T
    where
        T: Send + 'static,
    {
        if let Some(r) = self.resources.get_mut::<Resource<T>>() {
            r.get_mut().get_mut()
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Fetch the given `FetchResources`.
    pub fn fetch<'a, F>(&'a self) -> F
    where
        F: FetchResources<'a, Source = ResourceSet, Resources = RwResources<ResourceId>>,
    {
        F::fetch(self)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

impl ResourceId {
    pub fn of<C: 'static>() -> ResourceId {
        ResourceId(TypeId::of::<C>())
    }
}

/// `SystemData` type that reads the given resource.
///
/// # Panics
/// Panics if the resource does not exist or has already been borrowed for writing.
pub struct Read<'a, T>(AtomicRef<'a, T>);

impl<'a, T> FetchResources<'a> for Read<'a, T>
where
    T: Send + Sync + 'static,
{
    type Source = ResourceSet;
    type Resources = RwResources<ResourceId>;

    fn check_resources() -> Result<RwResources<ResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(ResourceId::of::<T>()),
            iter::empty(),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Read(set.borrow())
    }
}

impl<'a, T> Deref for Read<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.0
    }
}

/// `SystemData` type that writes the given resource.
///
/// # Panics
/// Panics if the resource does not exist or has already been borrowed for writing.
pub struct Write<'a, T>(AtomicRefMut<'a, T>);

impl<'a, T> FetchResources<'a> for Write<'a, T>
where
    T: Send + 'static,
{
    type Source = ResourceSet;
    type Resources = RwResources<ResourceId>;

    fn check_resources() -> Result<RwResources<ResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::empty(),
            iter::once(ResourceId::of::<T>()),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Write(set.borrow_mut())
    }
}

impl<'a, T> Deref for Write<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &*self.0
    }
}

impl<'a, T> DerefMut for Write<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.0
    }
}

type Resource<T> = AtomicRefCell<MakeSync<T>>;
