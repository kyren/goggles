use std::sync::Arc;

use goggles::{Component, Entities, VecStorage, World, WriteComponent};

#[test]
fn test_component_drop() {
    struct CA(Arc<()>);

    impl Component for CA {
        type Storage = VecStorage<CA>;
    }

    let r = Arc::new(());

    {
        let mut world = World::new();
        world.insert_component::<CA>();
        let (entities, mut component_a): (Entities, WriteComponent<CA>) = world.fetch();

        for _ in 0..100 {
            let e = entities.create();
            component_a.insert(e, CA(Arc::clone(&r))).unwrap();
        }
    }

    assert_eq!(Arc::strong_count(&r), 1);
}
