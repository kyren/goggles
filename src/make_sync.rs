/// Turns any type that implements `Send` into one that also implements `Sync` by only allowing
/// mutable access if the inner type is not `Sync`.
pub struct MakeSync<T>(T);

impl<T> MakeSync<T> {
    pub fn new(t: T) -> Self {
        MakeSync(t)
    }

    pub fn into_inner(self) -> T {
        self.0
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Sync> MakeSync<T> {
    pub fn get(&self) -> &T {
        &self.0
    }
}

// Safe because if `T` is `!Sync` then *ONLY* mutable access is allowed to the inner type, and
// therefore any access of the inner type is unique and cannot cause data races.
//
// If you send a `&MakeSync<T>` to another thread for some T: !Sync, then the reference you send is
// completely inert, you cannot access the inner T at all.
//
// Note that we rely on the automatic implementation of `Send` for `MakeSync<T>` which requires `T`
// to be `Send` in order to send a `&mut MakeSync<T>` to another thread.
unsafe impl<T> Sync for MakeSync<T> {}
