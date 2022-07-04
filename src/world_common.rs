use std::any::TypeId;

use crate::{masked::MaskedStorage, resources::RwResources, storage::RawStorage};

/// A trait for component types that associates their storage type with the component type itself.
pub trait Component: Sized {
    type Storage: RawStorage<Item = Self>;
}

pub type ComponentStorage<C> = MaskedStorage<<C as Component>::Storage>;

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

impl WorldResourceId {
    pub fn resource<C: 'static>() -> Self {
        Self::Resource(ResourceId(TypeId::of::<C>()))
    }

    pub fn component<C: Component + 'static>() -> Self {
        Self::Component(ComponentId(TypeId::of::<C>()))
    }
}

pub type WorldResources = RwResources<WorldResourceId>;
