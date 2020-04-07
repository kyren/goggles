use std::{
    any::Any,
    ops::{Deref, DerefMut},
};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};
use hibitset::{AtomicBitSet, BitSet, BitSetOr};

use crate::{
    component::{Component, MaskedStorage},
    entity::{Allocator, Entity, Index, WrongGeneration},
    join::Join,
    resource_set::ResourceSet,
};

#[derive(Default)]
pub struct Entities(Allocator);

impl Entities {
    pub fn kill_atomic(&self, e: Entity) -> Result<(), WrongGeneration> {
        self.0.kill_atomic(e)
    }

    pub fn is_alive(&self, e: Entity) -> bool {
        self.0.is_alive(e)
    }

    pub fn entity(&self, index: Index) -> Option<Entity> {
        self.0.entity(index)
    }

    pub fn allocate(&mut self) -> Entity {
        self.0.allocate()
    }

    pub fn allocate_atomoic(&self) -> Entity {
        self.0.allocate_atomic()
    }

    pub fn live_bitset(&self) -> BitSetOr<&BitSet, &AtomicBitSet> {
        self.0.live_bitset()
    }
}

pub struct ComponentAccess<'a, C, R>
where
    C: Component,
    R: Deref<Target = MaskedStorage<C>>,
{
    storage: R,
    entities: AtomicRef<'a, Entities>,
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    R: Deref<Target = MaskedStorage<C>>,
{
    pub fn entities(&self) -> &Entities {
        &self.entities
    }

    pub fn storage(&self) -> &MaskedStorage<C> {
        &self.storage
    }

    pub fn contains(&self, e: Entity) -> bool {
        self.entities.is_alive(e) && self.storage.contains(e.index())
    }

    pub fn get(&self, e: Entity) -> Option<&C> {
        if self.entities.is_alive(e) {
            self.storage.get(e.index())
        } else {
            None
        }
    }
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    R: DerefMut<Target = MaskedStorage<C>>,
{
    pub fn storage_mut(&mut self) -> &mut MaskedStorage<C> {
        &mut self.storage
    }

    pub fn get_mut(&mut self, e: Entity) -> Option<&mut C> {
        if self.entities.is_alive(e) {
            self.storage.get_mut(e.index())
        } else {
            None
        }
    }

    pub fn insert(&mut self, e: Entity, c: C) -> Result<Option<C>, WrongGeneration> {
        if self.entities.is_alive(e) {
            Ok(self.storage.insert(e.index(), c))
        } else {
            Err(WrongGeneration)
        }
    }

    pub fn remove(&mut self, e: Entity) -> Result<Option<C>, WrongGeneration> {
        if self.entities.is_alive(e) {
            Ok(self.storage.remove(e.index()))
        } else {
            Err(WrongGeneration)
        }
    }
}

impl<'a, C, R> Join for &'a ComponentAccess<'a, C, R>
where
    C: Component,
    R: Deref<Target = MaskedStorage<C>> + 'a,
{
    type Item = <&'a MaskedStorage<C> as Join>::Item;
    type Access = <&'a MaskedStorage<C> as Join>::Access;
    type Mask = <&'a MaskedStorage<C> as Join>::Mask;

    fn open(self) -> (Self::Mask, Self::Access) {
        <&MaskedStorage<C>>::open(&self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        <&MaskedStorage<C>>::get(access, index)
    }
}

impl<'a, C, R> Join for &'a mut ComponentAccess<'a, C, R>
where
    C: Component,
    R: DerefMut<Target = MaskedStorage<C>> + 'a,
{
    type Item = <&'a mut MaskedStorage<C> as Join>::Item;
    type Access = <&'a mut MaskedStorage<C> as Join>::Access;
    type Mask = <&'a mut MaskedStorage<C> as Join>::Mask;

    fn open(self) -> (Self::Mask, Self::Access) {
        <&mut MaskedStorage<C>>::open(&mut self.storage)
    }

    unsafe fn get(access: &Self::Access, index: Index) -> Self::Item {
        <&mut MaskedStorage<C>>::get(access, index)
    }
}

#[derive(Default)]
pub struct World {
    entities: AtomicRefCell<Entities>,
    resources: ResourceSet,
    components: ResourceSet,
    remove_components: Vec<Box<dyn Fn(&ResourceSet, &[Entity]) + Send>>,
}

impl World {
    pub fn new() -> Self {
        World {
            entities: AtomicRefCell::default(),
            resources: ResourceSet::new(),
            components: ResourceSet::new(),
            remove_components: Vec::new(),
        }
    }

    pub fn borrow_entities(&self) -> AtomicRef<Entities> {
        self.entities.borrow()
    }

    pub fn borrow_entities_mut(&self) -> AtomicRefMut<Entities> {
        self.entities.borrow_mut()
    }

    pub fn insert_resource<R>(&mut self, r: R) -> Option<R>
    where
        R: Send + 'static,
    {
        self.resources.insert(r)
    }

    pub fn remove_resource<R>(&mut self) -> Option<R>
    where
        R: Send + 'static,
    {
        self.resources.remove::<R>()
    }

    pub fn borrow_resource<T>(&self) -> AtomicRef<T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.resources.borrow::<T>()
    }

    pub fn borrow_resource_mut<T>(&self) -> AtomicRefMut<T>
    where
        T: Any + Send + 'static,
    {
        self.resources.borrow_mut::<T>()
    }

    pub fn register_component<C>(&mut self)
    where
        C: Component,
        C::Storage: Default + Send,
    {
        if !self.components.contains::<MaskedStorage<C>>() {
            self.components.insert(MaskedStorage::<C>::default());
            self.remove_components
                .push(Box::new(|resource_set, entities| {
                    let mut storage = resource_set.borrow_mut::<MaskedStorage<C>>();
                    for e in entities {
                        storage.remove(e.index());
                    }
                }));
        }
    }
}
