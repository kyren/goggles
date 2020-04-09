use rayon::iter::ParallelIterator;

use goggles::{Component, DenseVecStorage, IntoJoinExt, MaskedStorage, VecStorage};

pub struct CompA(i32);

impl Component for CompA {
    type Storage = VecStorage<Self>;
}

pub struct CompB(i32);

impl Component for CompB {
    type Storage = DenseVecStorage<Self>;
}

#[test]
fn test_masked_storage_join() {
    let mut a_storage = MaskedStorage::<CompA>::default();
    let mut b_storage = MaskedStorage::<CompB>::default();

    a_storage.insert(2, CompA(4));
    a_storage.insert(3, CompA(9));
    a_storage.insert(4, CompA(16));

    b_storage.insert(3, CompB(27));
    b_storage.insert(4, CompB(64));
    b_storage.insert(5, CompB(125));

    assert_eq!(
        (&a_storage, &b_storage)
            .join()
            .map(|(a, b)| (a.0, b.0))
            .collect::<Vec<(i32, i32)>>(),
        vec![(9, 27), (16, 64)]
    );
}

#[test]
fn test_masked_storage_par_join() {
    let mut a_storage = MaskedStorage::<CompA>::default();
    let mut b_storage = MaskedStorage::<CompB>::default();

    for i in 0..1000 {
        a_storage.insert(i, CompA(i as i32));
    }

    for i in 100..1100 {
        b_storage.insert(i, CompB(i as i32));
    }

    assert_eq!(
        (&a_storage, &b_storage)
            .par_join()
            .map(|(a, b)| {
                assert_eq!(a.0, b.0);
                a.0
            })
            .collect::<Vec<i32>>(),
        (100..1000).collect::<Vec<i32>>(),
    );
}
