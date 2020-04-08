use std::{any::type_name, collections::HashSet, hash::Hash};

use thiserror::Error;

/// Trait for identifying resources that are used in a in a `System`
pub trait Resources: Default {
    /// Union this set of resources with the given set of resources.
    fn union(&mut self, other: &Self);
    /// Return true if any resource in this set may not be used in parallel with any resource in the
    /// other set.
    fn conflicts_with(&self, other: &Self) -> bool;
}

/// Trait for the (possibly parallel) runner for a `System`.
pub trait Pool {
    /// Should run the two functions (potentially in parallel) and return their results.
    fn join<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send;
}

/// Trait for error types returned from `System::run`.
///
/// Errors must be combinable because systems may be run in parallel, and thus may result in
/// multiple errors before stopping.
pub trait Error {
    fn combine(self, other: Self) -> Self;
}

#[derive(Debug, Error)]
#[error("resource conflict in {type_name:?}")]
pub struct ResourceConflict {
    pub type_name: &'static str,
}

/// A system that may be run in parallel or in sequence with other such systems in a group.
///
/// This trait is designed so that systems may read or write to resources inside the `world`
/// parameter.  Systems report the resources they intend to use abstractly through the `Resources`
/// type, and this provides the ability to check parallel systems for resource conflicts.
pub trait System {
    type World: ?Sized;
    type Resources: Resources;
    type Pool: Pool;
    type Args: ?Sized;
    type Error: Error;

    /// Check for any internal resource conficts and if there are none, return a `Resources` that
    /// represents the used resources.
    ///
    /// Must be a constant value, this will generally only be called once.
    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict>;

    fn run(
        &mut self,
        pool: &Self::Pool,
        world: &Self::World,
        args: &Self::Args,
    ) -> Result<(), Self::Error>;
}

impl<S> System for Box<S>
where
    S: ?Sized + System,
{
    type World = S::World;
    type Resources = S::Resources;
    type Pool = S::Pool;
    type Args = S::Args;
    type Error = S::Error;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        (**self).check_resources()
    }

    fn run(
        &mut self,
        pool: &Self::Pool,
        world: &Self::World,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        (**self).run(pool, world, args)
    }
}

pub struct Par<H, T> {
    head: H,
    tail: T,
}

impl<H, T> Par<H, T> {
    pub fn new(head: H, tail: T) -> Par<H, T> {
        Par { head, tail }
    }

    pub fn with<S>(self, sys: S) -> Par<Par<H, T>, S> {
        Par {
            head: self,
            tail: sys,
        }
    }
}

impl<H, T, W, R, P, A, E> System for Par<H, T>
where
    H: System<World = W, Resources = R, Pool = P, Args = A, Error = E> + Send,
    T: System<World = W, Resources = R, Pool = P, Args = A, Error = E> + Send,
    W: ?Sized + Sync,
    R: Resources + Send,
    P: Pool + Sync,
    A: ?Sized + Sync,
    E: Error + Send,
{
    type World = W;
    type Resources = R;
    type Pool = P;
    type Args = A;
    type Error = E;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        let hr = self.head.check_resources()?;
        let tr = self.tail.check_resources()?;
        if hr.conflicts_with(&tr) {
            Err(ResourceConflict {
                type_name: type_name::<Self>(),
            })
        } else {
            let mut resources = hr;
            resources.union(&tr);
            Ok(resources)
        }
    }

    fn run(
        &mut self,
        pool: &Self::Pool,
        world: &Self::World,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        let Self { head, tail, .. } = self;
        match pool.join(
            move || head.run(pool, world, args),
            move || tail.run(pool, world, args),
        ) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(a), Ok(())) => Err(a),
            (Ok(()), Err(b)) => Err(b),
            (Err(a), Err(b)) => Err(a.combine(b)),
        }
    }
}

