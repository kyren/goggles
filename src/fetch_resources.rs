use std::{any::type_name, marker::PhantomData};

use crate::resources::{ResourceConflict, Resources};

/// A trait for statically defining mutable and immutable resources fetched from a data source which
/// may or may not conflict.
///
/// Tuples of types that implement `FetchResources` automatically themselves implement
/// `FetchResources` and correctly find the union of the resources they use.
pub trait FetchResources<'a> {
    type Source;
    type Resources: Resources;

    fn check_resources() -> Result<Self::Resources, ResourceConflict>;
    fn fetch(source: &'a Self::Source) -> Self;
}

/// An empty type useful in generic contexts that implements `FetchResources` but does not actually
/// fetch any resources.
pub struct FetchNone<S, R>(PhantomData<(S, R)>);

impl<S, R> Default for FetchNone<S, R> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<'a, S, R: Resources> FetchResources<'a> for FetchNone<S, R> {
    type Source = S;
    type Resources = R;

    fn check_resources() -> Result<Self::Resources, ResourceConflict> {
        Ok(R::default())
    }

    fn fetch(_: &'a Self::Source) -> Self {
        FetchNone::default()
    }
}

macro_rules! impl_data {
    ($($ty:ident),*) => {
        impl<'a, ST, RT, $($ty),*> FetchResources<'a> for ($($ty,)*)
        where
            RT: Resources,
            $($ty: FetchResources<'a, Source = ST, Resources = RT>),*
        {
            type Source = ST;
            type Resources = RT;

            fn check_resources() -> Result<Self::Resources, ResourceConflict> {
                let mut resources = Self::Resources::default();
                $({
                    let r = <$ty as FetchResources>::check_resources()?;
                    if resources.conflicts_with(&r) {
                        return Err(ResourceConflict { type_name: type_name::<Self>() });
                    }
                    resources.union(&r);
                })*
                Ok(resources)
            }

            fn fetch(source: &'a Self::Source) -> Self {
                ($(<$ty as FetchResources<'a>>::fetch(source),)*)
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
