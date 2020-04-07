use std::{
    any::{type_name, Any, TypeId},
    collections::HashMap,
    iter,
};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};

use crate::{
    make_sync::MakeSync,
    par_seq::{ResourceConflict, Resources, RwResources},
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
        S: SystemData<'a>,
    {
        S::fetch(self)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

/// A trait for statically defining mutable and immutable resources from a `ResourceSet` for use
/// with the `par_seq` module.
///
/// `SystemData` can be a `Read` or a `Write` of any resource type, as well as a tuple of inner
/// types which implement `SystemData` and do not have conflicting resource requirements.
pub trait SystemData<'a> {
    fn check_resources() -> Result<RwResources<TypeId>, ResourceConflict>;
    fn fetch(set: &'a ResourceSet) -> Self;
}

pub struct Read<'a, T>(AtomicRef<'a, T>);

impl<'a, T> SystemData<'a> for Read<'a, T>
where
    T: Any + Send + Sync + 'static,
{
    fn check_resources() -> Result<RwResources<TypeId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(TypeId::of::<T>()),
            iter::empty(),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Read(set.borrow())
    }
}

pub struct Write<'a, T>(AtomicRefMut<'a, T>);

impl<'a, T> SystemData<'a> for Write<'a, T>
where
    T: Any + Send + 'static,
{
    fn check_resources() -> Result<RwResources<TypeId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::empty(),
            iter::once(TypeId::of::<T>()),
        ))
    }

    fn fetch(set: &'a ResourceSet) -> Self {
        Write(set.borrow_mut())
    }
}

macro_rules! impl_data {
    ($($ty:ident),*) => {
        impl<'a, $($ty),*> SystemData<'a> for ($($ty,)*)
            where $($ty: SystemData<'a>),*
            {
                fn check_resources() -> Result<RwResources<TypeId>, ResourceConflict> {
                    let mut resources = RwResources::default();
                    $({
                        let r = <$ty as SystemData>::check_resources()?;
                        if resources.conflicts_with(&r) {
                            return Err(ResourceConflict { type_name: type_name::<Self>() });
                        }
                        resources.union(&r);
                    })*
                    Ok(resources)
                }

                fn fetch(world: &'a ResourceSet) -> Self {
                    ($(<$ty as SystemData<'a>>::fetch(world),)*)
                }
            }
    };
}

impl_data!(A);
impl_data!(A, B);
impl_data!(A, B, C);
impl_data!(A, B, C, D);
impl_data!(A, B, C, D, E);
impl_data!(A, B, C, D, E, F);
impl_data!(A, B, C, D, E, F, G);
impl_data!(A, B, C, D, E, F, G, H);
impl_data!(A, B, C, D, E, F, G, H, I);
impl_data!(A, B, C, D, E, F, G, H, I, J);
impl_data!(A, B, C, D, E, F, G, H, I, J, K);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y);
impl_data!(A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z);

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
