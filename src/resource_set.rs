use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    iter,
    ops::{Deref, DerefMut},
};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};

use crate::{
    make_sync::MakeSync,
    par_seq::{ResourceConflict, RwResources},
    system_data::SystemData,
};

#[derive(Default)]
pub struct ResourceSet {
    resources: HashMap<ResourceId, AtomicRefCell<Box<dyn Any + Send + Sync>>>,
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
            .insert(
                ResourceId::of::<T>(),
                AtomicRefCell::new(Box::new(Resource::new(r))),
            )
            .map(|r| {
                Box::<dyn Any + Send>::from(r.into_inner())
                    .downcast::<Resource<T>>()
                    .unwrap()
                    .into_inner()
            })
    }

    pub fn contains<T>(&self) -> bool
    where
        T: Send + 'static,
    {
        self.resources.contains_key(&ResourceId::of::<T>())
    }

    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: Send + 'static,
    {
        self.resources.remove(&ResourceId::of::<T>()).map(|r| {
            Box::<dyn Any + Send>::from(r.into_inner())
                .downcast::<Resource<T>>()
                .unwrap()
                .into_inner()
        })
    }

    pub fn get_mut<T>(&mut self) -> &mut T
        where T: 'static,
    {
        if let Some(r) = self.resources.get_mut(&ResourceId::of::<T>()) {
            r.get_mut().downcast_mut::<Resource<T>>().unwrap().get_mut()
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn borrow<T>(&self) -> AtomicRef<T>
    where
        T: Send + Sync + 'static,
    {
        if let Some(r) = self.resources.get(&ResourceId::of::<T>()) {
            AtomicRef::map(r.borrow(), |r| {
                r.downcast_ref::<Resource<T>>().unwrap().get()
            })
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
        if let Some(r) = self.resources.get(&ResourceId::of::<T>()) {
            AtomicRefMut::map(r.borrow_mut(), |r| {
                r.downcast_mut::<Resource<T>>().unwrap().get_mut()
            })
        } else {
            panic!("no such resource {:?}", type_name::<T>());
        }
    }

    /// Fetch the given `SystemData`.
    pub fn fetch<'a, S>(&'a self) -> S
    where
        S: SystemData<'a, Source = ResourceSet, Resources = RwResources<ResourceId>>,
    {
        S::fetch(self)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

impl ResourceId {
    pub fn of<C: 'static>() -> ResourceId {
        ResourceId(TypeId::of::<C>())
    }
}

pub struct Read<'a, T>(AtomicRef<'a, T>);

impl<'a, T> SystemData<'a> for Read<'a, T>
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

pub struct Write<'a, T>(AtomicRefMut<'a, T>);

impl<'a, T> SystemData<'a> for Write<'a, T>
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

type Resource<T> = MakeSync<T>;
