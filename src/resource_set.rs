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

    /// Try to borrow the given resource immutably.
    ///
    /// If the resource does not exist, returns None.
    ///
    /// # Panics
    /// Panics if the resource is already borrowed mutably.
    pub fn try_fetch<T>(&self) -> Option<AtomicRef<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        self.resources.get(&Resource::<T>::id()).map(|r| {
            AtomicRef::map(r.borrow(), |r| {
                r.downcast_ref::<Resource<T>>().unwrap().get()
            })
        })
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn fetch<T>(&self) -> AtomicRef<T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.try_fetch().expect("no such resource")
    }

    /// Try to borrow the given resource mutably.
    ///
    /// If the resource does not exist, returns None.
    ///
    /// # Panics
    /// Panics if the resource is already borrowed.
    pub fn try_fetch_mut<T>(&self) -> Option<AtomicRefMut<T>>
    where
        T: Any + Send + 'static,
    {
        self.resources.get(&Resource::<T>::id()).map(|r| {
            AtomicRefMut::map(r.borrow_mut(), |r| {
                r.downcast_mut::<Resource<T>>().unwrap().get_mut()
            })
        })
    }

    /// Borrow the given resource mutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed.
    pub fn fetch_mut<T>(&self) -> AtomicRefMut<T>
    where
        T: Any + Send + 'static,
    {
        self.try_fetch_mut().expect("no such resource")
    }

    /// Fetch the given `SystemData`.
    pub fn system_data<'a, S>(&'a self) -> S
    where
        S: SystemData<'a>,
    {
        S::fetch(self)
    }
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

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

/// A trait for statically defining mutable and immutable resources from a `ResourceSet` for use
/// with the `par_seq` module.
///
/// `SystemData` can be a `Read` or a `Write` of any resource type, as well as a tuple of inner
/// types which implement `SystemData` and do not have conflicting resource requirements.
pub trait SystemData<'a> {
    fn check_resources() -> Result<RwResources<TypeId>, ResourceConflict>;
    fn fetch(set: &'a ResourceSet) -> Self;
}

pub struct TryRead<'a, T>(AtomicRef<'a, T>);

impl<'a, T> SystemData<'a> for Option<TryRead<'a, T>>
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
        Some(TryRead(set.try_fetch()?))
    }
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
        Read(set.fetch())
    }
}

pub struct TryWrite<'a, T>(AtomicRefMut<'a, T>);

impl<'a, T> SystemData<'a> for Option<TryWrite<'a, T>>
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
        Some(TryWrite(set.try_fetch_mut()?))
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
        Write(set.fetch_mut())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_data() {
        struct A;
        struct B;
        struct C;

        let mut res = ResourceSet::new();
        res.insert(A);
        res.insert(B);
        res.insert(C);

        let _sys_data = res.system_data::<(Read<A>, Write<B>, Write<C>)>();
    }

    #[test]
    fn test_conflicts() {
        struct A;
        struct B;

        assert!(<(Read<A>, Read<B>, Write<A>)>::check_resources().is_err());
    }
}
