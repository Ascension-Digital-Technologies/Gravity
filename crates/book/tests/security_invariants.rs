use gravity_book::{AmendRequest, FeeModel, MarketRules, MarketStatus, OrderBook, OrderRequest};
use gravity_types::{Fixed, OrderKind, Price, Quantity, Side, Symbol, TimeInForce};

fn sym() -> Symbol { Symbol::new("BTC-USDx").unwrap() }
fn price(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
fn qty(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }
fn order(account: &str, side: Side, tif: TimeInForce, price_value: Option<i128>, quantity: i128) -> OrderRequest {
    OrderRequest { account: account.into(), symbol: sym(), side, kind: if price_value.is_some() { OrderKind::Limit } else { OrderKind::Market }, tif, price: price_value.map(price), quantity: qty(quantity), client_id: None }
}

#[test]
fn price_time_priority_is_deterministic_for_same_price() {
    let mut book = OrderBook::new(sym());
    let first = book.submit(order("maker-a", Side::Sell, TimeInForce::Gtc, Some(100), 1)).unwrap().order_id;
    let second = book.submit(order("maker-b", Side::Sell, TimeInForce::Gtc, Some(100), 1)).unwrap().order_id;
    let result = book.submit(order("taker", Side::Buy, TimeInForce::Ioc, Some(100), 1)).unwrap();
    assert_eq!(result.fills.len(), 1);
    assert_eq!(result.fills[0].maker_order, first);
    assert_ne!(result.fills[0].maker_order, second);
}

#[test]
fn market_and_ioc_orders_never_rest_remaining_quantity() {
    let mut book = OrderBook::new(sym());
    book.submit(order("maker", Side::Sell, TimeInForce::Gtc, Some(100), 1)).unwrap();
    let market = book.submit(order("market", Side::Buy, TimeInForce::Ioc, None, 5)).unwrap();
    assert_eq!(market.remaining.0.as_raw(), Fixed::from_units(4).as_raw());
    assert_eq!(book.snapshot(10).bids.len(), 0);
    let ioc = book.submit(order("ioc", Side::Buy, TimeInForce::Ioc, Some(100), 5)).unwrap();
    assert_eq!(ioc.status, "done");
    assert_eq!(book.snapshot(10).bids.len(), 0);
}

#[test]
fn cancel_cannot_remove_a_different_order() {
    let mut book = OrderBook::new(sym());
    let first = book.submit(order("maker-a", Side::Sell, TimeInForce::Gtc, Some(100), 1)).unwrap().order_id;
    let second = book.submit(order("maker-b", Side::Sell, TimeInForce::Gtc, Some(101), 1)).unwrap().order_id;
    assert!(book.cancel(&first).canceled);
    let snap = book.snapshot(10);
    assert_eq!(snap.asks.len(), 1);
    assert_eq!(book.cancel(&second).canceled, true);
}

#[test]
fn amend_cannot_increase_or_change_priority_fields() {
    let mut book = OrderBook::new(sym());
    let id = book.submit(order("maker", Side::Sell, TimeInForce::Gtc, Some(100), 5)).unwrap().order_id;
    assert!(!book.amend(&id, AmendRequest { quantity: Some(qty(6)), price: None, tif: None, client_id: None }).unwrap().amended);
    assert!(!book.amend(&id, AmendRequest { quantity: None, price: Some(price(101)), tif: None, client_id: None }).unwrap().amended);
    assert!(!book.amend(&id, AmendRequest { quantity: None, price: None, tif: Some(TimeInForce::Ioc), client_id: None }).unwrap().amended);
}

#[test]
fn market_status_modes_fail_closed() {
    let mut book = OrderBook::new(sym());
    let id = book.submit(order("maker", Side::Sell, TimeInForce::Gtc, Some(100), 1)).unwrap().order_id;
    book.set_status(MarketStatus::CancelOnly);
    assert!(book.submit(order("taker", Side::Buy, TimeInForce::Gtc, Some(99), 1)).is_err());
    assert!(book.cancel(&id).canceled);
    book.set_status(MarketStatus::Halted);
    assert!(book.submit(order("maker2", Side::Sell, TimeInForce::Gtc, Some(100), 1)).is_err());
}

#[test]
fn tick_lot_and_minimum_quantity_are_enforced() {
    let rules = MarketRules { min_quantity: qty(10), tick_size: Fixed::from_units(5), lot_size: Fixed::from_units(2), maker_fee_bps: 0, taker_fee_bps: 0 };
    let mut book = OrderBook::new(sym()).with_rules(rules);
    assert!(book.submit(order("bad-min", Side::Sell, TimeInForce::Gtc, Some(100), 1)).is_err());
    assert!(book.submit(order("bad-tick", Side::Sell, TimeInForce::Gtc, Some(101), 10)).is_err());
    assert!(book.submit(order("good", Side::Sell, TimeInForce::Gtc, Some(100), 10)).unwrap().accepted);
}
