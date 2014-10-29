pub type SequenceNr = u8;
pub static SEQUENCE_NR_MAX: SequenceNr = ::std::u8::MAX;
pub static SEQUENCE_NR_MIN: SequenceNr = ::std::u8::MIN;

pub fn overflow_aware_compare(a: SequenceNr, b: SequenceNr) -> Ordering {
    use std::cmp::{max, min};
    
    let difference = max(a, b) - min(a, b);
    
    if difference < SEQUENCE_NR_MAX / 2 {
        a.cmp(&b)
    } else {
        b.cmp(&a)
    }
}

#[test]
fn test_compare() {
    fn test_sequential(a: SequenceNr) {
        assert_eq!(overflow_aware_compare(a, a), Equal);
        assert_eq!(overflow_aware_compare(a, a + 1), Less);
        assert_eq!(overflow_aware_compare(a + 1, a), Greater);
    }

    test_sequential(0);
    test_sequential(42);
    test_sequential(SEQUENCE_NR_MAX);
    test_sequential(SEQUENCE_NR_MIN);
}
