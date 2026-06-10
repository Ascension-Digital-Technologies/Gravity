use gravity_types::{Fixed, GravityError, OrderKind, Price, Quantity, Side, Symbol, TimeInForce, now_ms, stable_hash_hex, BPS};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRequest {
    pub account: String,
    pub symbol: Symbol,
    pub side: Side,
    pub kind: OrderKind,
    pub tif: TimeInForce,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub client_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub account: String,
    pub symbol: Symbol,
    pub side: Side,
    pub kind: OrderKind,
    pub tif: TimeInForce,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub remaining: Quantity,
    pub client_id: Option<String>,
    pub created_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub id: String,
    pub symbol: Symbol,
    pub maker_order: String,
    pub taker_order: String,
    pub maker_account: String,
    pub taker_account: String,
    pub price: Price,
    pub quantity: Quantity,
    pub taker_side: Side,
    pub maker_fee_quote: Fixed,
    pub taker_fee_quote: Fixed,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderResult {
    pub accepted: bool,
    pub order_id: String,
    pub status: String,
    pub remaining: Quantity,
    pub fills: Vec<Fill>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookEventKind { OrderAccepted, OrderRejected, OrderCanceled, Fill, OrderAmended, OrderReplaced }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookEvent {
    pub kind: BookEventKind,
    pub symbol: Symbol,
    pub order_id: String,
    pub fill_id: Option<String>,
    pub price: Option<Price>,
    pub quantity: Option<Quantity>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelResult {
    pub canceled: bool,
    pub order_id: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendRequest {
    pub quantity: Option<Quantity>,
    pub price: Option<Price>,
    pub tif: Option<TimeInForce>,
    pub client_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmendResult {
    pub amended: bool,
    pub order_id: String,
    pub remaining: Option<Quantity>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceResult {
    pub canceled: CancelResult,
    pub replacement: Option<OrderResult>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketStatus { Open, CancelOnly, Halted }

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelfTradePrevention { Allow, RejectTaker }

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeeModel {
    pub maker_bps: i64,
    pub taker_bps: i64,
}

impl Default for FeeModel {
    fn default() -> Self { Self { maker_bps: 0, taker_bps: 0 } }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketRules {
    pub min_quantity: Quantity,
    pub tick_size: Fixed,
    pub lot_size: Fixed,
    pub maker_fee_bps: i64,
    pub taker_fee_bps: i64,
}

impl Default for MarketRules {
    fn default() -> Self {
        Self {
            min_quantity: Quantity(Fixed::raw(1)),
            tick_size: Fixed::raw(1),
            lot_size: Fixed::raw(1),
            maker_fee_bps: 0,
            taker_fee_bps: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookLevel {
    pub price: Price,
    pub quantity: Quantity,
    pub orders: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookSnapshot {
    pub symbol: Symbol,
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookStats {
    pub accepted: u64,
    pub rejected: u64,
    pub canceled: u64,
    pub amended: u64,
    pub replaced: u64,
    pub fills: u64,
    pub sequence: u64,
}

#[derive(Clone, Debug)]
pub struct OrderBook {
    symbol: Symbol,
    bids: BTreeMap<i128, VecDeque<Order>>,
    asks: BTreeMap<i128, VecDeque<Order>>,
    index: HashMap<String, OrderLocation>,
    snapshot_cache: RefCell<HashMap<usize, BookSnapshot>>,
    snapshot_dirty: Cell<bool>,
    stats: BookStats,
    rules: MarketRules,
    status: MarketStatus,
    stp: SelfTradePrevention,
    fees: FeeModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OrderLocation {
    side: Side,
    price_key: i128,
}

impl OrderBook {
    pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            index: HashMap::new(),
            snapshot_cache: RefCell::new(HashMap::new()),
            snapshot_dirty: Cell::new(true),
            stats: BookStats::default(),
            rules: MarketRules::default(),
            status: MarketStatus::Open,
            stp: SelfTradePrevention::RejectTaker,
            fees: FeeModel::default(),
        }
    }

    pub fn with_rules(mut self, rules: MarketRules) -> Self {
        self.rules = rules;
        self.fees = FeeModel { maker_bps: rules.maker_fee_bps, taker_bps: rules.taker_fee_bps };
        self
    }

    pub fn set_status(&mut self, status: MarketStatus) { self.status = status; }
    pub fn status(&self) -> MarketStatus { self.status }
    pub fn rules(&self) -> MarketRules { self.rules }
    pub fn stats(&self) -> BookStats { self.stats.clone() }

    pub fn open_orders(&self) -> usize {
        self.bids.values().map(VecDeque::len).sum::<usize>() + self.asks.values().map(VecDeque::len).sum::<usize>()
    }

    pub fn submit(&mut self, req: OrderRequest) -> Result<OrderResult, GravityError> {
        if self.status != MarketStatus::Open {
            self.stats.rejected += 1;
            return Err(GravityError::InvalidConfig(format!("market is {:?}; new orders are disabled", self.status)));
        }
        self.validate(&req)?;
        let order = self.order_from_request(req)?;
        if order.tif == TimeInForce::PostOnly && self.crosses(&order) {
            self.stats.rejected += 1;
            return Ok(OrderResult::reject(order.id, order.quantity, "post-only order would cross"));
        }
        if order.tif == TimeInForce::Fok && !self.can_fully_fill(&order) {
            self.stats.rejected += 1;
            return Ok(OrderResult::reject(order.id, order.quantity, "FOK order cannot fully fill"));
        }
        let result = self.execute(order)?;
        if result.accepted { self.stats.accepted += 1; }
        Ok(result)
    }

    pub fn cancel(&mut self, order_id: &str) -> CancelResult {
        if self.status == MarketStatus::Halted {
            return CancelResult { canceled: false, order_id: order_id.into(), message: "market halted".into() };
        }
        self.cancel_unchecked(order_id)
    }

    pub fn amend(&mut self, order_id: &str, amend: AmendRequest) -> Result<AmendResult, GravityError> {
        if self.status != MarketStatus::Open && self.status != MarketStatus::CancelOnly {
            return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: None, message: "market halted".into() });
        }
        if let Some(quantity) = amend.quantity { self.validate_quantity(quantity)?; }
        let Some(location) = self.index.get(order_id).copied() else {
            return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: None, message: "order not found".into() });
        };

        let remaining = {
            let queue = match location.side {
                Side::Buy => self.bids.get_mut(&location.price_key),
                Side::Sell => self.asks.get_mut(&location.price_key),
            };
            let Some(queue) = queue else {
                self.index.remove(order_id);
                return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: None, message: "order not found".into() });
            };
            let Some(order) = queue.iter_mut().find(|order| order.id == order_id) else {
                self.index.remove(order_id);
                return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: None, message: "order not found".into() });
            };
            if let Some(price) = amend.price {
                if order.price != Some(price) {
                    return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: Some(order.remaining), message: "price change requires cancel-replace".into() });
                }
            }
            if let Some(tif) = amend.tif {
                if tif != order.tif {
                    return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: Some(order.remaining), message: "time-in-force change requires cancel-replace".into() });
                }
            }
            if let Some(quantity) = amend.quantity {
                if quantity.0.as_raw() > order.remaining.0.as_raw() {
                    return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: Some(order.remaining), message: "amend may only reduce remaining quantity".into() });
                }
                order.remaining = quantity;
                order.quantity = quantity;
            }
            if amend.client_id.is_some() { order.client_id = amend.client_id; }
            order.remaining
        };

        self.stats.amended += 1;
        self.stats.sequence += 1;
        self.mark_dirty();
        Ok(AmendResult { amended: true, order_id: order_id.into(), remaining: Some(remaining), message: "amended".into() })
    }

    pub fn replace(&mut self, order_id: &str, replacement: OrderRequest) -> Result<ReplaceResult, GravityError> {
        if self.status != MarketStatus::Open {
            return Err(GravityError::InvalidConfig(format!("market is {:?}; replace disabled", self.status)));
        }
        let canceled = self.cancel_unchecked(order_id);
        if !canceled.canceled { return Ok(ReplaceResult { canceled, replacement: None }); }
        let replacement = Some(self.submit(replacement)?);
        self.stats.replaced += 1;
        Ok(ReplaceResult { canceled, replacement })
    }

    pub fn snapshot(&self, depth: usize) -> BookSnapshot {
        let depth = depth.max(1);
        if !self.snapshot_dirty.get() {
            if let Some(cached) = self.snapshot_cache.borrow().get(&depth) { return cached.clone(); }
        }
        if self.snapshot_dirty.get() { self.snapshot_cache.borrow_mut().clear(); }
        let bids = self.bids.iter().rev().take(depth).filter_map(|(price, q)| Self::level(*price, q)).collect();
        let asks = self.asks.iter().take(depth).filter_map(|(price, q)| Self::level(*price, q)).collect();
        let snapshot = BookSnapshot { symbol: self.symbol.clone(), bids, asks, sequence: self.stats.sequence, timestamp_ms: now_ms() };
        self.snapshot_cache.borrow_mut().insert(depth, snapshot.clone());
        self.snapshot_dirty.set(false);
        snapshot
    }

    fn validate(&self, req: &OrderRequest) -> Result<(), GravityError> {
        if req.account.trim().is_empty() { return Err(GravityError::InvalidConfig("order account is required".into())); }
        if req.symbol != self.symbol { return Err(GravityError::InvalidSymbol(req.symbol.0.clone())); }
        if req.kind == OrderKind::Limit && req.price.is_none() { return Err(GravityError::InvalidPrice); }
        if req.kind == OrderKind::Market && req.tif == TimeInForce::PostOnly { return Err(GravityError::InvalidConfig("market orders cannot be post-only".into())); }
        if let Some(price) = req.price { self.validate_tick(price)?; }
        self.validate_quantity(req.quantity)?;
        Ok(())
    }

    fn validate_tick(&self, price: Price) -> Result<(), GravityError> {
        let tick = self.rules.tick_size.as_raw();
        if tick > 0 && price.0.as_raw() % tick != 0 { return Err(GravityError::InvalidConfig("price is not aligned to market tick size".into())); }
        Ok(())
    }

    fn validate_quantity(&self, quantity: Quantity) -> Result<(), GravityError> {
        if quantity.0.as_raw() < self.rules.min_quantity.0.as_raw() { return Err(GravityError::InvalidQuantity); }
        let lot = self.rules.lot_size.as_raw();
        if lot > 0 && quantity.0.as_raw() % lot != 0 { return Err(GravityError::InvalidConfig("quantity is not aligned to market lot size".into())); }
        Ok(())
    }

    fn order_from_request(&mut self, req: OrderRequest) -> Result<Order, GravityError> {
        self.stats.sequence += 1;
        let seed = format!("{}:{}:{}:{}:{}", req.account, req.symbol, req.side.as_str(), self.stats.sequence, now_ms());
        Ok(Order {
            id: stable_hash_hex(&seed),
            account: req.account,
            symbol: req.symbol,
            side: req.side,
            kind: req.kind,
            tif: req.tif,
            price: req.price,
            quantity: req.quantity,
            remaining: req.quantity,
            client_id: req.client_id,
            created_ms: now_ms(),
        })
    }

    fn crosses(&self, order: &Order) -> bool {
        match order.side {
            Side::Buy => self.best_ask().is_some_and(|ask| order.price.map_or(true, |p| p >= ask)),
            Side::Sell => self.best_bid().is_some_and(|bid| order.price.map_or(true, |p| p <= bid)),
        }
    }

    fn can_fully_fill(&self, order: &Order) -> bool {
        let mut need = order.remaining.0;
        match order.side {
            Side::Buy => {
                for (price, queue) in self.asks.iter() {
                    let p = Price(Fixed::raw(*price));
                    if !Self::can_take(order, p) { break; }
                    for resting in queue { need = need - resting.remaining.0; if need.0 <= 0 { return true; } }
                }
            }
            Side::Sell => {
                for (price, queue) in self.bids.iter().rev() {
                    let p = Price(Fixed::raw(*price));
                    if !Self::can_take(order, p) { break; }
                    for resting in queue { need = need - resting.remaining.0; if need.0 <= 0 { return true; } }
                }
            }
        }
        false
    }

    fn execute(&mut self, mut taker: Order) -> Result<OrderResult, GravityError> {
        let mut fills = Vec::new();
        match taker.side {
            Side::Buy => self.match_against_asks(&mut taker, &mut fills)?,
            Side::Sell => self.match_against_bids(&mut taker, &mut fills)?,
        }
        self.stats.fills += fills.len() as u64;
        let placeable = taker.kind == OrderKind::Limit && taker.remaining.0.as_raw() > 0 && matches!(taker.tif, TimeInForce::Gtc | TimeInForce::PostOnly);
        if placeable { self.insert(taker.clone()); }
        let status = if taker.remaining.0.as_raw() == 0 { "filled" } else if placeable { "open" } else if fills.is_empty() { "expired" } else { "partial" };
        Ok(OrderResult { accepted: true, order_id: taker.id, status: status.into(), remaining: taker.remaining, fills, message: "accepted".into() })
    }

    fn match_against_asks(&mut self, taker: &mut Order, fills: &mut Vec<Fill>) -> Result<(), GravityError> {
        while taker.remaining.0.as_raw() > 0 {
            let Some(price_key) = self.asks.keys().next().copied() else { break; };
            let price = Price(Fixed::raw(price_key));
            if !Self::can_take(taker, price) { break; }
            let remove_level = self.fill_level(price_key, taker, fills)?;
            if remove_level { self.asks.remove(&price_key); }
        }
        Ok(())
    }

    fn match_against_bids(&mut self, taker: &mut Order, fills: &mut Vec<Fill>) -> Result<(), GravityError> {
        while taker.remaining.0.as_raw() > 0 {
            let Some(price_key) = self.bids.keys().next_back().copied() else { break; };
            let price = Price(Fixed::raw(price_key));
            if !Self::can_take(taker, price) { break; }
            let remove_level = self.fill_level(price_key, taker, fills)?;
            if remove_level { self.bids.remove(&price_key); }
        }
        Ok(())
    }

    fn fill_level(&mut self, price_key: i128, taker: &mut Order, fills: &mut Vec<Fill>) -> Result<bool, GravityError> {
        let stp = self.stp;
        let fees = self.fees;
        let queue = match taker.side { Side::Buy => self.asks.get_mut(&price_key), Side::Sell => self.bids.get_mut(&price_key) };
        let Some(queue) = queue else { return Ok(true); };
        let mut touched = false;
        while taker.remaining.0.as_raw() > 0 {
            let Some(resting) = queue.front_mut() else { break; };
            if stp == SelfTradePrevention::RejectTaker && taker.account == resting.account {
                return Err(GravityError::InvalidConfig("self-trade prevention rejected taker".into()));
            }
            let qty_raw = taker.remaining.0.as_raw().min(resting.remaining.0.as_raw());
            let qty = Quantity::new(Fixed::raw(qty_raw))?;
            taker.remaining = Quantity(Fixed::raw(taker.remaining.0.as_raw() - qty_raw));
            resting.remaining = Quantity(Fixed::raw(resting.remaining.0.as_raw() - qty_raw));
            let notional = Price(Fixed::raw(price_key)).0.checked_mul(qty.0)?;
            let maker_fee_quote = fee_from_notional(notional, fees.maker_bps)?;
            let taker_fee_quote = fee_from_notional(notional, fees.taker_bps)?;
            let fill_id = stable_hash_hex(&format!("{}:{}:{}:{}", taker.id, resting.id, price_key, fills.len()));
            fills.push(Fill {
                id: fill_id,
                symbol: self.symbol.clone(),
                maker_order: resting.id.clone(),
                taker_order: taker.id.clone(),
                maker_account: resting.account.clone(),
                taker_account: taker.account.clone(),
                price: Price(Fixed::raw(price_key)),
                quantity: qty,
                taker_side: taker.side,
                maker_fee_quote,
                taker_fee_quote,
                timestamp_ms: now_ms(),
            });
            self.stats.sequence += 1;
            touched = true;
            if resting.remaining.0.as_raw() == 0 {
                if let Some(done) = queue.pop_front() { self.index.remove(&done.id); }
            }
        }
        let empty = queue.is_empty();
        if touched { self.mark_dirty(); }
        Ok(empty)
    }

    fn can_take(taker: &Order, resting_price: Price) -> bool {
        match taker.kind {
            OrderKind::Market => true,
            OrderKind::Limit => match taker.side {
                Side::Buy => taker.price.is_some_and(|p| p >= resting_price),
                Side::Sell => taker.price.is_some_and(|p| p <= resting_price),
            },
        }
    }

    fn insert(&mut self, order: Order) {
        let Some(price) = order.price else { return; };
        let side = order.side;
        let key = price.0.as_raw();
        self.index.insert(order.id.clone(), OrderLocation { side, price_key: key });
        match side {
            Side::Buy => self.bids.entry(key).or_default().push_back(order),
            Side::Sell => self.asks.entry(key).or_default().push_back(order),
        }
        self.stats.sequence += 1;
        self.mark_dirty();
    }

    fn cancel_unchecked(&mut self, order_id: &str) -> CancelResult {
        if let Some(location) = self.index.remove(order_id) {
            let removed = match location.side {
                Side::Buy => Self::cancel_at(&mut self.bids, location.price_key, order_id),
                Side::Sell => Self::cancel_at(&mut self.asks, location.price_key, order_id),
            };
            if removed {
                self.stats.canceled += 1;
                self.stats.sequence += 1;
                self.mark_dirty();
                return CancelResult { canceled: true, order_id: order_id.into(), message: "canceled".into() };
            }
        }
        CancelResult { canceled: false, order_id: order_id.into(), message: "order not found".into() }
    }

    fn best_bid(&self) -> Option<Price> { self.bids.keys().next_back().copied().map(|v| Price(Fixed::raw(v))) }
    fn best_ask(&self) -> Option<Price> { self.asks.keys().next().copied().map(|v| Price(Fixed::raw(v))) }

    fn level(price: i128, queue: &VecDeque<Order>) -> Option<BookLevel> {
        let mut qty = Fixed::ZERO;
        for order in queue { qty = qty + order.remaining.0; }
        if qty.as_raw() <= 0 { return None; }
        Some(BookLevel { price: Price(Fixed::raw(price)), quantity: Quantity(qty), orders: queue.len() })
    }

    fn cancel_at(book: &mut BTreeMap<i128, VecDeque<Order>>, price_key: i128, order_id: &str) -> bool {
        let Some(queue) = book.get_mut(&price_key) else { return false; };
        let Some(pos) = queue.iter().position(|v| v.id == order_id) else { return false; };
        queue.remove(pos);
        if queue.is_empty() { book.remove(&price_key); }
        true
    }

    fn mark_dirty(&self) { self.snapshot_dirty.set(true); }
}

