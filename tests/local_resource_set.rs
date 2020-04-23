use goggles::{
    fetch_resources::FetchResources,
    local_resource_set::{Read, ResourceSet, Write},
};

#[test]
fn test_fetch_resources() {
    struct A;
    struct B;
    struct C;

    let mut res = ResourceSet::new();
    res.insert(A);
    res.insert(B);
    res.insert(C);

    let _sys_data = res.fetch::<(Read<A>, Write<B>, Write<C>)>();
}

#[test]
fn test_conflicts() {
    struct A;
    struct B;

    assert!(<(Read<A>, Read<B>, Write<A>)>::check_resources().is_err());
}
