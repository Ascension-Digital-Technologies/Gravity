use gravity_types::{Fixed, GravityError, Price, Quantity, Symbol, now_ms};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerpSide { Long, Short }

impl PerpSide {
    pub fn parse(value: &str) -> Result<Self, GravityError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "long" | "buy" => Ok(Self::Long),
            "short" | "sell" => Ok(Self::Short),
            other => Err(GravityError::InvalidConfig(format!("unsupported perp side: {other}"))),
        }
    }

    pub fn as_str(self) -> &'static str { match self { Self::Long => "long", Self::Short => "short" } }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpMarketConfig {
    pub symbol: Symbol,
    pub index_symbol: Symbol,
    pub initial_margin_bps: u32,
    pub maintenance_margin_bps: u32,
    pub max_leverage_bps: u32,
    pub funding_interval_ms: u64,
    pub maker_fee_bps: i64,
    pub taker_fee_bps: i64,
    pub insurance_fund_bps: u32,
}

impl PerpMarketConfig {
    pub fn new(symbol: Symbol, index_symbol: Symbol) -> Self {
        Self {
            symbol,
            index_symbol,
            initial_margin_bps: 1_000,
            maintenance_margin_bps: 500,
            max_leverage_bps: 100_000,
            funding_interval_ms: 3_600_000,
            maker_fee_bps: 0,
            taker_fee_bps: 5,
            insurance_fund_bps: 1_000,
        }
    }

