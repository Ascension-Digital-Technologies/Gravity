use gravity_book::Fill;
use gravity_settlement::CompressedSettlementBatch;
use gravity_types::{Fixed, Price, Quantity, Side, Symbol};

fn p(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }
fn sym() -> Symbol { Symbol::new("BTC-USDx").unwrap() }
fn fill(id: &str, maker: &str, taker: &str, taker_side: Side, qty_value: i128, price_value: i128) -> Fill {
    Fill { id: id.into(), symbol: sym(), maker_order: format!("maker-{id}"), taker_order: format!("taker-{id}"), maker_account: maker.into(), taker_account: taker.into(), price: p(price_value), quantity: q(qty_value), taker_side, maker_fee_quote: Fixed::ZERO, taker_fee_quote: Fixed::ZERO, timestamp_ms: 100 }
}

#[test]
fn compressed_batches_are_idempotent_for_same_fills() {
    let fills = vec![fill("a", "maker", "taker", Side::Buy, 1, 100), fill("b", "maker", "taker", Side::Buy, 2, 100)];
    let a = CompressedSettlementBatch::from_fills(sym(), &fills);
    let b = CompressedSettlementBatch::from_fills(sym(), &fills);
    assert_eq!(a.fills_root, b.fills_root);
    assert_eq!(a.idempotency_key(), b.idempotency_key());
    assert_eq!(a.deltas.len(), b.deltas.len());
}

#[test]
fn settlement_deltas_conserve_base_quantity_between_accounts() {
    let fills = vec![fill("a", "maker", "taker", Side::Buy, 3, 100)];
    let batch = CompressedSettlementBatch::from_fills(sym(), &fills);
    let bought: i128 = batch.deltas.iter().map(|d| d.bought_raw).sum();
    let sold: i128 = batch.deltas.iter().map(|d| d.sold_raw).sum();
    assert_eq!(bought, sold);
}

#[test]
fn settlement_payload_contains_stargate_instruction_body() {
    let fills = vec![fill("a", "maker", "taker", Side::Buy, 1, 100)];
    let payload = CompressedSettlementBatch::from_fills(sym(), &fills).to_payload().unwrap();
    assert_eq!(payload.kind, "SettleCompressedTrades");
    assert!(payload.body.contains("payload_root"));
}
