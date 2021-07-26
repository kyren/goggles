use std::{
    any::TypeId,
    cell::{Ref, RefMut},
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use rustc_hash::FxHashMap;

use crate::{
    entity::{Allocator, Entity, LiveBitSet, WrongGeneration},
    fetch_resources::FetchResources,
    join::{Index, IntoJoin},
    local_resource_set::ResourceSet,
    masked::{GuardedElement, GuardedJoin, ModifiedJoin, ModifiedJoinMut},
    resources::{ResourceConflict, RwResources},
    storage::DenseStorage,
    tracked::{ModifiedBitSet, TrackedStorage},
    world_common::{Component, ComponentId, ComponentStorage, ResourceId, WorldResourceId},
};

#[derive(Default)]
pub struct World {
    allocator: Allocator,
    resources: ResourceSet,
    components: ResourceSet,
    remove_components: FxHashMap<TypeId, Box<dyn Fn(&ResourceSet, &[Entity])>>,
    killed: Vec<Entity>,
}

impl World {
    pub fn new() -> Self {
        World {
            allocator: Allocator::new(),
            resources: ResourceSet::new(),
            components: ResourceSet::new(),
            remove_components: FxHashMap::default(),
            killed: Vec::new(),
        }
    }

    pub fn entities(&self) -> Entities {
        Entities(&self.allocator)
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
        R: 'static,
    {
        self.resources.insert(r)
    }

    pub fn remove_resource<R>(&mut self) -> Option<R>
    where
        R: 'static,
    {
        self.resources.remove::<R>()
    }

    pub fn contains_resource<T>(&self) -> bool
    where
        T: 'static,
    {
        self.resources.contains::<T>()
    }

    /// Borrow the given resource immutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed mutably.
    pub fn read_resource<R>(&self) -> ReadResource<R>
    where
        R: 'static,
    {
        ResourceAccess(self.resources.borrow())
    }

    /// Borrow the given resource mutably.
    ///
    /// # Panics
    /// Panics if the resource has not been inserted or is already borrowed.
    pub fn write_resource<R>(&self) -> WriteResource<R>
    where
        R: 'static,
    {
        ResourceAccess(self.resources.borrow_mut())
    }

    /// # Panics
    /// Panics if the resource has not been inserted.
    pub fn get_resource_mut<R>(&mut self) -> &mut R
    where
        R: 'static,
    {
        self.resources.get_mut()
    }

    /// Insert a new, fresh storage for the given component.
    ///
    /// If the component was already inserted, this will clear the storage for the component first.
    pub fn insert_component<C>(&mut self) -> Option<ComponentStorage<C>>
    where
        C: Component + 'static,
        C::Storage: Default,
    {
        self.remove_components.insert(
            TypeId::of::<C>(),
            Box::new(|resource_set, entities| {
                let mut storage = resource_set.borrow_mut::<ComponentStorage<C>>();
                for e in entities {
                    storage.remove(e.index());
                }
            }),
        );
        self.components.insert(ComponentStorage::<C>::default())
    }

    /// Remove storage for the given component.
    pub fn remove_component<C>(&mut self) -> Option<ComponentStorage<C>>
    where
        C: Component + 'static,
        C::Storage: Default,
    {
        self.remove_components.remove(&TypeId::of::<C>());
        self.components.remove::<ComponentStorage<C>>()
    }

    pub fn contains_component<C>(&self) -> bool
    where
        C: Component + 'static,
    {
        self.components.contains::<ComponentStorage<C>>()
    }

    /// Borrow the given component immutably.
    ///
    /// # Panics
    /// Panics if the component has not been inserted or is already borrowed mutably.
    pub fn read_component<C>(&self) -> ReadComponent<C>
    where
        C: Component + 'static,
    {
        ComponentAccess {
            storage: self.components.borrow(),
            entities: self.entities(),
            marker: PhantomData,
        }
    }

    /// Borrow the given component mutably.
    ///
    /// # Panics
    /// Panics if the component has not been inserted or is already borrowed.
    pub fn write_component<C>(&self) -> WriteComponent<C>
    where
        C: Component + 'static,
    {
        ComponentAccess {
            storage: self.components.borrow_mut(),
            entities: self.entities(),
            marker: PhantomData,
        }
    }

    /// # Panics
    /// Panics if the component has not been inserted.
    pub fn get_component_mut<C>(&mut self) -> ComponentAccess<C, &mut ComponentStorage<C>>
    where
        C: Component + 'static,
    {
        ComponentAccess {
            storage: self.components.get_mut(),
            entities: Entities(&self.allocator),
            marker: PhantomData,
        }
    }

    pub fn fetch<'a, F>(&'a self) -> F
    where
        F: FetchResources<'a, World>,
    {
        F::fetch(self)
    }

    /// Merge any pending entity operations.
    ///
    /// Merges atomically allocated entities into the normal entity `BitSet` for performance, and
    /// finalizes any entities that were requested to be deleted.
    ///
    /// No entity is actually removed until this method is called.
    pub fn merge(&mut self) {
        self.allocator.merge_atomic(&mut self.killed);
        for remove_component in self.remove_components.values() {
            remove_component(&self.components, &self.killed);
        }
    }
}

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

impl<'a> FetchResources<'a, World> for Entities<'a> {
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

/// `SystemData` type that reads the given resource.
///
/// # Panics
/// Panics if the resource does not exist or has already been borrowed for writing.
pub type ReadResource<'a, R> = ResourceAccess<Ref<'a, R>>;

impl<'a, R> FetchResources<'a, World> for ReadResource<'a, R>
where
    R: 'static,
{
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new().read(WorldResourceId::Resource(ResourceId::of::<R>())))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_resource()
    }
}

