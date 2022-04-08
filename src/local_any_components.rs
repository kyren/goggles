use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

use crate::{
    entity::{Entity, WrongGeneration},
    local_world::World,
    world_common::Component,
};

/// A dynamic set of components that can be inserted into a world.
#[derive(Default)]
pub struct AnyComponentSet {
    // TODO: This is slower than anymap, at least switch to using anymap's TypeIdHasher when that is
    // public (anymap 1.0 release).
    components: FxHashMap<TypeId, Box<dyn AnyComponent>>,
}

impl AnyComponentSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get<C>(&self) -> Option<&C>
    where
        C: Component + 'static,
    {
        self.components
            .get(&TypeId::of::<C>())
            .map(|c| c.as_any().downcast_ref().unwrap())
    }

    pub fn get_mut<C>(&mut self) -> Option<&mut C>
    where
        C: Component + 'static,
    {
        self.components
            .get_mut(&TypeId::of::<C>())
            .map(|c| c.as_any_mut().downcast_mut().unwrap())
    }

    pub fn insert<C>(&mut self, c: C) -> Option<C>
    where
        C: Component + 'static,
    {
        self.components
            .insert(TypeId::of::<C>(), Box::new(c))
            .map(|c| *c.into_any().downcast::<C>().ok().unwrap())
    }

    pub fn remove<C>(&mut self) -> Option<C>
    where
        C: Component + 'static,
    {
        self.components
            .remove(&TypeId::of::<C>())
            .map(|c| *c.into_any().downcast().ok().unwrap())
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    /// Merges the given component set on top of this one.
    ///
    /// Returns true if any component in this set was overwritten by the merge.
    pub fn merge(&mut self, other: AnyComponentSet) -> bool {
        let mut overwritten = false;
        for (type_id, component) in other.components.into_iter() {
            overwritten |= self.components.insert(type_id, component).is_some();
        }
        overwritten
    }

    /// Insert all of the contained components into the given world.
    ///
    /// Returns true if any component in this set overwrote any existing component for the given
    /// entity.
    ///
    /// # Panics
    /// Panics if any of the component types in this set are not previously registered into the
    /// given world.
    pub fn insert_into_world(
        self,
        world: &mut World,
        entity: Entity,
    ) -> Result<bool, WrongGeneration> {
        let mut overwritten = false;
        for (_, component) in self.components {
            overwritten |= component.insert_into_world(world, entity)?;
        }
        Ok(overwritten)
    }
}

#[derive(Default)]
pub struct AnyCloneComponentSet {
    components: FxHashMap<TypeId, Box<dyn AnyCloneComponent>>,
}

impl AnyCloneComponentSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get<C>(&self) -> Option<&C>
    where
        C: Component + Clone + 'static,
    {
        self.components
            .get(&TypeId::of::<C>())
            .map(|c| c.as_any().downcast_ref().unwrap())
    }

    pub fn get_mut<C>(&mut self) -> Option<&mut C>
    where
        C: Component + Clone + 'static,
    {
        self.components
            .get_mut(&TypeId::of::<C>())
            .map(|c| c.as_any_mut().downcast_mut().unwrap())
    }

    pub fn insert<C>(&mut self, c: C) -> Option<C>
    where
        C: Component + Clone + 'static,
    {
        self.components
            .insert(TypeId::of::<C>(), Box::new(c))
            .map(|c| *c.into_any().downcast::<C>().ok().unwrap())
    }

    /// Insert all of the contained components into the given world.
    ///
    /// Returns true if any component in this set overwrote any existing component for the given
    /// entity.
    ///
    /// # Panics
    /// Panics if any of the component types in this set are not previously registered into the
    /// given world.
    pub fn insert_into_world(
        &self,
        world: &mut World,
        entity: Entity,
    ) -> Result<bool, WrongGeneration> {
        let mut overwritten = false;
        for (_, component) in &self.components {
            overwritten |= component.clone_into_world(world, entity)?;
        }
        Ok(overwritten)
    }

    /// Clone all of the given components into the given `AnyComponentSet`.
    ///
    /// Returns true if any component was overwritten by an insert.
    pub fn clone_into_set(&self, component_set: &mut AnyComponentSet) -> bool {
        let mut overwritten = false;
        for (type_id, component) in self.components.iter() {
            overwritten |= component_set
                .components
                .insert(*type_id, (*component).boxed_clone())
                .is_some();
        }
        overwritten
    }
}

trait AnyComponent {
    // Should return true if inserting this component into the world overwrote a pre-existing
    // component.
    fn insert_into_world(
        self: Box<Self>,
        world: &mut World,
        entity: Entity,
    ) -> Result<bool, WrongGeneration>;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}

impl<C> AnyComponent for C
where
    C: Component + 'static,
{
    fn insert_into_world(
        self: Box<Self>,
        world: &mut World,
        entity: Entity,
    ) -> Result<bool, WrongGeneration> {
        Ok(world
            .get_component_mut::<C>()
            .insert(entity, *self)?
            .is_some())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

trait AnyCloneComponent: AnyComponent {
    fn boxed_clone(&self) -> Box<dyn AnyComponent>;
    fn clone_into_world(&self, world: &mut World, entity: Entity) -> Result<bool, WrongGeneration>;
}

impl<C> AnyCloneComponent for C
where
    C: Component + Clone + 'static,
{
    fn boxed_clone(&self) -> Box<dyn AnyComponent> {
        Box::new(self.clone())
    }

    fn clone_into_world(&self, world: &mut World, entity: Entity) -> Result<bool, WrongGeneration> {
        Ok(world
            .get_component_mut::<C>()
            .insert(entity, self.clone())?
            .is_some())
    }
}
