use hibitset::{
    AtomicBitSet, BitIter, BitProducer, BitSet, BitSetAll, BitSetAnd, BitSetLike, BitSetNot,
    BitSetOr, BitSetXor,
};
use rayon::iter::{
    plumbing::{bridge_unindexed, Folder, UnindexedConsumer, UnindexedProducer},
    ParallelIterator,
};
use thiserror::Error;

pub type Index = u32;

pub trait Join {
    type Item;
    type Access;
    type Mask: BitSetLike;

    fn open(self) -> (Self::Mask, Self::Access);

    /// Get a value out of the access type returned from `open`.
    ///
    /// MUST be called only with indexes which are present in the mask returned along with the
    /// access value from `open`.
    ///
    /// You must *only* allow one `Self::Item` for a given index to be alive at any given time.  It
    /// is allowed for a `Join` impl to have `Item` be a mutable reference that would alias if `get`
    /// were called multiple times on the same index.
    ///
    /// A simpler, more restrictive version of this rule that all of the uses of `Join` impls
    /// currently follow is that `Join::get` may only be called once per index for a given `Access`
    /// object.
    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item;
}

pub trait IntoJoin {
    type Item;
    type IntoJoin: Join<Item = Self::Item>;

    fn into_join(self) -> Self::IntoJoin;
}

impl<J: Join> IntoJoin for J {
    type Item = J::Item;
    type IntoJoin = J;

    fn into_join(self) -> Self::IntoJoin {
        self
    }
}

#[derive(Debug, Error)]
#[error("cannot iterate over unconstrained Join")]
pub struct JoinIterUnconstrained;

pub trait IntoJoinExt: IntoJoin {
    fn join(self) -> JoinIter<Self::IntoJoin>
    where
        Self: Sized,
        <Self::IntoJoin as Join>::Mask: BitSetConstrained,
    {
        JoinIter::new(self.into_join()).unwrap()
    }

    fn join_unconstrained(self) -> JoinIter<Self::IntoJoin>
    where
        Self: Sized,
    {
        JoinIter::new_unconstrained(self.into_join())
    }

    fn par_join(self) -> JoinParIter<Self::IntoJoin>
    where
        Self: Sized + Send + Sync,
        Self::Item: Send,
        <Self::IntoJoin as Join>::Mask: BitSetConstrained + Send + Sync,
    {
        JoinParIter::new(self.into_join()).unwrap()
    }

    fn par_join_unconstrained(self) -> JoinParIter<Self::IntoJoin>
    where
        Self: Sized + Send + Sync,
        Self::Item: Send,
        <Self::IntoJoin as Join>::Mask: Send + Sync,
    {
        JoinParIter::new_unconstrained(self.into_join())
    }

    fn maybe(self) -> MaybeJoin<Self::IntoJoin>
    where
        Self: Sized,
    {
        MaybeJoin(self.into_join())
    }
}

impl<J: IntoJoin> IntoJoinExt for J {}

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
        // Aliasing requirements must be upheld by the caller, but we ensure that no invalid index
        // is passed to our inner `Join`.
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
        // `JoinIter` only implements `Iterator`, so we only call `J::get` *once* for each index
        // that is returned from `BitIter`.  Since `BitIter` iterates over the correct mask and ond
        // does not return repeat indexes, our requirements are upheld.
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
        // All of the indexes here are ultimately derived from the mask returned by J::open, so we
        // know they are valid.  Each `JoinProducer` has a *distinct* subset of the valid indexes,
        // and we only fold over each index that this `JoinProducer` owns *once*, so we uphold the
        // aliasing requirements.
        folder.consume_iter(producer.0.map(|idx| unsafe { J::get(access, idx) }))
    }
}

/// If the inner type is a tuple of types which implement `Join`, then this type will implement
/// `Join` all of them.
pub struct JoinTuple<T>(T);

macro_rules! define_join {
    ($first:ident $(, $rest:ident)*) => {
        impl<$first, $($rest),*> Join for JoinTuple<($first, $($rest),*)>
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
                let ($first, $($rest),*) = self.0;
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

macro_rules! define_into_join {
    ($first:ident $(, $rest:ident)*) => {
        impl<$first, $($rest),*> IntoJoin for ($first, $($rest),*)
        where
            $first: IntoJoin,
            $($rest: IntoJoin,)*
        {
            type Item = ($first::Item, $($rest::Item),*);
            type IntoJoin = JoinTuple<(<$first as IntoJoin>::IntoJoin, $(<$rest as IntoJoin>::IntoJoin),*)>;

            #[allow(non_snake_case)]
            fn into_join(self) -> Self::IntoJoin {
                let ($first, $($rest),*) = self;
                JoinTuple(($first.into_join(), $($rest.into_join()),*))
            }
        }
    };
}

define_into_join! {A}
define_into_join! {A, B}
define_into_join! {A, B, C}
define_into_join! {A, B, C, D}
define_into_join! {A, B, C, D, E}
define_into_join! {A, B, C, D, E, F}
define_into_join! {A, B, C, D, E, F, G}
define_into_join! {A, B, C, D, E, F, G, H}
define_into_join! {A, B, C, D, E, F, G, H, I}
define_into_join! {A, B, C, D, E, F, G, H, I, J}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y}
define_into_join! {A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z}

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
