use goggles::{DenseVecStorage, IntoJoinExt, MaskedStorage, VecStorage};

pub struct CompA(i32);
pub struct CompB(i32);

#[test]
fn test_masked_storage_join() {
    let mut a_storage = MaskedStorage::<VecStorage<CompA>>::default();
    let mut b_storage = MaskedStorage::<DenseVecStorage<CompB>>::default();

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

#[cfg(feature = "rayon")]
#[test]
fn test_masked_storage_par_join() {
    use goggles::ParJoinExt;
    use rayon::iter::ParallelIterator;

    let mut a_storage = MaskedStorage::<VecStorage<CompA>>::default();
    let mut b_storage = MaskedStorage::<DenseVecStorage<CompB>>::default();

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
