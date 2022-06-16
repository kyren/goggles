use std::{
    iter,
    num::NonZeroI32,
    sync::atomic::{AtomicU32, Ordering},
    u32,
};

use hibitset::{AtomicBitSet, BitSet, BitSetLike, BitSetOr};
use thiserror::Error;

use crate::join::{Index, Join};

#[derive(Debug, Error)]
#[error("Entity is no longer alive or has a mismatched generation")]
pub struct WrongGeneration;

/// Entities are unqiue "generational indexes" with low-valued `index` values that are appropriate
/// as indexes into contiguous arrays.
///
/// In order to make sure every `Entity` is unique, allocating an `Entity` with the same index will
/// result in an incremented `generation` field.
///
/// No two entities will share the same `index` and `generation`, so every created `Entity` is unique.
#[derive(Clone, Copy, Debug, Hash, Eq, Ord, PartialEq, PartialOrd)]
pub struct Entity {
    index: Index,
    generation: AliveGeneration,
}

impl Entity {
    /// The low-valued `index` of the Entity.
    #[inline]
    pub fn index(self) -> Index {
        self.index
    }

    /// The entity's generation.
    ///
    /// This will never be zero.
    #[inline]
    pub fn generation(self) -> u32 {
        self.generation.id() as u32
    }

    fn new(index: Index, generation: AliveGeneration) -> Entity {
        Entity { index, generation }
    }
}

pub type LiveBitSet<'a> = BitSetOr<&'a BitSet, &'a AtomicBitSet>;

#[derive(Debug, Default)]
pub struct Allocator {
    generations: Vec<Generation>,
    alive: BitSet,
    raised_atomic: AtomicBitSet,
    killed_atomic: AtomicBitSet,
    cache: EntityCache,
    // The maximum ever allocated index + 1.  If there are no outstanding atomic operations, the
    // `generations` vector should be equal to this length.
    index_len: AtomicIndex,
}

impl Allocator {
    pub fn new() -> Allocator {
        Allocator::default()
    }

    /// Kill the given entity.
    ///
    /// Will return `Err(WrongGeneration)` if the given entity is not the current generation in this
    /// allcoator.
    #[inline]
    pub fn kill(&mut self, entity: Entity) -> Result<(), WrongGeneration> {
        if !self.is_alive(entity) {
            return Err(WrongGeneration);
        }

        self.alive.remove(entity.index);
        self.killed_atomic.remove(entity.index);

        if self.raised_atomic.remove(entity.index) {
            // If this entity is alive atomically and we're killing it non-atomically, we must commit
            // the entity as having been added then killed so it can properly go into the cache.
            self.update_generation_length();
            let generation = &mut self.generations[entity.index as usize];
            debug_assert!(!generation.is_alive());
            *generation = generation.raised().generation().killed();
        } else {
            let generation = &mut self.generations[entity.index as usize];
            debug_assert!(generation.is_alive());
            *generation = generation.killed();
        }

        self.cache.push(entity.index);

        Ok(())
    }

    /// Mark an entity to be killed on the next call to `Allocator::merge_atomic`.
    ///
    /// The entity's state is not changed at all until the next call to `Allocator::merge_atomic`,
    /// it is still considered live and may even have `Allocator::kill_atomic` called on it multiple
    /// times.
    ///
    /// If the entity is not current at the time of this call, however, then this will return
    /// `Err(WrongGeneration)`.
    #[inline]
    pub fn kill_atomic(&self, e: Entity) -> Result<(), WrongGeneration> {
        if !self.is_alive(e) {
            return Err(WrongGeneration);
        }

        self.killed_atomic.add_atomic(e.index());
        Ok(())
    }

    /// Returns whether the given entity has not been killed, and is thus the current generation for
    /// this allocator.
    ///
    /// More specifically, it checks whether the generation for the given entity is the current
    /// alive generation for that index.  This generally means that the generation is too old
    /// because the `Allocator` allocated it and then later killed it, but it may also happen if
    /// `Entity`s are improperly mixed between `Allocator` instances and this entity has a newer
    /// generation than the current live one for that index.
    #[inline]
    pub fn is_alive(&self, e: Entity) -> bool {
        self.entity(e.index()) == Some(e)
    }

