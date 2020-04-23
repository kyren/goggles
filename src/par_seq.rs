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
/// This trait is designed so that systems may read or write to resources inside the `source`
/// parameter.  Systems report the resources they intend to use abstractly through the `Resources`
/// type, and this provides the ability to check parallel systems for resource conflicts.
//
// TODO: It would be much nicer if our `System` trait could be this:
//
// ```
// pub trait System<'a> {
//     type Resources: Resources;
//     type Pool: Pool;
//     type Args: ?Sized + 'a;
//     type Error: Error;
//
//     fn check_resources(&self) -> Result<Self::Resources, ResourceConflict>;
//
//     fn run(
//         &mut self,
//         pool: &Self::Pool,
//         args: &Self::Args,
//     ) -> Result<(), Self::Error>;
// }
// ```
//
// This would allow dropping the `Source` associated type and would be much more general, allowing
// you to pass arbitrary non-'static arguments as parameters to `System::run` if your systems
// implement `for<'a> System<'a>`.

// However, when we implement this `System` trait for `Par` and `Seq` and try to use this with more
// than a few systems, we unfortunately run into quadratic or exponential Rust compiler behavior
// (Maybe this bug: https://github.com/rust-lang/rust/issues/69671).
//
// When this issue is fixed this trait should be changed.
pub trait System {
    type Source: ?Sized;
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
        source: &Self::Source,
        args: &Self::Args,
    ) -> Result<(), Self::Error>;
}

impl<S> System for Box<S>
where
    S: ?Sized + System,
{
    type Source = S::Source;
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
        source: &Self::Source,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        (**self).run(pool, source, args)
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

impl<H, T, S, R, P, A, E> System for Par<H, T>
where
    H: System<Source = S, Resources = R, Pool = P, Args = A, Error = E> + Send,
    T: System<Source = S, Resources = R, Pool = P, Args = A, Error = E> + Send,
    S: ?Sized + Sync,
    R: Resources + Send,
    P: Pool + Sync,
    A: ?Sized + Sync,
    E: Error + Send,
{
    type Source = S;
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
        source: &Self::Source,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        let Self { head, tail, .. } = self;
        match pool.join(
            move || head.run(pool, source, args),
            move || tail.run(pool, source, args),
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

impl<H, T, S, R, P, A, E> System for Seq<H, T>
where
    H: System<Source = S, Resources = R, Pool = P, Args = A, Error = E>,
    T: System<Source = S, Resources = R, Pool = P, Args = A, Error = E>,
    S: ?Sized,
    R: Resources,
    P: Pool,
    A: ?Sized,
    E: Error,
{
    type Source = S;
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
        source: &Self::Source,
        args: &Self::Args,
    ) -> Result<(), Self::Error> {
        self.head.run(pool, source, args)?;
        self.tail.run(pool, source, args)
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
