use std::{
    any::{Any, TypeId},
    collections::HashMap,
    iter,
    ops::{Deref, DerefMut},
};

use atomic_refcell::{AtomicRef, AtomicRefCell, AtomicRefMut};

use crate::{
    component::{Component, MaskedStorage},
    entity::{Allocator, Entity, Index, LiveBitSet, WrongGeneration},
    join::IntoJoin,
    par_seq::{ResourceConflict, RwResources},
    resource_set::ResourceSet,
    system_data::SystemData,
};

#[derive(Default)]
pub struct World {
    entities: AtomicRefCell<Entities>,
    resources: ResourceSet,
    components: ResourceSet,
    remove_components: HashMap<TypeId, Box<dyn Fn(&ResourceSet, &[Entity]) + Send>>,
}

impl World {
    pub fn new() -> Self {
        World {
            entities: AtomicRefCell::default(),
            resources: ResourceSet::new(),
            components: ResourceSet::new(),
            remove_components: HashMap::new(),
        }
    }

    pub fn read_entities(&self) -> ReadEntities {
        EntitiesAccess(self.entities.borrow())
    }

    pub fn write_entities(&self) -> WriteEntities {
        EntitiesAccess(self.entities.borrow_mut())
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

    pub fn read_resource<R>(&self) -> ReadResource<R>
    where
        R: Any + Send + Sync + 'static,
    {
        ResourceAccess(self.resources.borrow::<R>())
    }

    pub fn write_resource<R>(&self) -> WriteResource<R>
    where
        R: Any + Send + 'static,
    {
        ResourceAccess(self.resources.borrow_mut::<R>())
    }

    pub fn insert_component<C>(&mut self) -> Option<MaskedStorage<C>>
    where
        C: Component + Any + 'static,
        C::Storage: Default + Send,
    {
        self.remove_components.insert(
            TypeId::of::<C>(),
            Box::new(|resource_set, entities| {
                let mut storage = resource_set.borrow_mut::<MaskedStorage<C>>();
                for e in entities {
                    storage.remove(e.index());
                }
            }),
        );
        self.components.insert(MaskedStorage::<C>::default())
    }

    pub fn remove_component<C>(&mut self) -> Option<MaskedStorage<C>>
    where
        C: Component + Any + 'static,
        C::Storage: Default + Send,
    {
        self.remove_components.remove(&TypeId::of::<C>());
        self.components.remove::<MaskedStorage<C>>()
    }

    fn read_component<C>(&self) -> ReadComponent<C>
    where
        C: Component + Any + 'static,
        C::Storage: Send + Sync,
    {
        ComponentAccess {
            storage: self.components.borrow(),
            entities: self.entities.borrow(),
        }
    }

    fn write_component<C>(&self) -> WriteComponent<C>
    where
        C: Component + Any + 'static,
        C::Storage: Send,
    {
        ComponentAccess {
            storage: self.components.borrow_mut(),
            entities: self.entities.borrow(),
        }
    }

    pub fn fetch<'a, S>(&'a self) -> S
    where
        S: SystemData<'a, Source = World, Resources = RwResources<WorldResourceId>>,
    {
        S::fetch(self)
    }
}

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

    pub fn allocate_atomic(&self) -> Entity {
        self.0.allocate_atomic()
    }

    pub fn live_bitset(&self) -> LiveBitSet {
        self.0.live_bitset()
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ComponentId(TypeId);

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum WorldResourceId {
    Entities,
    Resource(ResourceId),
    Component(ComponentId),
}

pub struct EntitiesAccess<R>(R);

impl<R> Deref for EntitiesAccess<R>
where
    R: Deref<Target = Entities>,
{
    type Target = Entities;

    fn deref(&self) -> &Entities {
        &*self.0
    }
}

impl<R> DerefMut for EntitiesAccess<R>
where
    R: DerefMut<Target = Entities>,
{
    fn deref_mut(&mut self) -> &mut Entities {
        &mut *self.0
    }
}

impl<'a, R> IntoJoin for &'a EntitiesAccess<R>
where
    R: Deref<Target = Entities>,
{
    type Item = Entity;
    type IntoJoin = &'a Allocator;

    fn into_join(self) -> Self::IntoJoin {
        &(self.0).0
    }
}

pub type ReadEntities<'a> = EntitiesAccess<AtomicRef<'a, Entities>>;

impl<'a> SystemData<'a> for ReadEntities<'a> {
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(WorldResourceId::Entities),
            iter::empty(),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_entities()
    }
}

pub type WriteEntities<'a> = EntitiesAccess<AtomicRefMut<'a, Entities>>;

impl<'a> SystemData<'a> for WriteEntities<'a> {
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::empty(),
            iter::once(WorldResourceId::Entities),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_entities()
    }
}

pub struct ResourceAccess<R>(R);

impl<R> Deref for ResourceAccess<R>
where
    R: Deref,
{
    type Target = R::Target;

    fn deref(&self) -> &R::Target {
        &*self.0
    }
}

impl<R> DerefMut for ResourceAccess<R>
where
    R: DerefMut,
{
    fn deref_mut(&mut self) -> &mut R::Target {
        &mut *self.0
    }
}

pub type ReadResource<'a, R> = ResourceAccess<AtomicRef<'a, R>>;

impl<'a, R> SystemData<'a> for ReadResource<'a, R>
where
    R: Any + Send + Sync + 'static,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(WorldResourceId::Resource(ResourceId(TypeId::of::<R>()))),
            iter::empty(),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_resource()
    }
}

pub type WriteResource<'a, R> = ResourceAccess<AtomicRefMut<'a, R>>;

impl<'a, R> SystemData<'a> for WriteResource<'a, R>
where
    R: Any + Send + 'static,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::empty(),
            iter::once(WorldResourceId::Resource(ResourceId(TypeId::of::<R>()))),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_resource()
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

impl<'a, C, R> IntoJoin for &'a ComponentAccess<'a, C, R>
where
    C: Component,
    R: Deref<Target = MaskedStorage<C>> + 'a,
{
    type Item = &'a C;
    type IntoJoin = &'a MaskedStorage<C>;

    fn into_join(self) -> Self::IntoJoin {
        (&*self.storage).into_join()
    }
}

impl<'a, C, R> IntoJoin for &'a mut ComponentAccess<'a, C, R>
where
    C: Component,
    R: DerefMut<Target = MaskedStorage<C>> + 'a,
{
    type Item = &'a mut C;
    type IntoJoin = &'a mut MaskedStorage<C>;

    fn into_join(self) -> Self::IntoJoin {
        (&mut *self.storage).into_join()
    }
}

pub type ReadComponent<'a, C> = ComponentAccess<'a, C, AtomicRef<'a, MaskedStorage<C>>>;

impl<'a, C> SystemData<'a> for ReadComponent<'a, C>
where
    C: Component + Any + Send + Sync + 'static,
    C::Storage: Send + Sync,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            [
                WorldResourceId::Component(ComponentId(TypeId::of::<C>())),
                WorldResourceId::Entities,
            ]
            .iter()
            .copied(),
            iter::empty(),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_component()
    }
}

pub type WriteComponent<'a, C> = ComponentAccess<'a, C, AtomicRefMut<'a, MaskedStorage<C>>>;

impl<'a, C> SystemData<'a> for WriteComponent<'a, C>
where
    C: Component + Any + Send + 'static,
    C::Storage: Send,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::from_iters(
            iter::once(WorldResourceId::Entities),
            iter::once(WorldResourceId::Component(ComponentId(TypeId::of::<C>()))),
        ))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_component()
    }
}
