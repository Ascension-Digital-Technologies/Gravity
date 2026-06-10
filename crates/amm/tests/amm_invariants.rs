use gravity_amm::{AmmPool, PoolConfig, PoolKind, SwapSide};
use gravity_types::{Fixed, Price, Quantity, Symbol};

fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }
fn pool() -> AmmPool {
    let symbol = Symbol::new("BTC-USDx").unwrap();
    AmmPool::new(PoolConfig::normalized(symbol, PoolKind::ConstantProduct, 30, q(1)), q(100), q(10_000_000)).unwrap()
}

#[test]
fn constant_product_swap_preserves_positive_reserves_and_supply() {
    let mut pool = pool();
    let before = pool.snapshot().unwrap();
    let result = pool.swap(SwapSide::BaseIn, q(1), None).unwrap();
    assert!(result.snapshot.base_reserve.0.as_raw() > 0);
    assert!(result.snapshot.quote_reserve.0.as_raw() > 0);
    assert_eq!(before.lp_supply, result.snapshot.lp_supply);
}

#[test]
fn min_output_guard_fails_closed() {
    let mut pool = pool();
    let quote = pool.quote(SwapSide::BaseIn, q(1)).unwrap();
    let impossible_min = Quantity::new(quote.amount_out.0.checked_add(Fixed::from_units(1)).unwrap()).unwrap();
    assert!(pool.swap(SwapSide::BaseIn, q(1), Some(impossible_min)).is_err());
}

#[test]
fn oracle_deviation_guard_detects_bad_pool_price() {
    let pool = pool();
    let bad_oracle = Price::new(Fixed::from_units(1_000_000)).unwrap();
    assert!(!pool.oracle_guard(bad_oracle, 100).unwrap().allowed);
}

#[test]
fn weighted_pool_weights_must_sum_to_full_bps() {
    let symbol = Symbol::new("ETH-USDx").unwrap();
    let mut config = PoolConfig::normalized(symbol, PoolKind::Weighted, 30, q(1));
    config.base_weight_bps = 7_000;
    config.quote_weight_bps = 2_000;
    assert!(config.validate().is_err());
}
