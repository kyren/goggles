use std::any::TypeId;

use crate::{masked::MaskedStorage, resources::RwResources, storage::RawStorage};

/// A trait for component types that associates their storage type with the component type itself.
pub trait Component: Sized {
    type Storage: RawStorage<Item = Self>;
}

pub type ComponentStorage<C> = MaskedStorage<<C as Component>::Storage>;

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
