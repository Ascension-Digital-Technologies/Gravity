use gravity_perf::{FastCounterMap, Interner, PerfArena, SlotPool};

#[test]
fn interner_reuses_identity() {
    let mut interner = Interner::with_capacity(4);
    let a = interner.intern("BTC-USDx");
    let b = interner.intern("BTC-USDx");
    assert_eq!(a.as_str(), b.as_str());
    assert_eq!(interner.len(), 1);
}

#[test]
fn pool_reuses_slots() {
    let mut pool = SlotPool::new_with(2, |i| i);
    let a = pool.checkout().unwrap();
    let b = pool.checkout().unwrap();
    assert!(pool.checkout().is_none());
    pool.release(a);
    pool.release(b);
    assert_eq!(pool.available(), 2);
}

#[test]
fn arena_records_without_growth() {
    let mut arena = PerfArena::new(4, 16, 8, 4);
    for i in 0..100 { arena.record("BTC-USDx", &format!("acct-{}", i % 8), i); }
    let stats = arena.stats();
    assert_eq!(stats.interned_symbols, 1);
    assert_eq!(stats.pool_available, stats.pool_capacity);
}

#[test]
fn counter_map_aggregates() {
    let mut map = FastCounterMap::with_capacity(4);
    map.add("a", 10);
    map.add("a", -3);
    map.add("b", -4);
    assert_eq!(map.len(), 2);
    assert_eq!(map.total_abs(), 11);
}
