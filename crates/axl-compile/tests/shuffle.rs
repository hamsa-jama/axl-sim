mod common;
use common::*;
use proptest::prelude::*;

/// Fisher-Yates with a tiny deterministic generator, so the test needs no extra crate.
fn shuffle<T>(v: &mut [T], mut s: u64) {
    for i in (1..v.len()).rev() {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (s >> 33) as usize % (i + 1);
        v.swap(i, j);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]
    #[test]
    fn shuffled_part_order_gives_identical_mjcf(seed in any::<u64>(), which in 0usize..2) {
        let id = ["rover", "test_chassis"][which];
        let asm = starter(id);
        let base = compile_asm(&asm);
        let mut s = asm.clone();
        shuffle(&mut s.parts, seed);
        let c = compile_asm(&s);
        prop_assert_eq!(&c.mjcf, &base.mjcf);
        prop_assert_eq!(&c.devices, &base.devices);
        prop_assert_eq!(&c.bodies, &base.bodies);
    }
}
