pub use hibitset;

pub mod any_components;
pub mod entity;
pub mod fetch_resources;
pub mod join;
pub mod make_sync;
pub mod masked;
pub mod resource_set;
pub mod resources;
pub mod storage;
pub mod system;
pub mod tracked;
pub mod world;
pub mod world_common;

pub use {
    self::entity::{Entity, WrongGeneration},
    any_components::{AnyCloneComponentSet, AnyComponentSet},
    fetch_resources::{FetchNone, FetchResources},
    join::{Index, IntoJoin, IntoJoinExt, Join, JoinIter, JoinIterUnconstrained, JoinParIter},
    make_sync::MakeSync,
    masked::MaskedStorage,
    resource_set::{Read, ResourceSet, Write},
    resources::{ResourceConflict, Resources, RwResources},
    storage::{DenseStorage, DenseVecStorage, HashMapStorage, RawStorage, VecStorage},
    system::{parallelize, Error as SystemError, Par, Pool, Seq, SeqPool, System},
    tracked::{Flagged, TrackedStorage},
    world::{Entities, ReadComponent, ReadResource, World, WriteComponent, WriteResource},
    world_common::{Component, ComponentId, ResourceId, WorldResourceId, WorldResources},
};

#[cfg(feature = "rayon")]
pub use rayon;

#[cfg(feature = "rayon")]
pub mod par_join;
#[cfg(feature = "rayon")]
pub mod rayon_pool;

#[cfg(feature = "rayon")]
pub use self::{par_join::ParJoinExt, rayon_pool::RayonPool};
