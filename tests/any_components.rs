use goggles::{AnyCloneComponentSet, AnyComponentSet, Component, VecStorage, World};

#[derive(Clone)]
struct CA(u32);

impl Component for CA {
    type Storage = VecStorage<CA>;
}

#[derive(Clone)]
struct CB(u32);

impl Component for CB {
    type Storage = VecStorage<CB>;
}

#[test]
fn test_any_components() {
    let mut world = World::new();

    world.insert_component::<CA>();
    world.insert_component::<CB>();

    let mut components_prefab = AnyCloneComponentSet::new();
    components_prefab.insert::<CA>(CA(1));
    components_prefab.insert::<CB>(CB(2));

    let mut components = AnyComponentSet::new();
    components_prefab.clone_into_set(&mut components);

    assert_eq!(components.get::<CA>().unwrap().0, 1);
    assert_eq!(components.get::<CB>().unwrap().0, 2);

    components.get_mut::<CA>().unwrap().0 = 3;
    components.get_mut::<CB>().unwrap().0 = 4;

    let entity = world.create_entity();

    components_prefab
        .insert_into_world(&mut world, entity)
        .unwrap();
    assert_eq!(world.read_component::<CA>().get(entity).unwrap().0, 1);
    assert_eq!(world.read_component::<CB>().get(entity).unwrap().0, 2);

    components.insert_into_world(&mut world, entity).unwrap();
    assert_eq!(world.read_component::<CA>().get(entity).unwrap().0, 3);
    assert_eq!(world.read_component::<CB>().get(entity).unwrap().0, 4);
}
