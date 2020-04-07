# Goggles #

*like a nice pair of specs, but much more uncomfortable and inconvenient*

## A heavily minified version of specs, exactly as much as you need and no more.

### THIS IS CURRENTLY VERY WIP ###

This is a heavily modified, minified version of the 'specs' ECS library.  It is
less of a framework for doing a specific style of ECS as easily as possible and
more of a DIY library for doing ECS that will require you to adapt it to your
own needs.

It is also my personal ECS library for my own projects so may be somewhat
opinionated.  If something like `specs` or `legion` already works for you then
please continue to use it.

---

The basic data structure design is very, very close to specs, it uses the same
basic data structures that specs does to store components and do joins.  Just
like specs, it stores components in separate storages and records their presense
with a sparse bitset type from `hibitset`, and uses that same sparse bitset to
do joins.

On top of this, howeer, is a much more minimal, piecemeal API than even specs
provides.  It removes everything that I feel is extraneous or magical, and only
tries to handle what is very *hard* to do otherwise or is unsafe, and leaves the
easier parts to the user to design themselves.

The library contains a set of more or less independent parts:

1) The `par_seq` module is a completely independent way of setting up and
   running generic parallel and sequential systems.  It can be compared to
   `shred`, but it contains a much more generic `System` trait, and does not
   include functionality for locking and reading resources.  It simply allows
   you to define a set of resources in `System`s and ensure that they don't
   conflict.

2) The `resource_set` module defines a `ResourceSet` which is similar to an
   `AnyMap` with values stored in a `RwLock`.  It doesn't ever block, instead it
   simply panics when aliasing rules are violated.  It is desigend so that you
   can use the `par_seq` module to build systems that operate over the defined
   resources.  It also includes convenient types for defining and requesting
   sets of read / write handles to resources in tuples like `(Read<ResourceA>,
   Write<ResourceB>)`.  It is very similar to the `World` type in `shred`.
   
3) The `join` module contains a definition for a `Join` trait for data stored by
   `u32` indexes and tracked by a `BitSet`.  It provides the ability to iterate
   over a `Join` sequentially and in parallel, and provides means to join
   multiple `Join` instances together.  It is similar to the `Join` trait in
   specs, but redesigned and simplified for safety and soundness.
   
4) The `component` module contains the `RawStorage` trait and `MaskedStorage`
   structs, similarly to `specs`.  The `RawStorage` trait provides unsafe
   component storage based on `u32` indexes that must be paired with a `BitSet`
   to keep track of what components are present and what are not.  The
   `MaskedStorage` provides this pairing and is a safe interface to a
   `RawStorage` paired with a `BitSet`.  `MaskedStorage` is also joinable, and
   it depends on the `join` module for this.

5) The `entity` module contains an atomic generational index allocator that also
   uses `hibitset` sparse BitSet types to track which indexes are alive.
   
6) The `world` module ties the `resource_set`, `join`, `component`, and `entity`
   modules together into something that can be used to store resources and
   components, and has a recognizable `Entity` based API.

7) The `flagged` module contains a `RawStorage` wrapper that keeps track of
   component changes.  Unlike `specs`, this is extremely minimal and only sets a
   flag in a `BitSet` on mutable access, and contains a few convenience methods
   to update component values without accidentally triggering mutation.  It also
   adds some methods to the types in `component` and `world`.

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
2) Redesigned, simplified, sound Join trait.
3) Redesigned, simplified component modification tracking.
4) Removes some features of specs which are known to be unsound such as index
   component access through iterators.
5) The individual parts of the library go out of their way to be more loosely
   coupled, at the cost of convenience.
5) More of the internals are public in case you need to build a different
   abstration.