    /// *If* the given index has a live entity associated with it, returns that live `Entity`.
    #[inline]
    pub fn entity(&self, index: Index) -> Option<Entity> {
        let generation = self.generation(index);
        if let Some(alive) = generation.to_alive() {
            Some(Entity::new(index, alive))
        } else if self.raised_atomic.contains(index) {
            Some(Entity::new(index, generation.raised()))
        } else {
            None
        }
    }

    /// Allocate a new unique Entity.
    #[inline]
    pub fn allocate(&mut self) -> Entity {
        let index = self.cache.pop().unwrap_or_else(|| {
            let index = *self.index_len.get_mut();
            let index_len = index.checked_add(1).expect("no entity left to allocate");
            *self.index_len.get_mut() = index_len;
            self.update_generation_length();
            index
        });

        self.alive.add(index);

        let generation = &mut self.generations[index as usize];
        let raised = generation.raised();
        *generation = raised.generation();
        Entity::new(index, raised)
    }

    /// Allocate an entity atomically.
    ///
    /// Atomically allocated entities are immediately valid, live entities indistinguishable from
    /// non-atomically allocated entities.
    ///
    /// The only observable difference is that the query performance of atomically allocated
    /// entities may be slightly worse until `merge_atomic` is called, at which point they will be
    /// merged into the same data structure that keeps track of regular live entities.
    #[inline]
    pub fn allocate_atomic(&self) -> Entity {
        let index = self.cache.pop_atomic().unwrap_or_else(|| {
            atomic_increment(&self.index_len).expect("no entity left to allocate")
        });

        self.raised_atomic.add_atomic(index);
        Entity::new(index, self.generation(index).raised())
    }

    /// Returns a `BitSetLike` for all live entities.
    ///
    /// This is a `BitSetOr` of the non-atomically live entities and the atomically live entities.
    #[inline]
    pub fn live_bitset(&self) -> LiveBitSet {
        BitSetOr(&self.alive, &self.raised_atomic)
    }

    /// Returns the maximum ever allocated entity index + 1.
    ///
    /// Since finding the actual live entity count is costly, this is a very cheap way of finding
    /// out the approximate maximum number of entities ever allocated.
    #[inline]
    pub fn max_entity_count(&self) -> Index {
        self.index_len.load(Ordering::Relaxed)
    }

    /// Merge all atomic operations done since the last call to `Allocator::merge_atomic`.
    ///
    /// Atomically allocated entities become merged into the faster non-atomic BitSet, and entities
    /// marked for deletion with `Allocator::kill_atomic` actually become killed.
    ///
    /// Takes a `&mut Vec<Entity>` parameter which will be cleared and filled with newly killed
    /// entities.
    pub fn merge_atomic(&mut self, killed: &mut Vec<Entity>) {
        killed.clear();

        self.update_generation_length();

        for index in (&self.raised_atomic).iter() {
            let generation = &mut self.generations[index as usize];
            *generation = generation.raised().generation();
            self.alive.add(index);
        }
        self.raised_atomic.clear();

        for index in (&self.killed_atomic).iter() {
            self.alive.remove(index);
            let generation = &mut self.generations[index as usize];
            killed.push(Entity::new(index, generation.to_alive().unwrap()));
            *generation = generation.killed();
        }
        self.killed_atomic.clear();

        self.cache.extend(killed.iter().map(|e| e.index));
    }

    fn generation(&self, index: Index) -> Generation {
        self.generations
            .get(index as usize)
            .copied()
            .unwrap_or(Generation::zero())
    }

    // Commit the changes to the length of the generation vector from the atomically adjusted index
    // length.
    fn update_generation_length(&mut self) {
        let index_len = *self.index_len.get_mut() as usize;
        if self.generations.len() < index_len {
            self.generations.resize_with(index_len, Default::default);
        }
    }
}

impl<'a> Join for &'a Allocator {
    type Item = Entity;
    type Access = &'a Allocator;
    type Mask = LiveBitSet<'a>;

