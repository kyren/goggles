use hibitset::{
    AtomicBitSet, BitIter, BitProducer, BitSet, BitSetAll, BitSetAnd, BitSetLike, BitSetNot,
    BitSetOr, BitSetXor,
};
use rayon::iter::{
    plumbing::{bridge_unindexed, Folder, UnindexedConsumer, UnindexedProducer},
    ParallelIterator,
};
use thiserror::Error;

use crate::entity::Index;

pub trait Join {
    type Item;
    type Access;
    type Mask: BitSetLike;

    fn open(self) -> (Self::Mask, Self::Access);
    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item;
}

#[derive(Debug, Error)]
#[error("cannot iterate over unconstrained Join")]
pub struct JoinIterUnconstrained;

pub trait JoinExt: Join {
    fn join(self) -> JoinIter<Self>
    where
        Self: Sized,
        Self::Mask: BitSetConstrained,
    {
        JoinIter::new(self).unwrap()
    }

    fn join_unconstrained(self) -> JoinIter<Self>
    where
        Self: Sized,
    {
        JoinIter::new_unconstrained(self)
    }

    fn par_join(self) -> JoinParIter<Self>
    where
        Self: Sized + Send,
        Self::Item: Send,
        Self::Access: Send + Sync,
        Self::Mask: BitSetConstrained + Send + Sync,
    {
        JoinParIter::new(self).unwrap()
    }

    fn par_join_unconstrained(self) -> JoinParIter<Self>
    where
        Self: Sized + Send,
        Self::Item: Send,
        Self::Access: Send + Sync,
        Self::Mask: Send + Sync,
    {
        JoinParIter::new_unconstrained(self)
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

    fn open(self) -> (Self::Mask, Self::Access) {
        let (mask, access) = self.0.open();
        (BitSetAll, (mask, access))
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
    pub fn new(j: J) -> Result<Self, JoinIterUnconstrained>
    where
        J::Mask: BitSetConstrained,
    {
        let (mask, access) = j.open();
        if mask.is_constrained() {
            Ok(Self(mask.iter(), access))
        } else {
            Err(JoinIterUnconstrained)
        }
    }

    pub fn new_unconstrained(j: J) -> Self {
        let (mask, access) = j.open();
        Self(mask.iter(), access)
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
    pub fn new(j: J) -> Result<Self, JoinIterUnconstrained>
    where
        J::Mask: BitSetConstrained,
    {
        let (mask, access) = j.open();
        if mask.is_constrained() {
            Ok(Self(mask, access))
        } else {
            Err(JoinIterUnconstrained)
        }
    }

    pub fn new_unconstrained(j: J) -> Self {
        let (mask, access) = j.open();
        Self(mask, access)
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
            fn open(self) -> (Self::Mask, Self::Access) {
                let ($first, $($rest),*) = self;
                let ($first, $($rest),*) = ($first.open(), $($rest.open()),*);

                let mask = ($first.0, $($rest.0),*).and();
                let access = ($first.1, $($rest.1),*);
                (mask, access)
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

macro_rules! define_bit_join {
    (impl <$($lifetime:lifetime)? $(,)? $($arg:ident),*> for $bitset:ty) => {
        impl<$($lifetime,)* $($arg),*> Join for $bitset
            where $($arg: BitSetLike),*
        {
            type Item = Index;
            type Access = ();
            type Mask = Self;

            fn open(self) -> (Self::Mask, Self::Access) {
                (self, ())
            }

            unsafe fn get(_: &Self::Access, index: Index) -> Self::Item {
                index
            }
        }
    }
}

define_bit_join!(impl<> for BitSet);
define_bit_join!(impl<'a> for &'a BitSet);
define_bit_join!(impl<> for AtomicBitSet);
define_bit_join!(impl<'a> for &'a AtomicBitSet);
define_bit_join!(impl<> for BitSetAll);
define_bit_join!(impl<'a> for &'a BitSetAll);
define_bit_join!(impl<A> for BitSetNot<A>);
define_bit_join!(impl<'a, A> for &'a BitSetNot<A>);
define_bit_join!(impl<A, B> for BitSetAnd<A, B>);
define_bit_join!(impl<'a, A, B> for &'a BitSetAnd<A, B>);
define_bit_join!(impl<A, B> for BitSetOr<A, B>);
define_bit_join!(impl<'a, A, B> for &'a BitSetOr<A, B>);
define_bit_join!(impl<A, B> for BitSetXor<A, B>);
define_bit_join!(impl<'a> for &'a dyn BitSetLike);

pub trait BitSetConstrained: BitSetLike {
    fn is_constrained(&self) -> bool;
}

impl<'a, B: BitSetConstrained> BitSetConstrained for &'a B {
    fn is_constrained(&self) -> bool {
        (*self).is_constrained()
    }
}

macro_rules! define_bit_constrained {
    ($bitset:ty) => {
        impl BitSetConstrained for $bitset {
            fn is_constrained(&self) -> bool {
                true
            }
        }
    };
}

define_bit_constrained!(BitSet);
define_bit_constrained!(AtomicBitSet);

impl BitSetConstrained for BitSetAll {
    fn is_constrained(&self) -> bool {
        false
    }
}

impl<A: BitSetConstrained> BitSetConstrained for BitSetNot<A> {
    fn is_constrained(&self) -> bool {
        !self.0.is_constrained()
    }
}

impl<A, B> BitSetConstrained for BitSetAnd<A, B>
where
    A: BitSetConstrained,
    B: BitSetConstrained,
{
    fn is_constrained(&self) -> bool {
        self.0.is_constrained() || self.1.is_constrained()
    }
}

impl<A, B> BitSetConstrained for BitSetOr<A, B>
where
    A: BitSetConstrained,
    B: BitSetConstrained,
{
    fn is_constrained(&self) -> bool {
        self.0.is_constrained() && self.1.is_constrained()
    }
}

impl<A, B> BitSetConstrained for BitSetXor<A, B>
where
    A: BitSetConstrained,
    B: BitSetConstrained,
{
    fn is_constrained(&self) -> bool {
        self.0.is_constrained() && self.1.is_constrained()
    }
}
