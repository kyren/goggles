use std::{
    any::TypeId,
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use atomic_refcell::{AtomicRef, AtomicRefMut};
use hibitset::AtomicBitSet;

use crate::{
    component::Component,
    entity::{Allocator, Entity, LiveBitSet, WrongGeneration},
    join::{Index, IntoJoin},
    masked::{GuardedJoin, MaskedStorage},
    par_seq::{ResourceConflict, RwResources},
    resource_set::ResourceSet,
    system_data::SystemData,
    tracked::TrackedStorage,
};

#[derive(Default)]
pub struct World {
    allocator: Allocator,
    resources: ResourceSet,
    components: ResourceSet,
    remove_components: HashMap<TypeId, Box<dyn Fn(&ResourceSet, &[Entity]) + Send + Sync>>,
    killed: Vec<Entity>,
}

impl World {
    pub fn new() -> Self {
        World {
            allocator: Allocator::new(),
            resources: ResourceSet::new(),
            components: ResourceSet::new(),
            remove_components: HashMap::new(),
            killed: Vec::new(),
        }
    }

    pub fn entities(&self) -> Entities {
        Entities(&self.allocator)
    }

    pub fn create_entity_atomic(&self) -> Entity {
        self.allocator.allocate_atomic()
    }

    pub fn create_entity(&mut self) -> Entity {
        self.allocator.allocate()
    }

    pub fn delete_entity(&mut self, e: Entity) -> Result<(), WrongGeneration> {
        self.allocator.kill(e)?;
        for remove_component in self.remove_components.values() {
            remove_component(&self.components, &[e]);
        }
        Ok(())
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
        R: Send + Sync + 'static,
    {
        ResourceAccess(self.resources.borrow())
    }

    pub fn write_resource<R>(&self) -> WriteResource<R>
    where
        R: Send + 'static,
    {
        ResourceAccess(self.resources.borrow_mut())
    }

    pub fn get_resource_mut<R>(&mut self) -> &mut R
    where
        R: 'static,
    {
        self.resources.get_mut()
    }

    pub fn insert_component<C>(&mut self) -> Option<MaskedStorage<C>>
    where
        C: Component + 'static,
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
        C: Component + 'static,
        C::Storage: Default + Send,
    {
        self.remove_components.remove(&TypeId::of::<C>());
        self.components.remove::<MaskedStorage<C>>()
    }

    pub fn read_component<C>(&self) -> ReadComponent<C>
    where
        C: Component + 'static,
        C::Storage: Send + Sync,
    {
        ComponentAccess {
            storage: self.components.borrow(),
            entities: self.entities(),
        }
    }

    pub fn write_component<C>(&self) -> WriteComponent<C>
    where
        C: Component + 'static,
        C::Storage: Send,
    {
        ComponentAccess {
            storage: self.components.borrow_mut(),
            entities: self.entities(),
        }
    }

    pub fn get_component_mut<C>(&mut self) -> ComponentAccess<C, &mut MaskedStorage<C>>
    where
        C: Component + 'static,
    {
        ComponentAccess {
            storage: self.components.get_mut(),
            entities: Entities(&self.allocator),
        }
    }

    pub fn fetch<'a, S>(&'a self) -> S
    where
        S: SystemData<'a, Source = World, Resources = RwResources<WorldResourceId>>,
    {
        S::fetch(self)
    }

    /// Merge any pending atomic entity operations.
    ///
    /// Merges atomically allocated entities into the normal entity `BitSet` for performance, and
    /// finalizes any entities that were requested to be deleted.
    ///
    /// No entity is actually removed until this method is called.
    pub fn merge_atomic(&mut self) {
        self.allocator.merge_atomic(&mut self.killed);
        for remove_component in self.remove_components.values() {
            remove_component(&self.components, &self.killed);
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ResourceId(TypeId);

impl ResourceId {
    pub fn of<C: 'static>() -> ResourceId {
        ResourceId(TypeId::of::<C>())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ComponentId(TypeId);

impl ComponentId {
    pub fn of<C: Component + 'static>() -> ComponentId {
        ComponentId(TypeId::of::<C>())
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum WorldResourceId {
    Entities,
    Resource(ResourceId),
    Component(ComponentId),
}

pub type WorldResources = RwResources<WorldResourceId>;

pub struct Entities<'a>(&'a Allocator);

impl<'a> Entities<'a> {
    /// Atomically request that this entity be removed on the next call to `World::merge_atomic`.
    ///
    /// An entity is not deleted until `World::merge_atomic` is called, so it will still be 'alive'
    /// and show up in queries until that time.
    pub fn delete(&self, e: Entity) -> Result<(), WrongGeneration> {
        self.0.kill_atomic(e)
    }

    pub fn is_alive(&self, e: Entity) -> bool {
        self.0.is_alive(e)
    }

    pub fn entity(&self, index: Index) -> Option<Entity> {
        self.0.entity(index)
    }

    /// Atomically allocate an entity.  An atomically allocated entity is indistinguishable from a
    /// regular live entity, but when `World::merge_atomic` is called it will be merged into a
    /// non-atomic `BitSet` for performance.
    pub fn create(&self) -> Entity {
        self.0.allocate_atomic()
    }

    pub fn live_bitset(&self) -> LiveBitSet {
        self.0.live_bitset()
    }
}

impl<'a> IntoJoin for &'a Entities<'a> {
    type Item = Entity;
    type IntoJoin = &'a Allocator;

    fn into_join(self) -> Self::IntoJoin {
        &*self.0
    }
}

impl<'a> SystemData<'a> for Entities<'a> {
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new().read(WorldResourceId::Entities))
    }

    fn fetch(world: &'a World) -> Self {
        world.entities()
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
    R: Send + Sync + 'static,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new().read(WorldResourceId::Resource(ResourceId(TypeId::of::<R>()))))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_resource()
    }
}

pub type WriteResource<'a, R> = ResourceAccess<AtomicRefMut<'a, R>>;

impl<'a, R> SystemData<'a> for WriteResource<'a, R>
where
    R: Send + 'static,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new().write(WorldResourceId::Resource(ResourceId(TypeId::of::<R>()))))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_resource()
    }
}

