use hibitset::{BitIter, BitProducer, BitSetAll, BitSetAnd, BitSetLike};
use rayon::iter::{
    plumbing::{bridge_unindexed, Folder, UnindexedConsumer, UnindexedProducer},
    ParallelIterator,
};
use thiserror::Error;

use crate::entity::Index;

#[derive(Debug, Error)]
#[error("cannot iterate over unconstrained Join")]
pub struct JoinIterUnconstrained;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum JoinConstraint {
    Constrained,
    Unconstrained,
}

impl JoinConstraint {
    #[inline]
    pub fn and(self, other: JoinConstraint) -> JoinConstraint {
        if self == JoinConstraint::Unconstrained && other == JoinConstraint::Unconstrained {
            JoinConstraint::Unconstrained
        } else {
            JoinConstraint::Constrained
        }
    }
}

pub trait Join {
    type Item;
    type Access;
    type Mask: BitSetLike;

    fn open(self) -> (Self::Mask, Self::Access, JoinConstraint);
    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item;
}

pub trait JoinExt: Join {
    fn join(self) -> JoinIter<Self>
    where
        Self: Sized,
    {
        JoinIter::new(self).unwrap()
    }

    fn par_join(self) -> JoinParIter<Self>
    where
        Self: Sized + Send,
        Self::Item: Send,
        Self::Access: Send + Sync,
        Self::Mask: Send + Sync,
    {
        JoinParIter::new(self).unwrap()
    }

    fn maybe(self) -> MaybeJoin<Self>
    where
        Self: Sized,
    {
        MaybeJoin(self)
    }
}

impl<J: Join> JoinExt for J {}

pub struct MaybeJoin<J: Join>(pub J);

impl<J: Join> Join for MaybeJoin<J> {
    type Item = Option<J::Item>;
    type Access = (J::Mask, J::Access);
    type Mask = BitSetAll;

    fn open(self) -> (Self::Mask, Self::Access, JoinConstraint) {
        let (mask, access, _) = self.0.open();
        (BitSetAll, (mask, access), JoinConstraint::Unconstrained)
    }

    unsafe fn get((mask, access): &Self::Access, index: Index) -> Self::Item {
        if mask.contains(index) {
            Some(J::get(access, index))
        } else {
            None
        }
    }
}

pub struct JoinIter<J: Join>(BitIter<J::Mask>, J::Access);

impl<J: Join> JoinIter<J> {
    pub fn new(j: J) -> Result<Self, JoinIterUnconstrained> {
        let (mask, access, constraint) = j.open();
        if constraint == JoinConstraint::Unconstrained {
            Err(JoinIterUnconstrained)
        } else {
            Ok(Self(mask.iter(), access))
        }
    }
}

impl<J: Join> Iterator for JoinIter<J> {
    type Item = J::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|index| unsafe { J::get(&self.1, index) })
    }
}

pub struct JoinParIter<J: Join>(J::Mask, J::Access);

impl<J: Join> JoinParIter<J> {
    pub fn new(j: J) -> Result<Self, JoinIterUnconstrained> {
        let (mask, access, constraint) = j.open();
        if constraint == JoinConstraint::Unconstrained {
            Err(JoinIterUnconstrained)
        } else {
            Ok(Self(mask, access))
        }
    }
}

impl<J> ParallelIterator for JoinParIter<J>
where
    J: Join + Send,
    J::Item: Send,
    J::Access: Send + Sync,
    J::Mask: Send + Sync,
{
    type Item = J::Item;

    fn drive_unindexed<C>(self, consumer: C) -> C::Result
    where
        C: UnindexedConsumer<Self::Item>,
    {
        // Split 3 layers when forking, makes the smallest unit of of work have a maximum size of
        // usize_bits
        const LAYERS_SPLIT: u8 = 3;

        let JoinParIter(mask, access) = self;
        let producer = BitProducer((&mask).iter(), LAYERS_SPLIT);
        bridge_unindexed(
            JoinProducer::<J> {
                producer,
                access: &access,
            },
            consumer,
        )
    }
}

