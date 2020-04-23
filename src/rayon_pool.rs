use crate::par_seq::Pool;

/// A system runner that runs parallel systems using `rayon::join`.
#[derive(Default)]
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
