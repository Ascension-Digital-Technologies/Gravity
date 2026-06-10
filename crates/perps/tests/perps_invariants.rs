use gravity_perps::{FundingUpdateRequest, PerpEngine, PerpMarketConfig, PerpPositionRequest, PerpSide};
use gravity_types::{Fixed, Price, Quantity, Symbol};

fn p(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }

#[test]
fn perps_reject_excessive_leverage_and_insufficient_margin() {
    let mut engine = PerpEngine::new();
    let symbol = Symbol::new("BTC-PERP").unwrap();
    engine.create_market(PerpMarketConfig::new(symbol.clone(), Symbol::new("BTC-USDx").unwrap())).unwrap();
    let too_much = PerpPositionRequest { account: "acct-1".into(), symbol: symbol.clone(), side: PerpSide::Long, quantity: q(1), entry_price: p(100_000), collateral: Fixed::from_units(1_000), leverage_bps: 1_000_000 };
    assert!(engine.open_position(too_much).is_err());
    let no_margin = PerpPositionRequest { account: "acct-1".into(), symbol, side: PerpSide::Long, quantity: q(1), entry_price: p(100_000), collateral: Fixed::from_units(1), leverage_bps: 10_000 };
    assert!(engine.open_position(no_margin).is_err());
}

#[test]
fn mark_price_updates_long_pnl_deterministically() {
    let mut engine = PerpEngine::new();
    let symbol = Symbol::new("ETH-PERP").unwrap();
    engine.create_market(PerpMarketConfig::new(symbol.clone(), Symbol::new("ETH-USDx").unwrap())).unwrap();
    let pos = engine.open_position(PerpPositionRequest { account: "acct-1".into(), symbol: symbol.clone(), side: PerpSide::Long, quantity: q(1), entry_price: p(5_000), collateral: Fixed::from_units(1_000), leverage_bps: 50_000 }).unwrap();
    engine.update_funding(FundingUpdateRequest { symbol: symbol.clone(), index_price: p(5_500), mark_price: p(5_500), funding_rate_bps: 1 }).unwrap();
    let refreshed = engine.positions_for_account("acct-1").into_iter().find(|p| p.id == pos.id).unwrap();
    assert!(refreshed.unrealized_pnl.as_raw() > 0);
    assert!(refreshed.equity.as_raw() > pos.equity.as_raw());
}
