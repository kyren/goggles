use std::collections::HashSet;

use goggles::{par, seq, ResourceConflict, Resources, RwResources, SeqPool, System, SystemError};

#[derive(Default)]
struct TestResources(HashSet<&'static str>);

impl Resources for TestResources {
    fn union(&mut self, other: &Self) {
        for s in &other.0 {
            self.0.insert(s);
        }
    }

    fn conflicts_with(&self, other: &Self) -> bool {
        HashSet::intersection(&self.0, &other.0).next().is_some()
    }
}

#[derive(Debug)]
struct TestError;

impl SystemError for TestError {
    fn combine(self, _: Self) -> Self {
        TestError
    }
}

macro_rules! test_system {
        ($s:ident, $($resources:expr),*) => {
            struct $s;

            impl System for $s {
                type Source = ();
                type Resources = TestResources;
                type Pool = SeqPool;
                type Args = ();
                type Error = TestError;

                fn check_resources(&self) -> Result<TestResources, ResourceConflict> {
                    Ok(TestResources([$($resources),*].iter().copied().collect()))
                }

                fn run(&mut self, _: &Self::Pool, _: &Self::Source, _: &Self::Args) -> Result<(), Self::Error> {
                    Ok(())
                }
            }
        }
    }

test_system!(SystemA, "resource_a", "resource_b");
test_system!(SystemB, "resource_c");
test_system!(SystemC, "resource_a", "resource_b", "resource_c");
test_system!(SystemD, "resource_d");
test_system!(SystemE, "resource_e");

#[test]
fn test_par_seq() {
    let mut sys = par![seq![SystemA, SystemB], SystemD, SystemE];
    sys.check_resources().unwrap();
    sys.run(&SeqPool, &(), &()).unwrap();

    let mut sys = seq![par![SystemA, SystemB], SystemC, SystemD, SystemE];
    sys.check_resources().unwrap();
    sys.run(&SeqPool, &(), &()).unwrap();
}

#[test]
fn test_par_seq_conflict() {
    let sys = par![seq![SystemA, SystemB], SystemC];
    assert!(sys.check_resources().is_err());
}

#[test]
fn test_read_write_resources() {
    let rw1 = RwResources::new()
        .read("r1")
        .read("r2")
        .write("r3")
        .read("r3")
        .read("r4");

    let rw2 = RwResources::new().read("r2").read("r4");

    let rw3 = RwResources::new().read("r3").read("r4").write("r5");

    assert!(!rw1.conflicts_with(&rw2));
    assert!(rw1.conflicts_with(&rw3));
    assert!(!rw2.conflicts_with(&rw3));

    let mut rw4 = RwResources::new();
    rw4.union(&rw1);
    rw4.union(&rw2);
    assert!(rw4.conflicts_with(&rw3));
}
