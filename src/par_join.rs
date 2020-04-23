use hibitset::{BitProducer, BitSetLike};
use rayon::iter::{
    plumbing::{bridge_unindexed, Folder, UnindexedConsumer, UnindexedProducer},
    ParallelIterator,
};

pub use crate::join::{BitSetConstrained, Index, IntoJoin, Join, JoinIterUnconstrained};

pub trait ParJoinExt: IntoJoin {
    /// Safely iterate over this `Join` in parallel.
    ///
    /// # Panics
    /// Panics if the result of this join is unconstrained.
    fn par_join(self) -> JoinParIter<Self::IntoJoin>
    where
        Self: Sized + Send + Sync,
        Self::Item: Send,
        <Self::IntoJoin as Join>::Mask: BitSetConstrained + Send + Sync,
    {
        JoinParIter::new(self.into_join()).unwrap()
    }

    /// Safely iterate over this `Join` in parallel, and don't panic if it is unconstrained.
    ///
    /// Constraint detection is not perfect, so this is here if it is in your way.
    fn par_join_unconstrained(self) -> JoinParIter<Self::IntoJoin>
    where
        Self: Sized + Send + Sync,
        Self::Item: Send,
        <Self::IntoJoin as Join>::Mask: Send + Sync,
    {
        JoinParIter::new_unconstrained(self.into_join())
    }
}

impl<J: IntoJoin> ParJoinExt for J {}

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
