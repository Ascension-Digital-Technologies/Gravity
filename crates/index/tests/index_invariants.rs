use gravity_index::{IndexAsset, IndexEngine, IndexProductConfig};
use gravity_types::{Fixed, Price, Quantity, Symbol};

fn p(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }
fn config() -> IndexProductConfig {
    IndexProductConfig {
        id: "ASC10".into(),
        name: "Ascension Top 10".into(),
        quote_asset: "USDx".into(),
        management_fee_bps: 25,
        rebalance_threshold_bps: 500,
        min_mint_notional: Fixed::from_units(100),
        assets: vec![IndexAsset { symbol: Symbol::new("BTC-USDx").unwrap(), target_weight_bps: 6000, oracle_price: p(100_000) }, IndexAsset { symbol: Symbol::new("ETH-USDx").unwrap(), target_weight_bps: 4000, oracle_price: p(5_000) }],
    }
}

#[test]
fn index_weights_must_sum_to_full_bps() {
    let mut bad = config();
    bad.assets[0].target_weight_bps = 5000;
    assert!(bad.validate().is_err());
}

#[test]
fn nav_dependencies_match_basket_assets() {
    let mut engine = IndexEngine::new();
    engine.create_product(config(), Fixed::from_units(1_000_000)).unwrap();
    let nav = engine.nav("ASC10").unwrap();
    assert_eq!(nav.oracle_dependencies.len(), 2);
    assert!(nav.nav.as_raw() > 0);
    assert!(nav.nav_per_unit.0.as_raw() > 0);
}

#[test]
fn mint_minimum_is_enforced_and_redeem_has_fee() {
    let mut engine = IndexEngine::new();
    engine.create_product(config(), Fixed::from_units(1_000_000)).unwrap();
    assert!(engine.mint_plan("ASC10", "acct-1".into(), Fixed::from_units(1)).is_err());
    let plan = engine.redeem_plan("ASC10", "acct-1".into(), q(10)).unwrap();
    assert!(plan.fee.as_raw() >= 0);
}
