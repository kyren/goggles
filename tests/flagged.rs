use hibitset::BitSetLike;

use goggles::{
    join::IntoJoinExt, Component, Entities, Flagged, ReadComponent, VecStorage, World,
    WriteComponent,
};

#[derive(PartialEq)]
struct CA(i32);

impl Component for CA {
    type Storage = Flagged<VecStorage<CA>>;
}

#[derive(PartialEq)]
struct CB(i32);

impl Component for CB {
    type Storage = Flagged<VecStorage<CB>>;
}

#[test]
fn test_flagged() {
    let mut world = World::new();

    world.insert_component::<CA>();
    world.insert_component::<CB>();

    let mut evec = Vec::new();
    for _ in 0..100 {
        evec.push(world.create_entity());
    }

    {
        let (entities, mut component_a, mut component_b): (
            Entities,
            WriteComponent<CA>,
            WriteComponent<CB>,
        ) = world.fetch();

        for &e in &evec {
            component_a.insert(e, CA(-1)).unwrap();
            component_b.insert(e, CB(-1)).unwrap();
        }

        // flagged components do not track by default
        assert!(component_a.modified_indexes().is_empty());
        assert!(component_b.modified_indexes().is_empty());

        component_a.set_track_modified(true);
        component_b.set_track_modified(true);

        for &e in &evec {
            let mut a = component_a.get_guard(e).unwrap();
            a.get_mut().0 = e.index() as i32;

            let mut b = component_b.get_guard(e).unwrap();
            b.get_mut().0 = e.index() as i32;
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 100);
        assert_eq!(component_b.modified_indexes().iter().count(), 100);

        component_a.clear_modified();
        component_b.clear_modified();

        for (_, mut a, mut b) in (&entities, component_a.guard(), component_b.guard()).join() {
            let av = a.get().0;
            let an = av - av % 2 + 1;
            if av != an {
                a.get_mut().0 = an;
            }

            let bv = b.get().0;
            let bn = bv - bv % 2 + 1;
            if bv != bn {
                b.get_mut().0 = bn;
            }
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 50);
        assert_eq!(component_b.modified_indexes().iter().count(), 50);

        for a in component_a.modified().join() {
            assert!(a.unwrap().0 % 2 == 1);
        }

        for b in component_b.modified_mut().join() {
            assert!(b.unwrap().0 % 2 == 1);
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 50);
        assert_eq!(component_b.modified_indexes().iter().count(), 50);

        component_a.clear_modified();
        component_b.clear_modified();

        for i in 0..50 {
            entities.delete(evec[i]).unwrap();
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 0);
        assert_eq!(component_b.modified_indexes().iter().count(), 0);
    }

    world.merge();

    let (component_a, component_b): (ReadComponent<CA>, ReadComponent<CB>) = world.fetch();

    assert_eq!(component_a.modified_indexes().iter().count(), 50);
    assert_eq!(component_b.modified_indexes().iter().count(), 50);
}

#[test]
fn test_local_flagged() {
    let mut world = World::new();

    world.insert_component::<CA>();
    world.insert_component::<CB>();

    let mut evec = Vec::new();
    for _ in 0..100 {
        evec.push(world.create_entity());
    }

    {
        let (entities, mut component_a, mut component_b): (
            Entities,
            WriteComponent<CA>,
            WriteComponent<CB>,
        ) = world.fetch();

        for &e in &evec {
            component_a.insert(e, CA(-1)).unwrap();
            component_b.insert(e, CB(-1)).unwrap();
        }

        // flagged components do not track by default
        assert!(component_a.modified_indexes().is_empty());
        assert!(component_b.modified_indexes().is_empty());

        component_a.set_track_modified(true);
        component_b.set_track_modified(true);

        for &e in &evec {
            let mut a = component_a.get_guard(e).unwrap();
            a.get_mut().0 = e.index() as i32;

            let mut b = component_b.get_guard(e).unwrap();
            b.get_mut().0 = e.index() as i32;
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 100);
        assert_eq!(component_b.modified_indexes().iter().count(), 100);

        component_a.clear_modified();
        component_b.clear_modified();

        for (_, mut a, mut b) in (&entities, component_a.guard(), component_b.guard()).join() {
            let av = a.get().0;
            let an = av - av % 2 + 1;
            if av != an {
                a.get_mut().0 = an;
            }

            let bv = b.get().0;
            let bn = bv - bv % 2 + 1;
            if bv != bn {
                b.get_mut().0 = bn;
            }
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 50);
        assert_eq!(component_b.modified_indexes().iter().count(), 50);

        for a in component_a.modified().join() {
            assert!(a.unwrap().0 % 2 == 1);
        }

        for b in component_b.modified_mut().join() {
            assert!(b.unwrap().0 % 2 == 1);
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 50);
        assert_eq!(component_b.modified_indexes().iter().count(), 50);

        component_a.clear_modified();
        component_b.clear_modified();

        for i in 0..50 {
            entities.delete(evec[i]).unwrap();
        }

        assert_eq!(component_a.modified_indexes().iter().count(), 0);
        assert_eq!(component_b.modified_indexes().iter().count(), 0);
    }

    world.merge();

    let (component_a, component_b): (ReadComponent<CA>, ReadComponent<CB>) = world.fetch();

    assert_eq!(component_a.modified_indexes().iter().count(), 50);
    assert_eq!(component_b.modified_indexes().iter().count(), 50);
}
