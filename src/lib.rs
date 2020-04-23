pub use hibitset;

pub mod entity;
pub mod fetch_resources;
pub mod join;
pub mod make_sync;
pub mod masked;
pub mod par_seq;
pub mod resource_set;
pub mod resources;
pub mod storage;
pub mod tracked;
pub mod world;

pub use {
    self::entity::{Entity, WrongGeneration},
    fetch_resources::FetchResources,
    join::{Index, IntoJoin, IntoJoinExt, Join, JoinIter, JoinIterUnconstrained, JoinParIter},
    masked::MaskedStorage,
    par_seq::{Error as SystemError, Par, Pool, Seq, SeqPool, System},
    resource_set::{Read, ResourceSet, Write},
    resources::{ResourceConflict, Resources, RwResources},
    storage::{DenseVecStorage, HashMapStorage, RawStorage, VecStorage},
    tracked::{Flagged, TrackedStorage},
    world::{
        Component, ComponentId, Entities, ReadComponent, ReadResource, ResourceId, World,
        WorldResourceId, WorldResources, WriteComponent, WriteResource,
    },
};

#[cfg(feature = "rayon")]
pub use rayon;

#[cfg(feature = "rayon")]
pub mod par_join;
#[cfg(feature = "rayon")]
pub mod rayon_pool;

#[cfg(feature = "rayon")]
pub use self::{par_join::ParJoinExt, rayon_pool::RayonPool};
