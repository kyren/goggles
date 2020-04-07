use std::collections::HashSet;

use goggles::entity::Allocator;

#[test]
fn allocate_atomic() {
    let mut allocator = Allocator::default();

    let mut hash_set = HashSet::new();
    hash_set.insert(allocator.allocate());
    hash_set.insert(allocator.allocate());
    hash_set.insert(allocator.allocate_atomic());
    hash_set.insert(allocator.allocate_atomic());
    hash_set.insert(allocator.allocate());

    assert_eq!(hash_set.len(), 5);

    for &e in &hash_set {
        assert!(allocator.is_alive(e));
    }
}

#[test]
fn allocate_atomic_kill_atomic() {
    let mut allocator = Allocator::default();

    let e1 = allocator.allocate();
    let e2 = allocator.allocate_atomic();
    let e3 = allocator.allocate_atomic();
    let e4 = allocator.allocate_atomic();
    let e5 = allocator.allocate();
    let e6 = allocator.allocate();

    allocator.kill(e1).unwrap();
    allocator.kill(e2).unwrap();
    allocator.kill_atomic(e4).unwrap();
    allocator.kill_atomic(e5).unwrap();

    assert!(!allocator.is_alive(e1));
    assert!(!allocator.is_alive(e2));
    assert!(allocator.is_alive(e3));
    assert!(allocator.is_alive(e4));
    assert!(allocator.is_alive(e5));
    assert!(allocator.is_alive(e6));

    let mut killed = Vec::new();
    allocator.merge_atomic(&mut killed);
    assert_eq!(killed, vec![e4, e5]);

    assert!(!allocator.is_alive(e1));
    assert!(!allocator.is_alive(e2));
    assert!(allocator.is_alive(e3));
    assert!(!allocator.is_alive(e4));
    assert!(!allocator.is_alive(e5));
    assert!(allocator.is_alive(e6));
}

#[test]
fn kill_atomic_create_merge_atomic() {
    let mut allocator = Allocator::default();

    let entity = allocator.allocate();
    assert_eq!(entity.index(), 0);

    allocator.kill_atomic(entity).unwrap();

    assert_ne!(allocator.allocate(), entity);

    let mut killed = Vec::new();
    allocator.merge_atomic(&mut killed);
    assert_eq!(killed, vec![entity]);
}

#[test]
fn kill_atomic_kill_now_create_merge_atomic() {
    let mut allocator = Allocator::default();

    let entity = allocator.allocate();

    allocator.kill_atomic(entity).unwrap();

    assert_ne!(allocator.allocate(), entity);

    allocator.kill(entity).unwrap();

    allocator.allocate();

    let mut killed = Vec::new();
    allocator.merge_atomic(&mut killed);
    assert_eq!(killed, vec![]);
}