    fn open(self) -> (Self::Mask, Self::Access) {
        (self.live_bitset(), self)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        Entity::new(index, access.generation(index).raised())
    }
}

#[derive(Default, Debug)]
struct EntityCache {
    cache: Vec<Index>,
    len: AtomicIndex,
}

impl EntityCache {
    fn push(&mut self, index: Index) {
        self.extend(iter::once(index));
    }

    fn pop(&mut self) -> Option<Index> {
        self.maintain();
        let x = self.cache.pop();
        *self.len.get_mut() = self.cache.len() as Index;
        x
    }

    fn pop_atomic(&self) -> Option<Index> {
        atomic_decrement(&self.len).map(|x| self.cache[(x - 1) as usize])
    }

    fn maintain(&mut self) {
        self.cache.truncate(*self.len.get_mut() as usize);
    }
}

impl Extend<Index> for EntityCache {
    fn extend<T: IntoIterator<Item = Index>>(&mut self, iter: T) {
        self.maintain();
        self.cache.extend(iter);
        *self.len.get_mut() = self.cache.len() as Index;
    }
}

const MAX_INDEX: Index = u32::MAX;
type AtomicIndex = AtomicU32;

type GenId = i32;
type NZGenId = NonZeroI32;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
struct Generation(GenId);

impl Generation {
    // Generations start at the dead generation of zero.
    fn zero() -> Generation {
        Generation(0)
    }

    fn id(self) -> GenId {
        self.0
    }

    // A generation is alive if its ID is > 0
    fn is_alive(self) -> bool {
        self.0 > 0
    }

    fn to_alive(self) -> Option<AliveGeneration> {
        if self.0 > 0 {
            Some(AliveGeneration(unsafe { NZGenId::new_unchecked(self.0) }))
        } else {
            None
        }
    }

    // If this generation is alive, returns the 'killed' version of this generation, otherwise just
    // returns the current dead generation.
    //
    // The 'killed' version of a generation has an ID which is the negation of its current live ID.
    fn killed(self) -> Generation {
        if self.is_alive() {
            Generation(-self.id())
        } else {
            self
        }
    }

    // If this generation is dead, returns the 'raised' version of this generation, otherwise just
    // returns the current live generation.
    //
    // The 'raised' version of a generation has an ID which is the negation of its current dead ID
    // (so the positive verison of its dead ID) + 1.
    fn raised(self) -> AliveGeneration {
        if self.0 > 0 {
            AliveGeneration(unsafe { NZGenId::new_unchecked(self.0) })
        } else {
            let id = (1 as GenId)
                .checked_sub(self.id())
                .expect("generation overflow");
            AliveGeneration(unsafe { NZGenId::new_unchecked(id) })
        }
    }
}

// A generation that is guaranteed to be alive.
//
// Since the generation id cannot be 0, this can use `NZGenId` and enable layout optimizations.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
struct AliveGeneration(NZGenId);

impl AliveGeneration {
    fn id(self) -> GenId {
        self.0.get()
    }

    fn generation(self) -> Generation {
        Generation(self.0.get())
    }
}

// Increments `i` atomically without wrapping on overflow.
//
// Resembles a `fetch_add(1, Ordering::Relaxed)` with checked overflow, returning `None` instead.
fn atomic_increment(i: &AtomicIndex) -> Option<Index> {
    let mut prev = i.load(Ordering::Relaxed);
    while prev != MAX_INDEX {
        match i.compare_exchange_weak(prev, prev + 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(x) => return Some(x),
            Err(next_prev) => prev = next_prev,
        }
    }
    None
}

// Decrements `i` atomically without wrapping on underflow.
//
// Resembles a `fetch_sub(1, Ordering::Relaxed)` with checked underflow, returning `None` instead.
fn atomic_decrement(i: &AtomicIndex) -> Option<Index> {
    let mut prev = i.load(Ordering::Relaxed);
    while prev != 0 {
        match i.compare_exchange_weak(prev, prev - 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(x) => return Some(x),
            Err(next_prev) => prev = next_prev,
        }
    }
    None
}
