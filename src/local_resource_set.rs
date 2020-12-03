use std::{
    any::{type_name, TypeId},
    cell::{Ref, RefCell, RefMut},
    iter,
    ops::{Deref, DerefMut},
};

use anymap::{any::Any, Map};

use crate::{
    fetch_resources::FetchResources,
    resources::{ResourceConflict, RwResources},
};

/// Store a set of arbitrary types inside `AtomicRefCell`s, and then access them for either reading
/// or writing.
pub struct ResourceSet {
    resources: Map<dyn Any>,
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
        T: 'static,
    {
        self.resources
            .insert::<RefCell<T>>(RefCell::new(r))
            .map(|r| r.into_inner())
    }

    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: 'static,
    {
        self.resources
            .remove::<RefCell<T>>()
            .map(|r| r.into_inner())
    }

    pub fn contains<T>(&self) -> bool
    where
        T: 'static,
    {
        self.resources.contains::<RefCell<T>>()
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn borrow<T>(&self) -> Ref<T>
    where
        T: 'static,
    {
        if let Some(r) = self.resources.get::<RefCell<T>>() {
            r.borrow()
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Borrow the given resource mutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed.
    pub fn borrow_mut<T>(&self) -> RefMut<T>
    where
        T: 'static,
    {
        if let Some(r) = self.resources.get::<RefCell<T>>() {
            r.borrow_mut()
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// # Panics
    /// Panics if the resource has not been inserted.
    pub fn get_mut<T>(&mut self) -> &mut T
    where
        T: 'static,
    {
        if let Some(r) = self.resources.get_mut::<RefCell<T>>() {
            r.get_mut()
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Fetch the given `FetchResources`.
    pub fn fetch<'a, F>(&'a self) -> F
    where
        F: FetchResources<'a, Self>,
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
pub struct Read<'a, T>(Ref<'a, T>);

impl<'a, T> FetchResources<'a, ResourceSet> for Read<'a, T>
where
    T: 'static,
{
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
pub struct Write<'a, T>(RefMut<'a, T>);

impl<'a, T> FetchResources<'a, ResourceSet> for Write<'a, T>
where
    T: 'static,
{
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
