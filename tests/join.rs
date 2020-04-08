use hibitset::{BitSet, BitSetAll, BitSetAnd, BitSetNot, BitSetOr, BitSetXor};

use goggles::join::BitSetConstrained;

#[test]
fn test_bitset_constrained() {
    assert!(BitSet::new().is_constrained());
    assert!(!BitSetAll.is_constrained());
    assert!(!BitSetNot(BitSet::new()).is_constrained());
    assert!(BitSetNot(BitSetAll).is_constrained());
    assert!(BitSetAnd(BitSetNot(BitSetAll), BitSetAll).is_constrained());
    assert!(!BitSetOr(BitSetNot(BitSetAll), BitSetAll).is_constrained());
    assert!(!BitSetXor(BitSetNot(BitSetAll), BitSetAll).is_constrained());
    assert!(BitSetOr(BitSetNot(BitSetAll), BitSet::new()).is_constrained());
    assert!(BitSetXor(BitSetNot(BitSetAll), BitSet::new()).is_constrained());
}
