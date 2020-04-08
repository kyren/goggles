# goggles #

*It's like a nice pair of specs, except much more uncomfortable and inconvenient*

---

This crate is a heavily modified, stripped down version of the `specs` ECS
library.  It is less of a framework for doing a specific style of ECS as easily
as possible and more of a DIY library for doing ECS that will require you to
adapt it to your own needs.

It is also my personal ECS library for my own projects so may be somewhat
opinionated.  If something like `specs` or `legion` already works for you then
please continue to use it.

---

The basic data structure design is nearly exactly the same as `specs`, it uses
the same basic data structures that `specs` does to store components and do
joins.  Just like `specs`, it stores components in separate storages and records
their presence with a hierarchal bitset type from `hibitset`, and uses that same
hierarchal bitset to do joins.

On top of this, however, is a more minimal, piecemeal API than even `specs`
provides.  It removes everything that I feel is extraneous or magical, and only
tries to handle what is very *hard* to do otherwise or is unsafe, and mostly
leaves the easier parts to the user to design themselves.

The library contains a set of more or less independent parts, you can stop at
whatever level of abstraction is appropriate:

1) The `par_seq` module is a completely independent way of setting up and
   running generic parallel and sequential systems.  It can be compared to
   `shred`, but it contains a much more generic `System` trait, and does not
   include functionality for locking and reading resources.  It simply allows
   you to define a set of resources in `System`s, combine those systems using
   `Par` and `Seq`, make sure that the result doesn't have resource conflicts,
   then run it.

2) The `system_data` module defines a `SystemData` trait for statically defined
   resources for the `par_seq` module, and provides a `SystemData`
   implementation for tuples of `SystemData`.

3) The `resource_set` module defines a `ResourceSet` which is similar to an
   `AnyMap` with values stored in a `RwLock`.  It doesn't ever block, instead it
   simply panics when aliasing rules are violated.  It is designed so that you
   can use the `par_seq` module to build systems that operate over the defined
   resources.  It also includes convenient types for defining and requesting
   read / write handles to resources which implement `SystemData`, so they can
   be used in tuples like `(Read<ResourceA>, Write<ResourceB>)`.  It is very
   similar to the `World` type in `shred`.
   
4) The `join` module contains a definition for a `Join` trait for data stored by
   `u32` indexes and tracked by a `BitSet`.  It provides the ability to iterate
   over a `Join` sequentially and in parallel, and provides means to join
   multiple `Join` instances together.  It is similar to the `Join` trait in
   `specs`, but redesigned for a bit more safety.
   
5) The `component` module contains the `Component` and `RawStorage` traits, as
   well as the 3 most useful storage types: `VecStorage`, `DenseVecStorage`, and
   `HashMapStorage`.  It is extremely similar to the equivalent functionality in
   `specs`.

6) The `tracked` module contains a `RawStorage` wrapper that keeps track of
   component changes.  Unlike `specs`, this is pretty minimal and only
   optionally sets a flag in an `AtomicBitSet` on mutable access.

7) The `masked` module contains the `MaskedStorage` struct which safely wraps a
   `RawStorage` together with a `BitSet` to keep track of which components are
    present.  `MaskedStorage` is also safely join-able.

8) The `entity` module contains an atomic generational index allocator that also
   uses `hibitset` types to track which indexes are alive.  It also allows you
   to join on the allocator to output live `Entity`'s.
   
9) The `world` module ties everything together into something with a
   recognizable ECS API.  If you want to understand how this works, or want to
   build your own abstractions instead of what's provided in this module, start
   here.  Many of the changes to `specs` have been made so that the `world`
   module contains only safe code.

---

Here is an incomplete list of the important things removed from `shred`
and `specs`:

1) No automatic setup, you must insert / register all resource and component types.
2) No saving / loading support, you should handle this yourself.
3) No automatic system scheduling / dispatching / batching, you *must* design
   your execution strategy yourself with `par` and `seq` and then it can easily
   be checked for conflicts at startup.
4) No lazy updates, you probably want to handle this specifically for your
   application.  There is not always a universally best way to do this.

And here is an incomplete list of the important changes from `specs` for
functionality that is present in both this library and `specs`:

1) Does not require T: Sync for mutable access to resources / components (only
   requires T: Send).
2) Redesigned Join trait for soundness and a bit more safety.  There is an
   additional `IntoJoin` trait that allows you to participate in the convenient
   tuple join syntax without having to write unsafe code.
3) Component `RawStorage` impls require `UnsafeCell` for soundness
4) Simplified component modification tracking.
5) Removes some features of `specs` which are known to be unsound such as index
   component access through iterators.
6) The individual parts of the library go out of their way to be more loosely
   coupled, sometimes at the cost of extra code or user convenience.
7) Nearly all of the internals are public in case you need to build a different
   abstraction.

---

## Why does this exist?

I'd been working on a project for a while that used `specs` proper, and I kept
needing to redesign small pieces of it.  Much to `specs` credit, it is already a
library that you can use in a very piecemeal, independent way, and that's
exactly what I did.  At some point I looked up and realized that I only used
maybe a core 20% of what `specs` provided, and that was around the time that I
started needing to use messy extension traits to go further.  I decided to
re-implement the core part of `specs` that I still used, package it up with some
of the other things I made that were more flexible than what `shred` / `specs`
offered, and put it here.

## Should I use this?

There's a good chance you don't need or want this crate.  This is going to be an
opinionated, personal, minified "fork" of `specs`, and I may not even release it
on crates.io.

Still, if you find yourself needing to break up `specs` into its constituent
parts and build your own APIs on top of it, this is here if you need it.  Reach
out to me if this is useful to you and you'd like to see it on crates.io.

## Credit

This project is directly derived from `specs` so most of the credit goes to
`specs`' creators.  Anything at all in this crate that is especially clever is
probably theirs.

## License

This is derived from `specs`, so similarly it is dual-licensed under Apache-2.0
/ MIT.
