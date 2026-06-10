use gravity_risk::{AccountRiskInput, CollateralInput, PositionInput, RiskEngine, RiskStatus};
use gravity_types::{Fixed, Price, Quantity, Symbol};

fn p(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }

#[test]
fn healthy_account_has_positive_equity_and_free_collateral() {
    let mut engine = RiskEngine::default();
    let input = AccountRiskInput {
        account: "acct-healthy".into(),
        collaterals: vec![CollateralInput { asset: "USDx".into(), quantity: q(10_000), price: p(1), collateral_factor_bps: 9_000 }],
        positions: vec![PositionInput { symbol: Symbol::new("BTC-USDx").unwrap(), quantity: q(1), mark_price: p(50_000), side: "long".into() }],
        debt_value: Fixed::ZERO,
        maintenance_margin_bps: 1_000,
        initial_margin_bps: 2_000,
        timestamp_ms: Some(1),
    };
    let snapshot = engine.check(input).unwrap();
    assert!(matches!(snapshot.status, RiskStatus::Healthy | RiskStatus::Watch));
    assert!(snapshot.equity.as_raw() > 0);
    assert!(snapshot.oracle_dependencies.iter().any(|s| s.0 == "BTC-USDx"));
}

#[test]
fn deeply_underwater_account_is_liquidatable() {
    let mut engine = RiskEngine::default();
    let input = AccountRiskInput {
        account: "acct-bad".into(),
        collaterals: vec![CollateralInput { asset: "USDx".into(), quantity: q(100), price: p(1), collateral_factor_bps: 8_000 }],
        positions: vec![PositionInput { symbol: Symbol::new("ETH-USDx").unwrap(), quantity: q(10), mark_price: p(5_000), side: "long".into() }],
        debt_value: Fixed::from_units(100_000),
        maintenance_margin_bps: 1_000,
        initial_margin_bps: 2_000,
        timestamp_ms: Some(2),
    };
    let snapshot = engine.check(input).unwrap();
    assert_eq!(snapshot.status, RiskStatus::Liquidatable);
    assert!(snapshot.warnings.iter().any(|w| w.contains("negative equity")));
}
