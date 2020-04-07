use std::{
    iter,
    num::NonZeroI32,
    sync::atomic::{AtomicU32, Ordering},
    u32,
};

use hibitset::{AtomicBitSet, BitSet, BitSetLike, BitSetOr};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("Entity is no longer alive or has a mismatched generation")]
pub struct WrongGeneration;

pub type Index = u32;
const MAX_INDEX: Index = u32::MAX;
type AtomicIndex = AtomicU32;

pub type GenId = i32;
type NZGenId = NonZeroI32;

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct Generation(GenId);

impl Generation {
    pub fn new() -> Allocator {
        Allocator::default()
    }

    /// Generations start at the dead generation of zero.
    pub fn zero() -> Generation {
        Generation(0)
    }

    /// The first live generation.
    pub fn one() -> Generation {
        Generation(1)
    }

    #[inline]
    pub fn id(self) -> GenId {
        self.0
    }

    /// A generation is alive if its ID is > 0
    #[inline]
    pub fn is_alive(self) -> bool {
        self.id() > 0
    }

    /// If this generation is alive, returns the 'killed' version of this generation, otherwise just
    /// returns the current dead generation.
    ///
    /// The 'killed' version of a generation has an ID which is the negation of its current live ID.
    #[inline]
    pub fn killed(self) -> Generation {
        if self.is_alive() {
            Generation(-self.id())
        } else {
            self
        }
    }

    /// If this generation is dead, returns the 'raised' version of this generation, otherwise just
    /// returns the current live generation.
    ///
    /// The 'raised' version of a generation has an ID which is the negation of its current dead ID
    /// (so the positive verison of its dead ID) + 1.
    #[inline]
    pub fn raised(self) -> Generation {
        if !self.is_alive() {
            Generation(
                (1 as GenId)
                    .checked_sub(self.id())
                    .expect("generation overflow"),
            )
        } else {
            self
        }
    }
}

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
    // The generation of an entity cannot be <= 0, so we use a NonZero type here to enable layout
    // optimizations.
    generation: NZGenId,
}

impl Entity {
    /// The low-valued `index` of the Entity.
    #[inline]
    pub fn index(self) -> Index {
        self.index
    }

    /// The entity's generation.
    ///
    /// `Entity` values always contain generations for which `Generation::is_alive` returns true
    /// (they have ID > 0).
    #[inline]
    pub fn generation(self) -> Generation {
        Generation(self.generation.get())
    }

    fn new(index: Index, generation: Generation) -> Entity {
        debug_assert!(generation.is_alive());
        Entity {
            index,
            generation: NZGenId::new(generation.0).unwrap(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Allocator {
    generations: Vec<Generation>,
    alive: BitSet,
    raised_atomic: AtomicBitSet,
    killed_atomic: AtomicBitSet,
    cache: EntityCache,
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

        if !self.raised_atomic.remove(entity.index) {
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

    /// Returns the generation for this `Index`.
    ///
    /// All indexes start as the dead 'zero' generation.
    #[inline]
    pub fn generation(&self, index: Index) -> Generation {
        self.generations
            .get(index as usize)
            .copied()
            .unwrap_or(Generation::zero())
    }

    /// *If* the given index has a live entity associated with it, returns that live `Entity`.
    #[inline]
    pub fn entity(&self, index: Index) -> Option<Entity> {
        let generation = self.generation(index);
        if !generation.is_alive() && self.raised_atomic.contains(index) {
            Some(Entity::new(index, generation.raised()))
        } else if generation.is_alive() {
            Some(Entity::new(index, generation))
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
            debug_assert!(self.generations.len() <= index_len as usize);
            self.generations
                .resize_with(index_len as usize, Default::default);
            index
        });

        self.alive.add(index);

        let generation = &mut self.generations[index as usize];
        debug_assert!(!generation.is_alive());
        *generation = generation.raised();

        Entity::new(index, *generation)
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
    pub fn live_bitset(&self) -> BitSetOr<&BitSet, &AtomicBitSet> {
        BitSetOr(&self.alive, &self.raised_atomic)
    }

    /// Merge all atomic operations done since the last call to `Allocator::merge_atomic`.
    ///
    /// Atomically allocated entities become merged into the faster non-atomic BitSet, and entities
    /// marked for deletion with `Allocator::kill_atomic` actually become killed.
    ///
    /// Takes a `&mut Vec<Entity>` parameter which will be filled with newly killed entities.
    pub fn merge_atomic(&mut self, killed: &mut Vec<Entity>) {
        killed.clear();

        let index_len = *self.index_len.get_mut();
        debug_assert!(self.generations.len() <= index_len as usize);
        self.generations
            .resize_with(index_len as usize, Default::default);

        for index in (&self.raised_atomic).iter() {
            let generation = &mut self.generations[index as usize];
            debug_assert!(!generation.is_alive());
            *generation = generation.raised();
            self.alive.add(index);
        }
        self.raised_atomic.clear();

        for index in (&self.killed_atomic).iter() {
            self.alive.remove(index);
            let generation = &mut self.generations[index as usize];
            debug_assert!(generation.is_alive());
            killed.push(Entity::new(index, *generation));
            *generation = generation.killed();
        }
        self.killed_atomic.clear();

        self.cache.extend(killed.iter().map(|e| e.index));
    }
}

#[derive(Default, Debug)]
struct EntityCache {
    cache: Vec<Index>,
    len: AtomicIndex,
}

impl EntityCache {
    fn push(&mut self, index: Index) {
        self.extend(iter::once(index))
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

/// Increments `i` atomically without wrapping on overflow.  Resembles a `fetch_add(1,
/// Ordering::Relaxed)` with checked overflow, returning `None` instead.
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

/// Increments `i` atomically without wrapping on overflow.  Resembles a `fetch_sub(1,
/// Ordering::Relaxed)` with checked underflow, returning `None` instead.
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
