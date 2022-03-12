use std::{any::type_name, mem};

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
    R: Resources,
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

pub struct ParList<S>(pub Vec<S>);

impl<A, S> System<A> for ParList<S>
where
    A: Copy + Send,
    S: System<A> + Send,
    S::Pool: Sync,
    S::Error: Send,
{
    type Resources = S::Resources;
    type Pool = S::Pool;
    type Error = S::Error;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        let mut r = S::Resources::default();
        for s in &self.0 {
            let sr = s.check_resources()?;
            if sr.conflicts_with(&r) {
                return Err(ResourceConflict {
                    type_name: type_name::<Self>(),
                });
            }
            r.union(&sr);
        }
        Ok(r)
    }

    fn run(&mut self, pool: &Self::Pool, args: A) -> Result<(), Self::Error> {
        fn run<A, S>(s: &mut [S], pool: &S::Pool, args: A) -> Result<(), S::Error>
        where
            A: Copy + Send,
            S: System<A> + Send,
            S::Pool: Sync,
            S::Error: Send,
        {
            if s.len() == 0 {
                Ok(())
            } else if s.len() == 1 {
                s[0].run(pool, args)
            } else {
                let mid = s.len() / 2;
                let (lo, hi) = s.split_at_mut(mid);
                match pool.join(move || run(lo, pool, args), move || run(hi, pool, args)) {
                    (Ok(()), Ok(())) => Ok(()),
                    (Err(a), Ok(())) => Err(a),
                    (Ok(()), Err(b)) => Err(b),
                    (Err(a), Err(b)) => Err(a.combine(b)),
                }
            }
        }

        run(&mut self.0[..], pool, args)
    }
}

pub struct SeqList<S>(pub Vec<S>);

impl<A, S: System<A>> System<A> for SeqList<S>
where
    A: Copy,
    S: System<A>,
{
    type Resources = S::Resources;
    type Pool = S::Pool;
    type Error = S::Error;

    fn check_resources(&self) -> Result<Self::Resources, ResourceConflict> {
        let mut r = S::Resources::default();
        for s in &self.0 {
            r.union(&s.check_resources()?);
        }
        Ok(r)
    }

    fn run(&mut self, pool: &Self::Pool, args: A) -> Result<(), Self::Error> {
        for s in &mut self.0 {
            s.run(pool, args)?;
        }
        Ok(())
    }
}

/// Takes a list of systems all of the same type and makes them as parallel as possible without
/// conflicts and without changing the overall system order.
///
/// The parallelization is done eagerly and in order, the method tries to insert systems in order to
/// run in parallel until a resource conflict is detected, then runs the systems determined not to
/// conflict in parallel with each other and in sequence with the remaining systems.  The algorithm
/// then repeats this process with the remaining systems until there are no more systems remaining.
pub fn auto_schedule<A, S>(
    systems: impl IntoIterator<Item = S>,
) -> Result<
    impl System<A, Resources = S::Resources, Pool = S::Pool, Error = S::Error>,
    ResourceConflict,
>
where
    A: Copy + Send + 'static,
    S: System<A> + Send + 'static,
    S::Pool: Sync,
    S::Error: Send,
{
    let mut seq = Vec::new();

    let mut par = Vec::new();
    let mut par_resources = S::Resources::default();

    for system in systems {
        let sys_resources = system.check_resources()?;
        if sys_resources.conflicts_with(&par_resources) {
            assert!(par.len() != 0);
            seq.push(ParList(mem::take(&mut par)));
            par_resources = S::Resources::default();
        }

        par_resources.union(&sys_resources);
        par.push(system);
    }

    if !par.is_empty() {
        seq.push(ParList(par));
    }

    Ok(SeqList(seq))
}

/// A basic system runner that runs all systems sequentially in the current thread.
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
