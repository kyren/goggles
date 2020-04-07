use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    iter,
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
        T: Any + Send + 'static,
    {
        self.resources
            .insert(
                Resource::<T>::id(),
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
        T: Any + Send + 'static,
    {
        self.resources.contains_key(&Resource::<T>::id())
    }

    pub fn remove<T>(&mut self) -> Option<T>
    where
        T: Any + Send + 'static,
    {
        self.resources.remove(&Resource::<T>::id()).map(|r| {
            Box::<dyn Any + Send>::from(r.into_inner())
                .downcast::<Resource<T>>()
                .unwrap()
                .into_inner()
        })
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn borrow<T>(&self) -> AtomicRef<T>
    where
        T: Any + Send + Sync + 'static,
    {
        if let Some(r) = self.resources.get(&Resource::<T>::id()) {
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
        T: Any + Send + 'static,
    {
        if let Some(r) = self.resources.get(&Resource::<T>::id()) {
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

pub struct Read<'a, T>(pub AtomicRef<'a, T>);

impl<'a, T> SystemData<'a> for Read<'a, T>
where
    T: Any + Send + Sync + 'static,
{
    type Source = ResourceSet;
    type Resources = RwResources<ResourceId>;

    fn check_resources() -> Result<RwResources<ResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(Resource::<T>::id()),
            iter::empty(),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Read(set.borrow())
    }
}

pub struct Write<'a, T>(pub AtomicRefMut<'a, T>);

impl<'a, T> SystemData<'a> for Write<'a, T>
where
    T: Any + Send + 'static,
{
    type Source = ResourceSet;
    type Resources = RwResources<ResourceId>;

    fn check_resources() -> Result<RwResources<ResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::empty(),
            iter::once(Resource::<T>::id()),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Write(set.borrow_mut())
    }
}

struct Resource<T>(MakeSync<T>);

impl<T: Any> Resource<T> {
    fn id() -> ResourceId {
        ResourceId(TypeId::of::<T>())
    }

    fn new(t: T) -> Resource<T> {
        Resource(MakeSync::new(t))
    }

    fn into_inner(self) -> T {
        self.0.into_inner()
    }

    fn get(&self) -> &T
    where
        T: Sync,
    {
        self.0.get()
    }

    fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }
}
