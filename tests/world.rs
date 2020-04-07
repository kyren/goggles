use goggles::{
    component::{Component, VecStorage},
    join::JoinExt,
    world::{ReadComponent, ReadEntities, ReadResource, World, WriteComponent, WriteResource},
};

struct RA(i32);
struct RB(i32);

struct CA(u32);

impl Component for CA {
    type Storage = VecStorage<CA>;
}

struct CB(u32);

impl Component for CB {
    type Storage = VecStorage<CB>;
}

#[test]
fn test_world() {
    let mut world = World::new();

    world.insert_resource(RA(1));
    world.insert_resource(RB(2));

    world.insert_component::<CA>();
    world.insert_component::<CB>();

    {
        let (entities, mut component_a, mut component_b): (
            ReadEntities,
            WriteComponent<CA>,
            WriteComponent<CB>,
        ) = world.fetch();

        for _ in 0..100 {
            let e = entities.allocate_atomic();
            component_a.insert(e, CA(e.index())).unwrap();
            component_b.insert(e, CB(e.index())).unwrap();
        }
    }

    let (entities, resource_a, resource_b, component_a, component_b): (
        ReadEntities,
        ReadResource<RA>,
        WriteResource<RB>,
        ReadComponent<CA>,
        WriteComponent<CB>,
    ) = world.fetch();

    assert_eq!(resource_a.0, 1);
    assert_eq!(resource_b.0, 2);

    for (e, a, b) in (&entities, &component_a, &component_b).join() {
        assert_eq!(e.index(), a.0);
        assert_eq!(e.index(), b.0);
    }

    assert_eq!((&entities, &component_a, &component_b).join().count(), 100);
}