/// `SystemData` type that writes the given resource.
///
/// # Panics
/// Panics if the resource does not exist or has already been borrowed for writing.
pub type WriteResource<'a, R> = ResourceAccess<RefMut<'a, R>>;

impl<'a, R> FetchResources<'a, World> for WriteResource<'a, R>
where
    R: 'static,
{
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new().write(WorldResourceId::Resource(ResourceId::of::<R>())))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_resource()
    }
}

/// Returned from the `World` methods `read_component`, `write_component`, and `get_component_mut`.
///
/// This is a simple wrapper around a `MaskedStorage` paired with an entity `Allocator`.  It
/// prevents you from inserting or accessing components that do not have a live `Entity` associated
/// with them.
pub struct ComponentAccess<'a, C, R>
where
    C: Component,
{
    entities: Entities<'a>,
    storage: R,
    marker: PhantomData<C>,
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    R: Deref<Target = ComponentStorage<C>>,
{
    pub fn entities(&self) -> &Entities {
        &self.entities
    }

    pub fn storage(&self) -> &ComponentStorage<C> {
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
    R: DerefMut<Target = ComponentStorage<C>>,
{
    /// Access the inner `MaskedStorage` type.
    ///
    /// It is possible by using this type directly to insert components into the underlying
    /// `MaskedStorage` for indexes that do not have a live `Entity` associated with them.  This is
    /// not unsafe to do, but it is probably incorrect, and such components may either never be
    /// automatically removed or possibly be assigned to new entities that have the same index.
    pub fn storage_mut(&mut self) -> &mut ComponentStorage<C> {
        &mut self.storage
    }

    pub fn get_mut(&mut self, e: Entity) -> Option<&mut C> {
        if self.entities.is_alive(e) {
            self.storage.get_mut(e.index())
        } else {
            None
        }
    }

    pub fn get_guard<'b>(&'b mut self, e: Entity) -> Option<GuardedElement<'b, C::Storage>> {
        if self.entities.is_alive(e) {
            self.storage.get_guard(e.index())
        } else {
            None
        }
    }

    pub fn get_or_insert_with(
        &mut self,
        e: Entity,
        f: impl FnOnce() -> C,
    ) -> Result<&mut C, WrongGeneration> {
        if self.entities.is_alive(e) {
            Ok(self.storage.get_or_insert_with(e.index(), f))
        } else {
            Err(WrongGeneration)
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

    pub fn guard(&mut self) -> GuardedJoin<C::Storage> {
        self.storage.guard()
    }
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    C::Storage: DenseStorage,
    R: Deref<Target = ComponentStorage<C>>,
{
    pub fn as_slice(&self) -> &[C] {
        self.storage.as_slice()
    }
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    C::Storage: TrackedStorage,
    R: Deref<Target = ComponentStorage<C>>,
{
    pub fn tracking_modified(&self) -> bool {
        self.storage.raw_storage().tracking_modified()
    }

    pub fn modified_indexes(&self) -> &ModifiedBitSet {
        self.storage.raw_storage().modified_indexes()
    }

    pub fn mark_modified(&self, entity: Entity) -> Result<(), WrongGeneration> {
        if self.entities.is_alive(entity) {
            self.storage.raw_storage().mark_modified(entity.index());
            Ok(())
        } else {
            Err(WrongGeneration)
        }
    }

    pub fn modified(&self) -> ModifiedJoin<C::Storage> {
        self.storage.modified()
    }
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    C::Storage: DenseStorage,
    R: DerefMut<Target = ComponentStorage<C>>,
{
    pub fn as_mut_slice(&mut self) -> &mut [C] {
        self.storage.as_mut_slice()
    }
}

impl<'a, C, R> ComponentAccess<'a, C, R>
where
    C: Component,
    C::Storage: TrackedStorage,
    R: DerefMut<Target = ComponentStorage<C>>,
{
    pub fn set_track_modified(&mut self, flag: bool) {
        self.storage.raw_storage_mut().set_track_modified(flag);
    }

    pub fn clear_modified(&mut self) {
        self.storage.raw_storage_mut().clear_modified();
    }

    pub fn modified_mut(&mut self) -> ModifiedJoinMut<C::Storage> {
        self.storage.modified_mut()
    }
}

impl<'a, 'b, C, R> IntoJoin for &'a ComponentAccess<'b, C, R>
where
    C: Component,
    R: Deref<Target = ComponentStorage<C>> + 'a,
{
    type Item = &'a C;
    type IntoJoin = &'a ComponentStorage<C>;

    fn into_join(self) -> Self::IntoJoin {
        (&*self.storage).into_join()
    }
}

impl<'a, 'b, C, R> IntoJoin for &'a mut ComponentAccess<'b, C, R>
where
    C: Component,
    R: DerefMut<Target = ComponentStorage<C>> + 'a,
{
    type Item = &'a mut C;
    type IntoJoin = &'a mut ComponentStorage<C>;

    fn into_join(self) -> Self::IntoJoin {
        (&mut *self.storage).into_join()
    }
}

/// `SystemData` type that reads the given component.
///
/// # Panics
/// Panics if the component does not exist or has already been borrowed for writing.
pub type ReadComponent<'a, C> = ComponentAccess<'a, C, Ref<'a, ComponentStorage<C>>>;

impl<'a, C> FetchResources<'a, World> for ReadComponent<'a, C>
where
    C: Component + 'static,
{
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new()
            .read(WorldResourceId::Entities)
            .read(WorldResourceId::Component(ComponentId::of::<C>())))
    }

    fn fetch(world: &'a World) -> Self {
        world.read_component()
    }
}

/// `SystemData` type that writes the given component.
///
/// # Panics
/// Panics if the component does not exist or has already been borrowed for writing.
pub type WriteComponent<'a, C> = ComponentAccess<'a, C, RefMut<'a, ComponentStorage<C>>>;

impl<'a, C> FetchResources<'a, World> for WriteComponent<'a, C>
where
    C: Component + 'static,
{
    type Resources = RwResources<WorldResourceId>;

    fn check_resources() -> Result<RwResources<WorldResourceId>, ResourceConflict> {
        Ok(RwResources::new()
            .read(WorldResourceId::Entities)
            .write(WorldResourceId::Component(ComponentId::of::<C>())))
    }

    fn fetch(world: &'a World) -> Self {
        world.write_component()
    }
}
