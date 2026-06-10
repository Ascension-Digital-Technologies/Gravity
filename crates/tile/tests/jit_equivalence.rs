use gravity_tile::{JitMode, JitRegistry, KernelInput, KernelKind};

#[test]
fn all_default_jit_kernels_keep_native_equivalence() {
    let registry = JitRegistry::with_default_kernels();
    assert!(!registry.is_empty());
    let cases = [
        (KernelKind::FeeBps, KernelInput::new(1_000_000, 0, 0, 25)),
        (KernelKind::MarginRequirement, KernelInput::new(-5_000_000, 0, 0, 1_000)),
        (KernelKind::HealthBps, KernelInput::new(2_000_000, 1_000_000, 0, 0)),
        (KernelKind::NetDelta, KernelInput::new(10, 20, 5, 0)),
        (KernelKind::AmmQuote, KernelInput::new(100_000_000, 10_000_000_000, 1_000_000, 30)),
        (KernelKind::IndexNav, KernelInput::new(100, 6000, 50, 0)),
    ];
    for (kind, input) in cases {
        let check = registry.execute_checked(kind, input, JitMode::Hot);
        assert!(check.equivalent, "kernel {:?} was not equivalent", kind);
        assert_eq!(check.native, check.accelerated);
    }
}