fn fee_from_notional(notional: Fixed, bps: i64) -> Result<Fixed, GravityError> {
    if bps == 0 { return Ok(Fixed::ZERO); }
    let raw = notional.as_raw().checked_mul(bps as i128).and_then(|v| v.checked_div(BPS)).ok_or(GravityError::Overflow)?;
    Ok(Fixed::raw(raw))
}

impl OrderResult {
    fn reject(order_id: String, remaining: Quantity, msg: &str) -> Self {
        Self { accepted: false, order_id, status: "rejected".into(), remaining, fills: Vec::new(), message: msg.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
    fn qty(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }
    fn sym() -> Symbol { Symbol::new("BTC-USDx").unwrap() }

    #[test]
    fn matches_price_time_priority() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(2), client_id: None };
        assert_eq!(book.submit(sell).unwrap().status, "open");
        let buy = OrderRequest { account: "taker".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Ioc, price: Some(price(101)), quantity: qty(1), client_id: None };
        let result = book.submit(buy).unwrap();
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.remaining.0.as_raw(), 0);
        assert_eq!(book.snapshot(5).asks[0].quantity.0.to_string(), "1");
    }

    #[test]
    fn post_only_rejects_crossing_order() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "a".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        book.submit(sell).unwrap();
        let buy = OrderRequest { account: "b".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::PostOnly, price: Some(price(100)), quantity: qty(1), client_id: None };
        assert!(!book.submit(buy).unwrap().accepted);
    }

    #[test]
    fn self_trade_is_rejected() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "same".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        book.submit(sell).unwrap();
        let buy = OrderRequest { account: "same".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Ioc, price: Some(price(100)), quantity: qty(1), client_id: None };
        assert!(book.submit(buy).is_err());
    }

    #[test]
    fn amend_can_reduce_quantity_without_losing_priority() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(5), client_id: None };
        let order_id = book.submit(sell).unwrap().order_id;
        let result = book.amend(&order_id, AmendRequest { quantity: Some(qty(2)), price: None, tif: None, client_id: Some("amended".into()) }).unwrap();
        assert!(result.amended);
        assert_eq!(book.snapshot(5).asks[0].quantity.0.to_string(), "2");
    }

    #[test]
    fn fok_rejects_when_full_fill_is_not_available() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        book.submit(sell).unwrap();
        let buy = OrderRequest { account: "taker".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Fok, price: Some(price(100)), quantity: qty(2), client_id: None };
        let result = book.submit(buy).unwrap();
        assert!(!result.accepted);
        assert_eq!(book.snapshot(5).asks[0].quantity.0.to_string(), "1");
    }

    #[test]
    fn cancel_only_allows_cancel_but_rejects_new_orders() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        let order_id = book.submit(sell).unwrap().order_id;
        book.set_status(MarketStatus::CancelOnly);
        let buy = OrderRequest { account: "taker".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(99)), quantity: qty(1), client_id: None };
        assert!(book.submit(buy).is_err());
        assert!(book.cancel(&order_id).canceled);
    }

    #[test]
    fn halted_market_rejects_cancel() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        let order_id = book.submit(sell).unwrap().order_id;
        book.set_status(MarketStatus::Halted);
        let result = book.cancel(&order_id);
        assert!(!result.canceled);
    }

    #[test]
    fn market_rules_enforce_tick_lot_and_min_size() {
        let rules = MarketRules {
            min_quantity: qty(2),
            tick_size: Fixed::from_units(5),
            lot_size: Fixed::from_units(2),
            maker_fee_bps: 1,
            taker_fee_bps: 2,
        };
        let mut book = OrderBook::new(sym()).with_rules(rules);
        let bad_tick = OrderRequest { account: "a".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(101)), quantity: qty(2), client_id: None };
        assert!(book.submit(bad_tick).is_err());
        let bad_lot = OrderRequest { account: "a".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(3), client_id: None };
        assert!(book.submit(bad_lot).is_err());
        let good = OrderRequest { account: "a".into(), symbol: sym(), side: Side::Buy, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(4), client_id: None };
        assert!(book.submit(good).unwrap().accepted);
    }

    #[test]
    fn replace_cancels_old_order_and_places_replacement() {
        let mut book = OrderBook::new(sym());
        let sell = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(100)), quantity: qty(1), client_id: None };
        let order_id = book.submit(sell).unwrap().order_id;
        let replacement = OrderRequest { account: "maker".into(), symbol: sym(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Gtc, price: Some(price(105)), quantity: qty(1), client_id: Some("replace".into()) };
        let result = book.replace(&order_id, replacement).unwrap();
        assert!(result.canceled.canceled);
        assert!(result.replacement.unwrap().accepted);
        assert_eq!(book.snapshot(5).asks[0].price.0.to_string(), "105");
    }


    fn limit_req(account: &str, side: Side, price_value: i128, quantity_value: i128, tif: TimeInForce) -> OrderRequest {
        OrderRequest {
            account: account.into(),
            symbol: sym(),
            side,
            kind: OrderKind::Limit,
            tif,
            price: Some(price(price_value)),
            quantity: qty(quantity_value),
            client_id: None,
        }
    }

    fn market_req(account: &str, side: Side, quantity_value: i128, tif: TimeInForce) -> OrderRequest {
        OrderRequest {
            account: account.into(),
            symbol: sym(),
            side,
            kind: OrderKind::Market,
            tif,
            price: None,
            quantity: qty(quantity_value),
            client_id: None,
        }
    }

    #[test]
    fn fifo_priority_is_preserved_inside_same_price_level() {
        let mut book = OrderBook::new(sym());
        let maker_a = book.submit(limit_req("maker-a", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap().order_id;
        let maker_b = book.submit(limit_req("maker-b", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap().order_id;
        let result = book.submit(limit_req("taker", Side::Buy, 100, 2, TimeInForce::Ioc)).unwrap();
        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.fills[0].maker_order, maker_a);
        assert_eq!(result.fills[1].maker_order, maker_b);
        assert_eq!(result.fills[0].maker_account, "maker-a");
        assert_eq!(result.fills[1].maker_account, "maker-b");
    }

    #[test]
    fn market_order_never_rests_when_liquidity_is_missing() {
        let mut book = OrderBook::new(sym());
        let result = book.submit(market_req("taker", Side::Buy, 5, TimeInForce::Ioc)).unwrap();
        assert!(result.accepted);
        assert_eq!(result.status, "expired");
        assert_eq!(result.fills.len(), 0);
        assert_eq!(book.open_orders(), 0);
    }

    #[test]
    fn ioc_partial_fill_does_not_rest_remaining_quantity() {
        let mut book = OrderBook::new(sym());
        book.submit(limit_req("maker", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap();
        let result = book.submit(limit_req("taker", Side::Buy, 100, 3, TimeInForce::Ioc)).unwrap();
        assert_eq!(result.status, "partial");
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.remaining.0.to_string(), "2");
        assert_eq!(book.open_orders(), 0);
    }

    #[test]
    fn fok_full_fill_executes_without_resting_taker() {
        let mut book = OrderBook::new(sym());
        book.submit(limit_req("maker-a", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap();
        book.submit(limit_req("maker-b", Side::Sell, 101, 1, TimeInForce::Gtc)).unwrap();
        let result = book.submit(limit_req("taker", Side::Buy, 101, 2, TimeInForce::Fok)).unwrap();
        assert!(result.accepted);
        assert_eq!(result.status, "filled");
        assert_eq!(result.fills.len(), 2);
        assert_eq!(result.remaining.0.as_raw(), 0);
        assert_eq!(book.open_orders(), 0);
    }

    #[test]
    fn post_only_non_crossing_order_rests() {
        let mut book = OrderBook::new(sym());
        book.submit(limit_req("maker", Side::Sell, 105, 1, TimeInForce::Gtc)).unwrap();
        let result = book.submit(limit_req("post", Side::Buy, 100, 1, TimeInForce::PostOnly)).unwrap();
        assert!(result.accepted);
        assert_eq!(result.status, "open");
        assert_eq!(book.snapshot(5).bids[0].price.0.to_string(), "100");
    }

    #[test]
    fn amend_rejects_quantity_increase_and_keeps_original_quantity() {
        let mut book = OrderBook::new(sym());
        let order_id = book.submit(limit_req("maker", Side::Sell, 100, 2, TimeInForce::Gtc)).unwrap().order_id;
        let result = book.amend(&order_id, AmendRequest { quantity: Some(qty(3)), price: None, tif: None, client_id: None }).unwrap();
        assert!(!result.amended);
        assert_eq!(book.snapshot(5).asks[0].quantity.0.to_string(), "2");
    }

    #[test]
    fn amend_rejects_price_and_tif_changes() {
        let mut book = OrderBook::new(sym());
        let order_id = book.submit(limit_req("maker", Side::Sell, 100, 2, TimeInForce::Gtc)).unwrap().order_id;
        let price_change = book.amend(&order_id, AmendRequest { quantity: None, price: Some(price(101)), tif: None, client_id: None }).unwrap();
        assert!(!price_change.amended);
        assert!(price_change.message.contains("price change"));
        let tif_change = book.amend(&order_id, AmendRequest { quantity: None, price: None, tif: Some(TimeInForce::Ioc), client_id: None }).unwrap();
        assert!(!tif_change.amended);
        assert!(tif_change.message.contains("time-in-force"));
        assert_eq!(book.snapshot(5).asks[0].price.0.to_string(), "100");
    }

    #[test]
    fn cancel_missing_order_is_noop() {
        let mut book = OrderBook::new(sym());
        book.submit(limit_req("maker", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap();
        let before = book.stats();
        let result = book.cancel("missing-order");
        let after = book.stats();
        assert!(!result.canceled);
        assert_eq!(before.canceled, after.canceled);
        assert_eq!(book.open_orders(), 1);
    }

    #[test]
    fn snapshot_cache_invalidates_after_cancel() {
        let mut book = OrderBook::new(sym());
        let order_id = book.submit(limit_req("maker", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap().order_id;
        let first = book.snapshot(5);
        assert_eq!(first.asks.len(), 1);
        assert!(book.cancel(&order_id).canceled);
        let second = book.snapshot(5);
        assert!(second.asks.is_empty());
        assert!(second.sequence > first.sequence);
    }

    #[test]
    fn replace_missing_order_does_not_submit_replacement() {
        let mut book = OrderBook::new(sym());
        let replacement = limit_req("maker", Side::Sell, 100, 1, TimeInForce::Gtc);
        let result = book.replace("missing-order", replacement).unwrap();
        assert!(!result.canceled.canceled);
        assert!(result.replacement.is_none());
        assert_eq!(book.open_orders(), 0);
    }

    #[test]
    fn halted_market_rejects_replace() {
        let mut book = OrderBook::new(sym());
        let order_id = book.submit(limit_req("maker", Side::Sell, 100, 1, TimeInForce::Gtc)).unwrap().order_id;
        book.set_status(MarketStatus::Halted);
        let replacement = limit_req("maker", Side::Sell, 101, 1, TimeInForce::Gtc);
        assert!(book.replace(&order_id, replacement).is_err());
        assert_eq!(book.open_orders(), 1);
    }

    #[test]
    fn maker_and_taker_fees_are_recorded_on_fill() {
        let rules = MarketRules {
            min_quantity: qty(1),
            tick_size: Fixed::from_units(1),
            lot_size: Fixed::from_units(1),
            maker_fee_bps: 10,
            taker_fee_bps: 20,
        };
        let mut book = OrderBook::new(sym()).with_rules(rules);
        book.submit(limit_req("maker", Side::Sell, 100, 2, TimeInForce::Gtc)).unwrap();
        let result = book.submit(limit_req("taker", Side::Buy, 100, 2, TimeInForce::Ioc)).unwrap();
        assert_eq!(result.fills.len(), 1);
        assert_eq!(result.fills[0].maker_fee_quote.to_string(), "0.2");
        assert_eq!(result.fills[0].taker_fee_quote.to_string(), "0.4");
    }

    #[test]
    fn market_rules_reject_quantity_below_minimum() {
        let rules = MarketRules {
            min_quantity: qty(5),
            tick_size: Fixed::from_units(1),
            lot_size: Fixed::from_units(1),
            maker_fee_bps: 0,
            taker_fee_bps: 0,
        };
        let mut book = OrderBook::new(sym()).with_rules(rules);
        let small = limit_req("small", Side::Buy, 100, 4, TimeInForce::Gtc);
        assert!(book.submit(small).is_err());
        assert_eq!(book.open_orders(), 0);
    }

}