#[macro_export]
macro_rules! par {
    ($head:expr, $tail:expr $(, $rest:expr)* $(,)?) => {
        {
            $crate::par_seq::Par::new($head, $tail)
                $(.with($rest))*
        }
    };
}

pub struct Seq<H, T> {
    head: H,
    tail: T,
}

impl<H, T> Seq<H, T> {
    pub fn new(head: H, tail: T) -> Seq<H, T> {
        Seq { head, tail }
    }

    pub fn with<S>(self, sys: S) -> Seq<Seq<H, T>, S> {
        Seq {
            head: self,
            tail: sys,
        }
    }
}

impl<H, T, W, R, P, A, E> System for Seq<H, T>
where
    H: System<World = W, Resources = R, Pool = P, Args = A, Error = E>,
    T: System<World = W, Resources = R, Pool = P, Args = A, Error = E>,
    W: ?Sized,
    R: Resources,
    P: Pool,
    A: ?Sized,
    E: Error,
{
    type World = W;
    type Resources = R;
    type Pool = P;
    type Args = A;
    type Error = E;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        let mut r = self.head.check_resources()?;
        r.union(&self.tail.check_resources()?);
        Ok(r)
    }

    fn run(
        &mut self,
        pool: &Self::Pool,
        world: &Self::World,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        self.head.run(pool, world, args)?;
        self.tail.run(pool, world, args)
    }
}

#[macro_export]
macro_rules! seq {
    ($head:expr, $tail:expr $(, $rest:expr)* $(,)?) => {
        {
            $crate::par_seq::Seq::new($head, $tail)
                $( .with($rest) )*
        }
    };
}

/// A `Resources` implementation that describes R/W locks.
///
/// Two read locks for the same resource do not conflict, but a read and a write or two writes to
/// the same resource do.
pub struct RwResources<R> {
    reads: HashSet<R>,
    writes: HashSet<R>,
}

impl<R> Default for RwResources<R>
where
    R: Eq + Hash,
{
    fn default() -> Self {
        RwResources {
            reads: HashSet::new(),
            writes: HashSet::new(),
        }
    }
}

impl<R> RwResources<R>
where
    R: Eq + Hash,
{
    pub fn new() -> Self {
        Default::default()
    }

    pub fn from_iters(
        reads: impl IntoIterator<Item = R>,
        writes: impl IntoIterator<Item = R>,
    ) -> Self {
        let writes: HashSet<R> = writes.into_iter().collect();
        let reads: HashSet<R> = reads.into_iter().filter(|r| !writes.contains(r)).collect();
        RwResources { reads, writes }
    }

    pub fn add_read(&mut self, r: R) {
        if !self.writes.contains(&r) {
            self.reads.insert(r);
        }
    }

    pub fn add_write(&mut self, r: R) {
        self.reads.remove(&r);
        self.writes.insert(r);
    }

    pub fn read(mut self, r: R) -> Self {
        self.add_read(r);
        self
    }

    pub fn write(mut self, r: R) -> Self {
        self.add_write(r);
        self
    }
}

impl<R: Eq + Hash + Clone> Resources for RwResources<R> {
    fn union(&mut self, other: &Self) {
        for w in &other.writes {
            self.writes.insert(w.clone());
        }

        for r in &other.reads {
            if !self.writes.contains(r) {
                self.reads.insert(r.clone());
            }
        }
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        self.writes.intersection(&other.reads).next().is_some()
            || self.writes.intersection(&other.writes).next().is_some()
            || other.writes.intersection(&self.reads).next().is_some()
            || other.writes.intersection(&self.writes).next().is_some()
    }
}

/// A system runner that runs parallel systems single-threaded in the current thread.
#[derive(Default)]
pub struct SeqPool;

impl Pool for SeqPool {
    fn join<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        let ra = a();
        let rb = b();
        (ra, rb)
    }
}

/// A system runner that runs parallel systems using `rayon::join`.
pub struct RayonPool;

impl Pool for RayonPool {
    fn join<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        rayon::join(a, b)
    }
}
