pub use hibitset;

pub mod entity;
pub mod fetch_resources;
pub mod join;
pub mod local_resource_set;
pub mod local_world;
pub mod make_sync;
pub mod masked;
pub mod par_seq;
pub mod resource_set;
pub mod resources;
pub mod storage;
pub mod tracked;
pub mod world;
pub mod world_common;

pub use {
    self::entity::{Entity, WrongGeneration},
    fetch_resources::FetchResources,
    join::{Index, IntoJoin, IntoJoinExt, Join, JoinIter, JoinIterUnconstrained, JoinParIter},
    local_resource_set::{Read as LocalRead, ResourceSet as LocalResourceSet, Write as LocalWrite},
    local_world::{
        Entities as LocalEntities, ReadComponent as ReadLocalComponent,
        ReadResource as ReadLocalResource, World as LocalWorld,
        WriteComponent as WriteLocalComponent, WriteResource as WriteLocalResource,
    },
    masked::MaskedStorage,
    par_seq::{Error as SystemError, Par, Pool, Seq, SeqPool, System},
    resource_set::{Read, ResourceSet, Write},
    resources::{ResourceConflict, Resources, RwResources},
    storage::{DenseVecStorage, HashMapStorage, RawStorage, VecStorage},
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
