//! Native AMM runtime primitives for Gravity.
//!
//! The AMM crate is deterministic and fixed-point only. Gravity simulates and
//! prepares settlement hints here; Stargate/L3 remains final truth.

use gravity_types::{Fixed, GravityError, Price, Quantity, Symbol, now_ms, BPS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolKind { ConstantProduct, Stable, Weighted }

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwapSide { BaseIn, QuoteIn }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolConfig {
    pub symbol: Symbol,
    pub kind: PoolKind,
    pub fee_bps: u32,
    pub min_liquidity: Quantity,
    pub base_weight_bps: u32,
    pub quote_weight_bps: u32,
    pub amplification_bps: u32,
    pub max_price_impact_bps: u32,
}

impl PoolConfig {
    pub fn normalized(symbol: Symbol, kind: PoolKind, fee_bps: u32, min_liquidity: Quantity) -> Self {
        Self {
            symbol,
            kind,
            fee_bps,
            min_liquidity,
            base_weight_bps: 5_000,
            quote_weight_bps: 5_000,
            amplification_bps: 10_000,
            max_price_impact_bps: 5_000,
        }
    }

    pub fn validate(&self) -> Result<(), GravityError> {
        if self.fee_bps > 1_000 { return Err(GravityError::InvalidConfig("AMM fee cannot exceed 1000 bps".into())); }
        if self.max_price_impact_bps == 0 || self.max_price_impact_bps > 10_000 { return Err(GravityError::InvalidConfig("AMM max_price_impact_bps must be 1..10000".into())); }
        match self.kind {
            PoolKind::Weighted => {
                if self.base_weight_bps == 0 || self.quote_weight_bps == 0 || self.base_weight_bps + self.quote_weight_bps != 10_000 {
                    return Err(GravityError::InvalidConfig("weighted AMM pools require base_weight_bps + quote_weight_bps = 10000".into()));
                }
            }
            PoolKind::Stable => {
                if self.amplification_bps < 1_000 || self.amplification_bps > 1_000_000 {
                    return Err(GravityError::InvalidConfig("stable AMM amplification_bps must be 1000..1000000".into()));
                }
            }
            PoolKind::ConstantProduct => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolSnapshot {
    pub symbol: Symbol,
    pub kind: PoolKind,
    pub base_reserve: Quantity,
    pub quote_reserve: Quantity,
    pub lp_supply: Quantity,
    pub price: Price,
    pub fee_bps: u32,
    pub base_weight_bps: u32,
    pub quote_weight_bps: u32,
    pub amplification_bps: u32,
    pub max_price_impact_bps: u32,
    pub sequence: u64,
    pub updated_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapQuote {
    pub symbol: Symbol,
    pub side: SwapSide,
    pub amount_in: Quantity,
    pub amount_out: Quantity,
    pub fee_paid: Quantity,
    pub price_impact_bps: u32,
    pub before_price: Price,
    pub after_price: Price,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidityResult {
    pub symbol: Symbol,
    pub base_added: Quantity,
    pub quote_added: Quantity,
    pub lp_minted: Quantity,
    pub snapshot: PoolSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveLiquidityResult {
    pub symbol: Symbol,
    pub lp_burned: Quantity,
    pub base_removed: Quantity,
    pub quote_removed: Quantity,
    pub snapshot: PoolSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwapResult {
    pub quote: SwapQuote,
    pub snapshot: PoolSnapshot,
    pub settlement_hint: AmmSettlementHint,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmmSettlementHint {
    pub symbol: Symbol,
    pub side: SwapSide,
    pub base_delta: Fixed,
    pub quote_delta: Fixed,
    pub fee_delta: Fixed,
    pub pool_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmmGuardResult {
    pub symbol: Symbol,
    pub pool_price: Price,
    pub oracle_price: Price,
    pub deviation_bps: u32,
    pub max_deviation_bps: u32,
    pub allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolEvent {
    pub symbol: Symbol,
    pub kind: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct AmmPool {
    config: PoolConfig,
    base_reserve: Quantity,
    quote_reserve: Quantity,
    lp_supply: Quantity,
    sequence: u64,
}

impl AmmPool {
    pub fn new(config: PoolConfig, base_reserve: Quantity, quote_reserve: Quantity) -> Result<Self, GravityError> {
        config.validate()?;
        let lp_supply = integer_sqrt(base_reserve.0.checked_mul(quote_reserve.0)?)?;
        let pool = Self { config, base_reserve, quote_reserve, lp_supply: Quantity::new(lp_supply)?, sequence: 0 };
        pool.ensure_liquidity()?;
        Ok(pool)
    }

    pub fn symbol(&self) -> &Symbol { &self.config.symbol }
    pub fn sequence(&self) -> u64 { self.sequence }

    pub fn snapshot(&self) -> Result<PoolSnapshot, GravityError> {
        Ok(PoolSnapshot {
            symbol: self.config.symbol.clone(),
            kind: self.config.kind,
            base_reserve: self.base_reserve,
            quote_reserve: self.quote_reserve,
            lp_supply: self.lp_supply,
            price: self.price()?,
            fee_bps: self.config.fee_bps,
            base_weight_bps: self.config.base_weight_bps,
            quote_weight_bps: self.config.quote_weight_bps,
            amplification_bps: self.config.amplification_bps,
            max_price_impact_bps: self.config.max_price_impact_bps,
            sequence: self.sequence,
            updated_ms: now_ms(),
        })
    }

    pub fn quote(&self, side: SwapSide, amount_in: Quantity) -> Result<SwapQuote, GravityError> {
        self.ensure_liquidity()?;
        let before_price = self.price()?;
        let fee_paid = fee_amount(amount_in, self.config.fee_bps)?;
        let amount_after_fee = Quantity::new(amount_in.0.checked_sub(fee_paid.0)?)?;
        let amount_out = match self.config.kind {
            PoolKind::ConstantProduct => self.constant_product_out(side, amount_after_fee)?,
            PoolKind::Stable => self.stable_out(side, amount_after_fee)?,
            PoolKind::Weighted => self.weighted_out(side, amount_after_fee)?,
        };
        let (next_base, next_quote) = self.simulated_reserves(side, amount_after_fee, amount_out)?;
        let after_price = pool_price(next_base, next_quote)?;
        let impact = price_impact_bps(before_price, after_price);
        Ok(SwapQuote { symbol: self.config.symbol.clone(), side, amount_in, amount_out, fee_paid, price_impact_bps: impact, before_price, after_price, sequence: self.sequence, timestamp_ms: now_ms() })
    }

    pub fn swap(&mut self, side: SwapSide, amount_in: Quantity, min_out: Option<Quantity>) -> Result<SwapResult, GravityError> {
        let quote = self.quote(side, amount_in)?;
        if quote.price_impact_bps > self.config.max_price_impact_bps { return Err(GravityError::InvalidConfig("AMM price impact guard rejected swap".into())); }
        if let Some(min) = min_out {
            if quote.amount_out < min { return Err(GravityError::InvalidConfig("AMM slippage check failed".into())); }
        }
        let after_fee = Quantity::new(amount_in.0.checked_sub(quote.fee_paid.0)?)?;
        let (next_base, next_quote) = self.simulated_reserves(side, after_fee, quote.amount_out)?;
        self.base_reserve = next_base;
        self.quote_reserve = next_quote;
        self.sequence = self.sequence.saturating_add(1);
        let hint = match side {
            SwapSide::BaseIn => AmmSettlementHint { symbol: self.config.symbol.clone(), side, base_delta: amount_in.0, quote_delta: Fixed::raw(-quote.amount_out.0.as_raw()), fee_delta: quote.fee_paid.0, pool_sequence: self.sequence },
            SwapSide::QuoteIn => AmmSettlementHint { symbol: self.config.symbol.clone(), side, base_delta: Fixed::raw(-quote.amount_out.0.as_raw()), quote_delta: amount_in.0, fee_delta: quote.fee_paid.0, pool_sequence: self.sequence },
        };
        Ok(SwapResult { quote, snapshot: self.snapshot()?, settlement_hint: hint })
    }

    pub fn add_liquidity(&mut self, base: Quantity, quote: Quantity) -> Result<LiquidityResult, GravityError> {
        let base_share = base.0.checked_div(self.base_reserve.0)?;
        let quote_share = quote.0.checked_div(self.quote_reserve.0)?;
        let share = if base_share < quote_share { base_share } else { quote_share };
        let minted = self.lp_supply.0.checked_mul(share)?;
        self.base_reserve = Quantity::new(self.base_reserve.0.checked_add(base.0)?)?;
        self.quote_reserve = Quantity::new(self.quote_reserve.0.checked_add(quote.0)?)?;
        self.lp_supply = Quantity::new(self.lp_supply.0.checked_add(minted)?)?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(LiquidityResult { symbol: self.config.symbol.clone(), base_added: base, quote_added: quote, lp_minted: Quantity::new(minted)?, snapshot: self.snapshot()? })
    }

    pub fn remove_liquidity(&mut self, lp: Quantity, min_base: Option<Quantity>, min_quote: Option<Quantity>) -> Result<RemoveLiquidityResult, GravityError> {
        if lp > self.lp_supply { return Err(GravityError::InvalidQuantity); }
        let share = lp.0.checked_div(self.lp_supply.0)?;
        let base_out = Quantity::new(self.base_reserve.0.checked_mul(share)?)?;
        let quote_out = Quantity::new(self.quote_reserve.0.checked_mul(share)?)?;
        if let Some(min) = min_base { if base_out < min { return Err(GravityError::InvalidConfig("AMM base output slippage check failed".into())); } }
        if let Some(min) = min_quote { if quote_out < min { return Err(GravityError::InvalidConfig("AMM quote output slippage check failed".into())); } }
        self.base_reserve = Quantity::new(self.base_reserve.0.checked_sub(base_out.0)?)?;
        self.quote_reserve = Quantity::new(self.quote_reserve.0.checked_sub(quote_out.0)?)?;
        self.lp_supply = Quantity::new(self.lp_supply.0.checked_sub(lp.0)?)?;
        self.sequence = self.sequence.saturating_add(1);
        Ok(RemoveLiquidityResult { symbol: self.config.symbol.clone(), lp_burned: lp, base_removed: base_out, quote_removed: quote_out, snapshot: self.snapshot()? })
    }

    pub fn oracle_guard(&self, oracle_price: Price, max_deviation_bps: u32) -> Result<AmmGuardResult, GravityError> {
        let pool_price = self.price()?;
        let deviation_bps = price_impact_bps(pool_price, oracle_price);
        Ok(AmmGuardResult { symbol: self.config.symbol.clone(), pool_price, oracle_price, deviation_bps, max_deviation_bps, allowed: deviation_bps <= max_deviation_bps })
    }

    fn ensure_liquidity(&self) -> Result<(), GravityError> {
        if self.base_reserve < self.config.min_liquidity || self.quote_reserve < self.config.min_liquidity { return Err(GravityError::InvalidConfig("AMM pool has insufficient liquidity".into())); }
        Ok(())
    }

    fn price(&self) -> Result<Price, GravityError> { pool_price(self.base_reserve, self.quote_reserve) }

    fn constant_product_out(&self, side: SwapSide, amount_after_fee: Quantity) -> Result<Quantity, GravityError> {
        let (reserve_in, reserve_out) = match side { SwapSide::BaseIn => (self.base_reserve, self.quote_reserve), SwapSide::QuoteIn => (self.quote_reserve, self.base_reserve) };
        let numerator = amount_after_fee.0.checked_mul(reserve_out.0)?;
        let denominator = reserve_in.0.checked_add(amount_after_fee.0)?;
        Quantity::new(numerator.checked_div(denominator)?)
    }

    fn weighted_out(&self, side: SwapSide, amount_after_fee: Quantity) -> Result<Quantity, GravityError> {
        let cp = self.constant_product_out(side, amount_after_fee)?;
        let (in_weight, out_weight) = match side { SwapSide::BaseIn => (self.config.base_weight_bps, self.config.quote_weight_bps), SwapSide::QuoteIn => (self.config.quote_weight_bps, self.config.base_weight_bps) };
        let adjusted = cp.0.as_raw()
            .checked_mul(in_weight as i128).and_then(|v| v.checked_div(out_weight as i128))
            .ok_or(GravityError::Overflow)?;
        let reserve_out = match side { SwapSide::BaseIn => self.quote_reserve, SwapSide::QuoteIn => self.base_reserve };
        let cap = reserve_out.0.as_raw().saturating_mul(9).saturating_div(10);
        Quantity::new(Fixed::raw(adjusted.min(cap).max(1)))
    }

    fn stable_out(&self, side: SwapSide, amount_after_fee: Quantity) -> Result<Quantity, GravityError> {
        let reserve_out = match side { SwapSide::BaseIn => self.quote_reserve, SwapSide::QuoteIn => self.base_reserve };
        let reserve_in = match side { SwapSide::BaseIn => self.base_reserve, SwapSide::QuoteIn => self.quote_reserve };
        let imbalance = if reserve_in > reserve_out { reserve_in.0.checked_sub(reserve_out.0)? } else { reserve_out.0.checked_sub(reserve_in.0)? };
        let imbalance_bps = imbalance.as_raw().saturating_mul(BPS).saturating_div(reserve_in.0.as_raw().max(1).abs());
        let amp = self.config.amplification_bps.max(1) as i128;
        let penalty_bps = imbalance_bps.saturating_mul(10_000).saturating_div(amp).min(5_000);
        let raw = amount_after_fee.0.as_raw().saturating_mul(BPS.saturating_sub(penalty_bps)).saturating_div(BPS);
        let cap = reserve_out.0.as_raw().saturating_mul(3).saturating_div(10);
        Quantity::new(Fixed::raw(raw.min(cap).max(1)))
    }

    fn simulated_reserves(&self, side: SwapSide, amount_after_fee: Quantity, amount_out: Quantity) -> Result<(Quantity, Quantity), GravityError> {
        match side {
            SwapSide::BaseIn => Ok((Quantity::new(self.base_reserve.0.checked_add(amount_after_fee.0)?)?, Quantity::new(self.quote_reserve.0.checked_sub(amount_out.0)?)?)),
            SwapSide::QuoteIn => Ok((Quantity::new(self.base_reserve.0.checked_sub(amount_out.0)?)?, Quantity::new(self.quote_reserve.0.checked_add(amount_after_fee.0)?)?)),
        }
    }
}

#[derive(Clone, Default)]
pub struct AmmBook {
    pools: BTreeMap<String, AmmPool>,
    events: VecDeque<PoolEvent>,
    limit: usize,
}

impl AmmBook {
    pub fn new() -> Self { Self { pools: BTreeMap::new(), events: VecDeque::with_capacity(100_000), limit: 100_000 } }

    pub fn create_pool(&mut self, config: PoolConfig, base: Quantity, quote: Quantity) -> Result<PoolSnapshot, GravityError> {
        let pool = AmmPool::new(config, base, quote)?;
        let snapshot = pool.snapshot()?;
        self.pools.insert(snapshot.symbol.0.clone(), pool);
        self.record(&snapshot.symbol, "pool_created", snapshot.sequence, "pool created");
        Ok(snapshot)
    }

    pub fn snapshots(&self) -> Result<Vec<PoolSnapshot>, GravityError> { self.pools.values().map(AmmPool::snapshot).collect() }
    pub fn snapshot(&self, symbol: &str) -> Result<Option<PoolSnapshot>, GravityError> { self.pools.get(symbol).map(AmmPool::snapshot).transpose() }
    pub fn quote(&self, symbol: &str, side: SwapSide, amount_in: Quantity) -> Result<SwapQuote, GravityError> { self.pools.get(symbol).ok_or_else(|| GravityError::NotFound(format!("AMM pool {symbol}")))?.quote(side, amount_in) }

    pub fn swap(&mut self, symbol: &str, side: SwapSide, amount_in: Quantity, min_out: Option<Quantity>) -> Result<SwapResult, GravityError> {
        let result = self.pools.get_mut(symbol).ok_or_else(|| GravityError::NotFound(format!("AMM pool {symbol}")))?.swap(side, amount_in, min_out)?;
        self.record(&result.snapshot.symbol, "swap", result.snapshot.sequence, "swap simulated and applied");
        Ok(result)
    }

    pub fn add_liquidity(&mut self, symbol: &str, base: Quantity, quote: Quantity) -> Result<LiquidityResult, GravityError> {
        let result = self.pools.get_mut(symbol).ok_or_else(|| GravityError::NotFound(format!("AMM pool {symbol}")))?.add_liquidity(base, quote)?;
        self.record(&result.symbol, "liquidity_added", result.snapshot.sequence, "liquidity added");
        Ok(result)
    }

    pub fn remove_liquidity(&mut self, symbol: &str, lp: Quantity, min_base: Option<Quantity>, min_quote: Option<Quantity>) -> Result<RemoveLiquidityResult, GravityError> {
        let result = self.pools.get_mut(symbol).ok_or_else(|| GravityError::NotFound(format!("AMM pool {symbol}")))?.remove_liquidity(lp, min_base, min_quote)?;
        self.record(&result.symbol, "liquidity_removed", result.snapshot.sequence, "liquidity removed");
        Ok(result)
    }

    pub fn oracle_guard(&self, symbol: &str, oracle_price: Price, max_deviation_bps: u32) -> Result<AmmGuardResult, GravityError> {
        self.pools.get(symbol).ok_or_else(|| GravityError::NotFound(format!("AMM pool {symbol}")))?.oracle_guard(oracle_price, max_deviation_bps)
    }

    pub fn recent_events(&self, limit: usize) -> Vec<PoolEvent> {
        let mut out = self.events.iter().rev().take(limit.min(10_000)).cloned().collect::<Vec<_>>();
        out.reverse();
        out
    }

    fn record(&mut self, symbol: &Symbol, kind: impl Into<String>, sequence: u64, body: impl Into<String>) {
        if self.events.len() >= self.limit { self.events.pop_front(); }
        self.events.push_back(PoolEvent { symbol: symbol.clone(), kind: kind.into(), sequence, timestamp_ms: now_ms(), body: body.into() });
    }
}

fn fee_amount(amount: Quantity, fee_bps: u32) -> Result<Quantity, GravityError> {
    let raw = amount.0.as_raw().checked_mul(fee_bps as i128).and_then(|v| v.checked_div(10_000)).ok_or(GravityError::Overflow)?;
    Quantity::new(Fixed::raw(raw.max(1)))
}

fn pool_price(base: Quantity, quote: Quantity) -> Result<Price, GravityError> { Price::new(quote.0.checked_div(base.0)?) }

fn price_impact_bps(before: Price, after: Price) -> u32 {
    let diff = (after.0 - before.0).abs();
    if before.0.as_raw() == 0 { return 0; }
    let value = diff.as_raw().saturating_mul(10_000).saturating_div(before.0.as_raw().abs());
    value.min(u32::MAX as i128) as u32
}

fn integer_sqrt(value: Fixed) -> Result<Fixed, GravityError> {
    if value.as_raw() <= 0 { return Err(GravityError::InvalidQuantity); }
    let mut x0 = value.as_raw();
    let mut x1 = (x0 + 1) / 2;
    while x1 < x0 { x0 = x1; x1 = (x1 + value.as_raw() / x1) / 2; }
    Ok(Fixed::raw(x0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }

    #[test]
    fn constant_product_quote_and_swap_work() {
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let config = PoolConfig::normalized(symbol.clone(), PoolKind::ConstantProduct, 30, q(1));
        let mut pool = AmmPool::new(config, q(100), q(10_000_000)).unwrap();
        let quote = pool.quote(SwapSide::BaseIn, q(1)).unwrap();
        assert!(quote.amount_out.0.as_raw() > 0);
        let result = pool.swap(SwapSide::BaseIn, q(1), None).unwrap();
        assert_eq!(result.snapshot.sequence, 1);
    }

    #[test]
    fn remove_liquidity_burns_lp_and_returns_reserves() {
        let symbol = Symbol::new("ETH-USDx").unwrap();
        let config = PoolConfig::normalized(symbol, PoolKind::ConstantProduct, 30, q(1));
        let mut pool = AmmPool::new(config, q(100), q(200_000)).unwrap();
        let before = pool.snapshot().unwrap();
        let result = pool.remove_liquidity(Quantity::new(Fixed::raw(before.lp_supply.0.as_raw() / 10)).unwrap(), None, None).unwrap();
        assert!(result.base_removed.0.as_raw() > 0);
        assert!(result.quote_removed.0.as_raw() > 0);
    }

    #[test]
    fn oracle_guard_rejects_large_deviation() {
        let symbol = Symbol::new("SOL-USDx").unwrap();
        let config = PoolConfig::normalized(symbol, PoolKind::ConstantProduct, 30, q(1));
        let pool = AmmPool::new(config, q(100), q(10_000)).unwrap();
        let guard = pool.oracle_guard(Price::new(Fixed::from_units(500)).unwrap(), 100).unwrap();
        assert!(!guard.allowed);
    }
}