    pub fn validate(&self) -> Result<(), GravityError> {
        if self.initial_margin_bps == 0 || self.initial_margin_bps > 10_000 { return Err(GravityError::InvalidConfig("initial margin must be 1..=10000 bps".into())); }
        if self.maintenance_margin_bps == 0 || self.maintenance_margin_bps > self.initial_margin_bps { return Err(GravityError::InvalidConfig("maintenance margin must be >0 and <= initial margin".into())); }
        if self.max_leverage_bps == 0 { return Err(GravityError::InvalidConfig("max leverage must be positive".into())); }
        if self.funding_interval_ms == 0 { return Err(GravityError::InvalidConfig("funding interval must be positive".into())); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpPositionRequest {
    pub account: String,
    pub symbol: Symbol,
    pub side: PerpSide,
    pub quantity: Quantity,
    pub entry_price: Price,
    pub collateral: Fixed,
    pub leverage_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingUpdateRequest {
    pub symbol: Symbol,
    pub index_price: Price,
    pub mark_price: Price,
    pub funding_rate_bps: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpPosition {
    pub id: String,
    pub account: String,
    pub symbol: Symbol,
    pub side: PerpSide,
    pub quantity: Quantity,
    pub entry_price: Price,
    pub mark_price: Price,
    pub collateral: Fixed,
    pub notional: Fixed,
    pub unrealized_pnl: Fixed,
    pub margin_requirement: Fixed,
    pub maintenance_requirement: Fixed,
    pub equity: Fixed,
    pub leverage_bps: u32,
    pub liquidation_price: Option<Price>,
    pub opened_ms: u64,
    pub updated_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpMarketSnapshot {
    pub symbol: Symbol,
    pub index_symbol: Symbol,
    pub index_price: Option<Price>,
    pub mark_price: Option<Price>,
    pub open_interest: Quantity,
    pub long_open_interest: Quantity,
    pub short_open_interest: Quantity,
    pub funding_rate_bps: i64,
    pub next_funding_ms: u64,
    pub position_count: usize,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpEvent {
    pub kind: String,
    pub account: Option<String>,
    pub symbol: Symbol,
    pub position_id: Option<String>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerpStats {
    pub markets: usize,
    pub positions: usize,
    pub events: usize,
    pub sequence: u64,
    pub funding_updates: u64,
    pub opened_positions: u64,
}

struct PerpMarket {
    config: PerpMarketConfig,
    index_price: Option<Price>,
    mark_price: Option<Price>,
    open_interest_raw: i128,
    long_interest_raw: i128,
    short_interest_raw: i128,
    funding_rate_bps: i64,
    next_funding_ms: u64,
}

impl PerpMarket {
    fn snapshot(&self, position_count: usize, sequence: u64) -> Result<PerpMarketSnapshot, GravityError> {
        Ok(PerpMarketSnapshot {
            symbol: self.config.symbol.clone(),
            index_symbol: self.config.index_symbol.clone(),
            index_price: self.index_price,
            mark_price: self.mark_price,
            open_interest: Quantity::new(Fixed::raw(self.open_interest_raw.max(1)))?,
            long_open_interest: Quantity::new(Fixed::raw(self.long_interest_raw.max(1)))?,
            short_open_interest: Quantity::new(Fixed::raw(self.short_interest_raw.max(1)))?,
            funding_rate_bps: self.funding_rate_bps,
            next_funding_ms: self.next_funding_ms,
            position_count,
            sequence,
            timestamp_ms: now_ms(),
        })
    }
}

#[derive(Default)]
pub struct PerpEngine {
    markets: BTreeMap<String, PerpMarket>,
    positions: BTreeMap<String, PerpPosition>,
    account_index: BTreeMap<String, Vec<String>>,
    events: VecDeque<PerpEvent>,
    event_limit: usize,
    sequence: u64,
    funding_updates: u64,
    opened_positions: u64,
}

impl PerpEngine {
    pub fn new() -> Self { Self { event_limit: 100_000, ..Self::default() } }

    pub fn create_market(&mut self, config: PerpMarketConfig) -> Result<PerpMarketSnapshot, GravityError> {
        config.validate()?;
        let key = config.symbol.0.clone();
        self.sequence = self.sequence.saturating_add(1);
        let market = PerpMarket {
            next_funding_ms: now_ms().saturating_add(config.funding_interval_ms),
            config,
            index_price: None,
            mark_price: None,
            open_interest_raw: 0,
            long_interest_raw: 0,
            short_interest_raw: 0,
            funding_rate_bps: 0,
        };
        self.markets.insert(key.clone(), market);
        let snapshot = self.market_snapshot(&key)?.ok_or_else(|| GravityError::NotFound(key.clone()))?;
        self.push_event("market_created", None, snapshot.symbol.clone(), None, "perp market created");
        Ok(snapshot)
    }

    pub fn markets(&self) -> Result<Vec<PerpMarketSnapshot>, GravityError> {
        let mut out = Vec::with_capacity(self.markets.len());
        for key in self.markets.keys() {
            if let Some(snapshot) = self.market_snapshot(key)? { out.push(snapshot); }
        }
        Ok(out)
    }

    pub fn market_snapshot(&self, symbol: &str) -> Result<Option<PerpMarketSnapshot>, GravityError> {
        let Some(market) = self.markets.get(symbol) else { return Ok(None); };
        let positions = self.positions.values().filter(|p| p.symbol.0 == symbol).count();
        Ok(Some(market.snapshot(positions, self.sequence)?))
    }

    pub fn open_position(&mut self, request: PerpPositionRequest) -> Result<PerpPosition, GravityError> {
        if request.account.trim().is_empty() { return Err(GravityError::InvalidConfig("account is required".into())); }
        let key = request.symbol.0.clone();
        let market = self.markets.get_mut(&key).ok_or_else(|| GravityError::NotFound(key.clone()))?;
        let max_leverage = market.config.max_leverage_bps;
        if request.leverage_bps == 0 || request.leverage_bps > max_leverage { return Err(GravityError::InvalidConfig(format!("leverage must be 1..={max_leverage} bps"))); }
        let notional = request.quantity.0.checked_mul(request.entry_price.0)?;
        let initial_requirement = bps_mul(notional, market.config.initial_margin_bps)?;
        if request.collateral.0 < initial_requirement.0 { return Err(GravityError::InvalidConfig("insufficient collateral for initial margin".into())); }
        self.sequence = self.sequence.saturating_add(1);
        let now = now_ms();
        let id = format!("perp-{}-{}", key, self.sequence);
        let liq = liquidation_price(request.side, request.entry_price, request.collateral, request.quantity, market.config.maintenance_margin_bps).ok();
        let position = PerpPosition {
            id: id.clone(),
            account: request.account.clone(),
            symbol: request.symbol.clone(),
            side: request.side,
            quantity: request.quantity,
            entry_price: request.entry_price,
            mark_price: market.mark_price.unwrap_or(request.entry_price),
            collateral: request.collateral,
            notional,
            unrealized_pnl: Fixed::ZERO,
            margin_requirement: initial_requirement,
            maintenance_requirement: bps_mul(notional, market.config.maintenance_margin_bps)?,
            equity: request.collateral,
            leverage_bps: request.leverage_bps,
            liquidation_price: liq,
            opened_ms: now,
            updated_ms: now,
        };
        let qty_raw = request.quantity.0.as_raw();
        market.open_interest_raw = market.open_interest_raw.saturating_add(qty_raw);
        match request.side {
            PerpSide::Long => market.long_interest_raw = market.long_interest_raw.saturating_add(qty_raw),
            PerpSide::Short => market.short_interest_raw = market.short_interest_raw.saturating_add(qty_raw),
        }
        self.positions.insert(id.clone(), position.clone());
        self.account_index.entry(request.account.clone()).or_default().push(id.clone());
        self.opened_positions = self.opened_positions.saturating_add(1);
        self.push_event("position_opened", Some(request.account), request.symbol, Some(id), "perp position opened");
        Ok(position)
    }

    pub fn update_funding(&mut self, request: FundingUpdateRequest) -> Result<PerpMarketSnapshot, GravityError> {
        let key = request.symbol.0.clone();
        let market = self.markets.get_mut(&key).ok_or_else(|| GravityError::NotFound(key.clone()))?;
        self.sequence = self.sequence.saturating_add(1);
        market.index_price = Some(request.index_price);
        market.mark_price = Some(request.mark_price);
        market.funding_rate_bps = request.funding_rate_bps;
        market.next_funding_ms = now_ms().saturating_add(market.config.funding_interval_ms);
        self.funding_updates = self.funding_updates.saturating_add(1);
        for position in self.positions.values_mut().filter(|p| p.symbol.0 == key) {
            refresh_position(position, request.mark_price, market.config.maintenance_margin_bps, market.config.initial_margin_bps)?;
        }
        let snapshot = self.market_snapshot(&key)?.ok_or_else(|| GravityError::NotFound(key.clone()))?;
        self.push_event("funding_updated", None, snapshot.symbol.clone(), None, "funding and mark price updated");
        Ok(snapshot)
    }

    pub fn positions_for_account(&self, account: &str) -> Vec<PerpPosition> {
        let mut out = Vec::new();
        if let Some(ids) = self.account_index.get(account) {
            for id in ids {
                if let Some(position) = self.positions.get(id) { out.push(position.clone()); }
            }
        }
        out
    }

    pub fn all_positions(&self, limit: usize) -> Vec<PerpPosition> {
        self.positions.values().take(limit.min(10_000)).cloned().collect()
    }

    pub fn events(&self, limit: usize) -> Vec<PerpEvent> {
        let mut out = self.events.iter().rev().take(limit.min(10_000)).cloned().collect::<Vec<_>>();
        out.reverse();
        out
    }

    pub fn stats(&self) -> PerpStats {
        PerpStats { markets: self.markets.len(), positions: self.positions.len(), events: self.events.len(), sequence: self.sequence, funding_updates: self.funding_updates, opened_positions: self.opened_positions }
    }

    fn push_event(&mut self, kind: impl Into<String>, account: Option<String>, symbol: Symbol, position_id: Option<String>, message: impl Into<String>) {
        if self.event_limit == 0 { self.event_limit = 100_000; }
        if self.events.len() >= self.event_limit { self.events.pop_front(); }
        self.events.push_back(PerpEvent { kind: kind.into(), account, symbol, position_id, sequence: self.sequence, timestamp_ms: now_ms(), message: message.into() });
    }
}

fn refresh_position(position: &mut PerpPosition, mark_price: Price, maintenance_bps: u32, initial_bps: u32) -> Result<(), GravityError> {
    let mark_notional = position.quantity.0.checked_mul(mark_price.0)?;
    let entry_notional = position.quantity.0.checked_mul(position.entry_price.0)?;
    let pnl = match position.side {
        PerpSide::Long => mark_notional.checked_sub(entry_notional)?,
        PerpSide::Short => entry_notional.checked_sub(mark_notional)?,
    };
    position.mark_price = mark_price;
    position.notional = mark_notional;
    position.unrealized_pnl = pnl;
    position.equity = position.collateral.checked_add(pnl)?;
    position.margin_requirement = bps_mul(mark_notional, initial_bps)?;
    position.maintenance_requirement = bps_mul(mark_notional, maintenance_bps)?;
    position.liquidation_price = liquidation_price(position.side, position.entry_price, position.collateral, position.quantity, maintenance_bps).ok();
    position.updated_ms = now_ms();
    Ok(())
}

fn liquidation_price(side: PerpSide, entry: Price, collateral: Fixed, quantity: Quantity, maintenance_bps: u32) -> Result<Price, GravityError> {
    let maintenance_per_unit = entry.0.checked_mul(Fixed::raw(maintenance_bps as i128))?.checked_div(Fixed::raw(10_000))?;
    let collateral_per_unit = collateral.checked_div(quantity.0)?;
    let raw = match side {
        PerpSide::Long => entry.0.checked_sub(collateral_per_unit)?.checked_add(maintenance_per_unit)?,
        PerpSide::Short => entry.0.checked_add(collateral_per_unit)?.checked_sub(maintenance_per_unit)?,
    };
    Price::new(Fixed::raw(raw.as_raw().max(1)))
}

fn bps_mul(value: Fixed, bps: u32) -> Result<Fixed, GravityError> {
    value.checked_mul(Fixed::raw(bps as i128))?.checked_div(Fixed::raw(10_000))
}