struct JoinProducer<'a, J>
where
    J: Join + Send,
    J::Item: Send,
    J::Access: Sync + 'a,
    J::Mask: Send + Sync + 'a,
{
    producer: BitProducer<'a, J::Mask>,
    access: &'a J::Access,
}

impl<'a, J> UnindexedProducer for JoinProducer<'a, J>
where
    J: Join + Send,
    J::Item: Send,
    J::Access: Sync + 'a,
    J::Mask: Send + Sync + 'a,
{
    type Item = J::Item;

    fn split(self) -> (Self, Option<Self>) {
        let (first_producer, second_producer) = self.producer.split();
        let access = self.access;
        let first = JoinProducer {
            producer: first_producer,
            access,
        };
        let second = second_producer.map(|producer| JoinProducer { producer, access });
        (first, second)
    }

    fn fold_with<F>(self, folder: F) -> F
    where
        F: Folder<Self::Item>,
    {
        let JoinProducer { producer, access } = self;
        let iter = producer.0.map(|idx| unsafe { J::get(access, idx) });
        folder.consume_iter(iter)
    }
}

macro_rules! define_join {
    ($first:ident $(, $rest:ident)*) => {
        impl<$first, $($rest),*> Join for ($first, $($rest),*)
        where
            $first: Join,
            $($rest: Join,)*
            (<$first as Join>::Mask, $(<$rest as Join>::Mask),*): BitAnd,
        {
            type Item = ($first::Item, $($rest::Item),*);
            type Access = ($first::Access, $($rest::Access),*);
            type Mask = <(<$first as Join>::Mask, $(<$rest as Join>::Mask),*) as BitAnd>::Value;

            #[allow(non_snake_case)]
            fn open(self) -> (Self::Mask, Self::Access, JoinConstraint) {
                let ($first, $($rest),*) = self;
                let ($first, $($rest),*) = ($first.open(), $($rest.open()),*);

                let mask = ($first.0, $($rest.0),*).and();
                let access = ($first.1, $($rest.1),*);
                let constraint = $first.2$(.and($rest.2))*;
                (mask, access, constraint)
            }

            #[allow(non_snake_case)]
            unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
                let ($first, $($rest),*) = access;
                ($first::get($first, index), $($rest::get($rest, index)),*)
            }
        }
    };
}

define_join! {A}
define_join! {A, B}
define_join! {A, B, C}
define_join! {A, B, C, D}
define_join! {A, B, C, D, E}
define_join! {A, B, C, D, E, F}
define_join! {A, B, C, D, E, F, G}
define_join! {A, B, C, D, E, F, G, H}
define_join! {A, B, C, D, E, F, G, H, I}
define_join! {A, B, C, D, E, F, G, H, I, J}
define_join! {A, B, C, D, E, F, G, H, I, J, K}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y}
define_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z}

pub trait BitAnd {
    type Value: BitSetLike;

    fn and(self) -> Self::Value;
}

macro_rules! define_bit_and {
    ($first:ident, $($rest:ident),+ $(,)?) => {
        impl<$first, $($rest),*> BitAnd for ($first, $($rest),*)
        where
            $first: BitSetLike,
            $($rest: BitSetLike),*
        {
            type Value = BitSetAnd<$first, <($($rest,)*) as BitAnd>::Value>;

            #[allow(non_snake_case)]
            fn and(self) -> Self::Value {
                let ($first, $($rest),*) = self;
                BitSetAnd($first, ($($rest,)*).and())
            }
        }
    };

    ($first:ident $(,)?) => {
        impl<$first> BitAnd for ($first,)
        where
            $first: BitSetLike,
        {
            type Value = $first;

            fn and(self) -> Self::Value {
                self.0
            }
        }
    };
}

define_bit_and! {A}
define_bit_and! {A, B}
define_bit_and! {A, B, C}
define_bit_and! {A, B, C, D}
define_bit_and! {A, B, C, D, E}
define_bit_and! {A, B, C, D, E, F}
define_bit_and! {A, B, C, D, E, F, G}
define_bit_and! {A, B, C, D, E, F, G, H}
define_bit_and! {A, B, C, D, E, F, G, H, I}
define_bit_and! {A, B, C, D, E, F, G, H, I, J}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y}
define_bit_and! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z}
