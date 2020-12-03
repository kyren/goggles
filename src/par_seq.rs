use std::any::type_name;

use crate::resources::{ResourceConflict, Resources};

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

/// A system that may be run in parallel or in sequence with other such systems in a group.
///
/// This trait is designed so that systems may read or write to resources inside the `args`
/// parameter.  Systems report the resources they intend to use abstractly through the `Resources`
/// type, and this provides the ability to check parallel systems for resource conflicts.
pub trait System<Args> {
    type Resources: Resources;
    type Pool: Pool;
    type Error: Error;

    /// Check for any internal resource conficts and if there are none, return a `Resources` that
    /// represents the used resources.
    ///
    /// Must be a constant value, this will generally only be called once.
    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict>;

    fn run(&mut self, pool: &Self::Pool, args: Args) -> Result<(), Self::Error>;
}

impl<A, S> System<A> for Box<S>
where
    S: ?Sized + System<A>,
{
    type Resources = S::Resources;
    type Pool = S::Pool;
    type Error = S::Error;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        (**self).check_resources()
    }

    fn run(&mut self, pool: &Self::Pool, args: A) -> Result<(), Self::Error> {
        (**self).run(pool, args)
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

    pub fn with<S>(self, sys: S) -> Par<H, Par<T, S>> {
        Par {
            head: self.head,
            tail: Par::new(self.tail, sys),
        }
    }
}

impl<H, T, A, R, P, E> System<A> for Par<H, T>
where
    H: System<A, Resources = R, Pool = P, Error = E> + Send,
    T: System<A, Resources = R, Pool = P, Error = E> + Send,
    A: Copy + Send,
    R: Resources + Send,
    P: Pool + Sync,
    E: Error + Send,
{
    type Resources = R;
    type Pool = P;
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

    fn run(&mut self, pool: &Self::Pool, args: A) -> Result<(), Self::Error> {
        let Self { head, tail, .. } = self;
        match pool.join(move || head.run(pool, args), move || tail.run(pool, args)) {
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

    pub fn with<S>(self, sys: S) -> Seq<H, Seq<T, S>> {
        Seq {
            head: self.head,
            tail: Seq::new(self.tail, sys),
        }
    }
}

impl<H, T, A, R, P, E> System<A> for Seq<H, T>
where
    H: System<A, Resources = R, Pool = P, Error = E>,
    T: System<A, Resources = R, Pool = P, Error = E>,
    A: Copy,
    R: Resources,
    P: Pool,
    E: Error,
{
    type Resources = R;
    type Pool = P;
    type Error = E;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        let mut r = self.head.check_resources()?;
        r.union(&self.tail.check_resources()?);
        Ok(r)
    }

    fn run(&mut self, pool: &Self::Pool, args: A) -> Result<(), Self::Error> {
        self.head.run(pool, args)?;
        self.tail.run(pool, args)
    }
}

#[macro_export]
macro_rules! seq {
    ($head:expr, $tail:expr $(, $rest:expr)* $(,)?) => {
        {
            $crate::par_seq::Seq::new($head, $tail)
                $(.with($rest))*
        }
    };
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