pub struct ComponentAccess<'e, C, R>
where
    C: Component,
    R: Deref<Target = MaskedStorage<C>>,
{
    entities: Entities<'e>,
    storage: R,
}

impl<'e, C, R> ComponentAccess<'e, C, R>
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

impl<'e, C, R> ComponentAccess<'e, C, R>
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

    pub fn update(&mut self, e: Entity, c: C) -> Result<Option<C>, WrongGeneration>
    where
        C: PartialEq,
    {
        if self.entities.is_alive(e) {
            Ok(self.storage.update(e.index(), c))
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

    pub fn guard(&mut self) -> GuardedJoin<C> {
        self.storage.guard()
    }
}

impl<'e, C, R> ComponentAccess<'e, C, R>
where
    C: Component,
    C::Storage: TrackedStorage<C>,
    R: DerefMut<Target = MaskedStorage<C>>,
{
    pub fn set_track_modified(&mut self, flag: bool) {
        self.storage.raw_storage_mut().set_track_modified(flag);
    }

    pub fn tracking_modified(&self) -> bool {
        self.storage.raw_storage().tracking_modified()
    }

    pub fn modified_indexes(&self) -> &AtomicBitSet {
        self.storage.raw_storage().modified()
    }

    pub fn clear_modified(&mut self) {
        self.storage.raw_storage_mut().clear_modified();
    }
}

impl<'a, 'e, C, R> IntoJoin for &'a ComponentAccess<'e, C, R>
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

impl<'a, 'e, C, R> IntoJoin for &'a mut ComponentAccess<'e, C, R>
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
    C: Component + Send + Sync + 'static,
    C::Storage: Send + Sync,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new()
            .read(WorldResourceId::Entities)
            .read(WorldResourceId::Component(ComponentId(TypeId::of::<C>()))))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_component()
    }
}

pub type WriteComponent<'a, C> = ComponentAccess<'a, C, AtomicRefMut<'a, MaskedStorage<C>>>;

impl<'a, C> SystemData<'a> for WriteComponent<'a, C>
where
    C: Component + Send + 'static,
    C::Storage: Send,
{
    type Source = World;
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new()
            .read(WorldResourceId::Entities)
            .write(WorldResourceId::Component(ComponentId(TypeId::of::<C>()))))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_component()
    }
}
