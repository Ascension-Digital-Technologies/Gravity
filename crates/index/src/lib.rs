use gravity_types::{Fixed, GravityError, Price, Quantity, Symbol, now_ms, BPS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexAsset {
    pub symbol: Symbol,
    pub target_weight_bps: u32,
    pub oracle_price: Price,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexProductConfig {
    pub id: String,
    pub name: String,
    pub quote_asset: String,
    pub management_fee_bps: u32,
    pub rebalance_threshold_bps: u32,
    pub min_mint_notional: Fixed,
    pub assets: Vec<IndexAsset>,
}

impl IndexProductConfig {
    pub fn validate(&self) -> Result<(), GravityError> {
        validate_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 96 { return Err(GravityError::InvalidConfig("index name must be 1-96 characters".into())); }
        if self.quote_asset.trim().is_empty() || self.quote_asset.len() > 32 { return Err(GravityError::InvalidConfig("quote asset must be 1-32 characters".into())); }
        if self.management_fee_bps > 2_000 { return Err(GravityError::InvalidConfig("management fee must be <= 2000 bps".into())); }
        if self.rebalance_threshold_bps > 10_000 { return Err(GravityError::InvalidConfig("rebalance threshold must be <= 10000 bps".into())); }
        if self.min_mint_notional.0 < 0 { return Err(GravityError::InvalidConfig("minimum mint notional cannot be negative".into())); }
        if self.assets.is_empty() { return Err(GravityError::InvalidConfig("index product requires at least one asset".into())); }
        let mut total = 0_u32;
        for asset in &self.assets {
            if asset.target_weight_bps == 0 { return Err(GravityError::InvalidConfig("asset target weight must be positive".into())); }
            total = total.saturating_add(asset.target_weight_bps);
        }
        if total != 10_000 { return Err(GravityError::InvalidConfig(format!("index target weights must sum to 10000 bps, got {total}"))); }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexHolding {
    pub symbol: Symbol,
    pub quantity: Quantity,
    pub price: Price,
    pub value: Fixed,
    pub current_weight_bps: u32,
    pub target_weight_bps: u32,
    pub drift_bps: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexProductSnapshot {
    pub id: String,
    pub name: String,
    pub quote_asset: String,
    pub nav: Fixed,
    pub unit_supply: Quantity,
    pub management_fee_bps: u32,
    pub rebalance_threshold_bps: u32,
    pub holdings: Vec<IndexHolding>,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexNavReport {
    pub product_id: String,
    pub nav: Fixed,
    pub unit_supply: Quantity,
    pub nav_per_unit: Price,
    pub holdings: Vec<IndexHolding>,
    pub oracle_dependencies: Vec<Symbol>,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebalanceLeg {
    pub symbol: Symbol,
    pub action: String,
    pub target_weight_bps: u32,
    pub current_weight_bps: u32,
    pub drift_bps: i64,
    pub notional_delta: Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebalancePlan {
    pub product_id: String,
    pub nav: Fixed,
    pub threshold_bps: u32,
    pub required: bool,
    pub legs: Vec<RebalanceLeg>,
    pub settlement_hint: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintRedeemPlan {
    pub product_id: String,
    pub account: String,
    pub side: String,
    pub notional: Fixed,
    pub estimated_units: Quantity,
    pub nav_per_unit: Price,
    pub fee: Fixed,
    pub settlement_hint: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEvent {
    pub kind: String,
    pub product_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStats {
    pub products: usize,
    pub reports: u64,
    pub rebalance_plans: u64,
    pub mint_plans: u64,
    pub redeem_plans: u64,
    pub events: usize,
}

#[derive(Clone, Debug)]
struct IndexProductState {
    config: IndexProductConfig,
    holdings: Vec<(Symbol, Quantity, Price, u32)>,
    unit_supply: Quantity,
    sequence: u64,
}

#[derive(Debug)]
pub struct IndexEngine {
    products: BTreeMap<String, IndexProductState>,
    events: VecDeque<IndexEvent>,
    stats: IndexStats,
    event_limit: usize,
}

impl Default for IndexEngine { fn default() -> Self { Self::new() } }

impl IndexEngine {
    pub fn new() -> Self {
        Self { products: BTreeMap::new(), events: VecDeque::with_capacity(100_000), stats: IndexStats::default(), event_limit: 100_000 }
    }

    pub fn create_product(&mut self, config: IndexProductConfig, seed_notional: Fixed) -> Result<IndexProductSnapshot, GravityError> {
        config.validate()?;
        if self.products.contains_key(&config.id) { return Err(GravityError::InvalidConfig(format!("index product already exists: {}", config.id))); }
        let seed = if seed_notional.0 <= 0 { Fixed::from_units(1_000_000) } else { seed_notional };
        let mut holdings = Vec::with_capacity(config.assets.len());
        for asset in &config.assets {
            let target_value = mul_bps(seed, asset.target_weight_bps)?;
            let qty = Quantity::new(target_value.checked_div(asset.oracle_price.0)?)?;
            holdings.push((asset.symbol.clone(), qty, asset.oracle_price, asset.target_weight_bps));
        }
        let unit_supply = Quantity::new(Fixed::from_units(1_000_000))?;
        let state = IndexProductState { config: config.clone(), holdings, unit_supply, sequence: 1 };
        let snap = snapshot(&state)?;
        self.products.insert(config.id.clone(), state);
        self.stats.products = self.products.len();
        self.push_event("product_created", &config.id, 1, "index product created");
        Ok(snap)
    }

    pub fn products(&self) -> Result<Vec<IndexProductSnapshot>, GravityError> {
        self.products.values().map(snapshot).collect()
    }

    pub fn product(&self, id: &str) -> Result<Option<IndexProductSnapshot>, GravityError> {
        self.products.get(id).map(snapshot).transpose()
    }

    pub fn nav(&mut self, id: &str) -> Result<IndexNavReport, GravityError> {
        let state = self.products.get_mut(id).ok_or_else(|| GravityError::InvalidConfig(format!("unknown index product: {id}")))?;
        state.sequence = state.sequence.saturating_add(1);
        let snap = snapshot(state)?;
        let nav_per_unit = Price::new(snap.nav.checked_div(snap.unit_supply.0)?)?;
        let report = IndexNavReport {
            product_id: snap.id.clone(),
            nav: snap.nav,
            unit_supply: snap.unit_supply,
            nav_per_unit,
            holdings: snap.holdings,
            oracle_dependencies: state.holdings.iter().map(|h| h.0.clone()).collect(),
            sequence: state.sequence,
            timestamp_ms: now_ms(),
        };
        self.stats.reports = self.stats.reports.saturating_add(1);
        self.push_event("nav", id, report.sequence, "index NAV calculated");
        Ok(report)
    }

    pub fn rebalance_plan(&mut self, id: &str) -> Result<RebalancePlan, GravityError> {
        let report = self.nav(id)?;
        let state = self.products.get(id).ok_or_else(|| GravityError::InvalidConfig(format!("unknown index product: {id}")))?;
        let threshold = state.config.rebalance_threshold_bps;
        let mut legs = Vec::new();
        for holding in &report.holdings {
            let target_value = mul_bps(report.nav, holding.target_weight_bps)?;
            let notional_delta = target_value.checked_sub(holding.value)?;
            let required = holding.drift_bps.unsigned_abs() as u32 >= threshold;
            if required {
                legs.push(RebalanceLeg {
                    symbol: holding.symbol.clone(),
                    action: if notional_delta.0 >= 0 { "buy".into() } else { "sell".into() },
                    target_weight_bps: holding.target_weight_bps,
                    current_weight_bps: holding.current_weight_bps,
                    drift_bps: holding.drift_bps,
                    notional_delta,
                });
            }
        }
        let required = !legs.is_empty();
        let plan = RebalancePlan {
            product_id: id.to_owned(),
            nav: report.nav,
            threshold_bps: threshold,
            required,
            legs,
            settlement_hint: format!("index-rebalance:{id}:{}", report.sequence),
            sequence: report.sequence,
            timestamp_ms: now_ms(),
        };
        self.stats.rebalance_plans = self.stats.rebalance_plans.saturating_add(1);
        self.push_event("rebalance_plan", id, plan.sequence, if required { "rebalance required" } else { "rebalance not required" });
        Ok(plan)
    }

    pub fn mint_plan(&mut self, id: &str, account: String, notional: Fixed) -> Result<MintRedeemPlan, GravityError> {
        validate_account(&account)?;
        if notional.0 <= 0 { return Err(GravityError::InvalidConfig("mint notional must be positive".into())); }
        let report = self.nav(id)?;
        let state = self.products.get(id).ok_or_else(|| GravityError::InvalidConfig(format!("unknown index product: {id}")))?;
        if notional.0 < state.config.min_mint_notional.0 { return Err(GravityError::InvalidConfig("mint notional below product minimum".into())); }
        let fee = mul_bps(notional, state.config.management_fee_bps)?;
        let net = notional.checked_sub(fee)?;
        let units = Quantity::new(net.checked_div(report.nav_per_unit.0)?)?;
        let plan = MintRedeemPlan { product_id: id.to_owned(), account, side: "mint".into(), notional, estimated_units: units, nav_per_unit: report.nav_per_unit, fee, settlement_hint: format!("index-mint:{id}:{}", report.sequence), sequence: report.sequence, timestamp_ms: now_ms() };
        self.stats.mint_plans = self.stats.mint_plans.saturating_add(1);
        self.push_event("mint_plan", id, plan.sequence, "index mint plan created");
        Ok(plan)
    }

    pub fn redeem_plan(&mut self, id: &str, account: String, units: Quantity) -> Result<MintRedeemPlan, GravityError> {
        validate_account(&account)?;
        let report = self.nav(id)?;
        let state = self.products.get(id).ok_or_else(|| GravityError::InvalidConfig(format!("unknown index product: {id}")))?;
        let notional = units.0.checked_mul(report.nav_per_unit.0)?;
        let fee = mul_bps(notional, state.config.management_fee_bps)?;
        let plan = MintRedeemPlan { product_id: id.to_owned(), account, side: "redeem".into(), notional, estimated_units: units, nav_per_unit: report.nav_per_unit, fee, settlement_hint: format!("index-redeem:{id}:{}", report.sequence), sequence: report.sequence, timestamp_ms: now_ms() };
        self.stats.redeem_plans = self.stats.redeem_plans.saturating_add(1);
        self.push_event("redeem_plan", id, plan.sequence, "index redeem plan created");
        Ok(plan)
    }

    pub fn events(&self, limit: usize) -> Vec<IndexEvent> {
        let limit = limit.min(10_000);
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn stats(&self) -> IndexStats {
        let mut stats = self.stats.clone();
        stats.products = self.products.len();
        stats.events = self.events.len();
        stats
    }

    fn push_event(&mut self, kind: &str, product_id: &str, sequence: u64, detail: &str) {
        if self.events.len() >= self.event_limit { self.events.pop_front(); }
        self.events.push_back(IndexEvent { kind: kind.into(), product_id: product_id.into(), sequence, timestamp_ms: now_ms(), detail: detail.into() });
    }
}

fn snapshot(state: &IndexProductState) -> Result<IndexProductSnapshot, GravityError> {
    let mut total = Fixed::ZERO;
    let mut values = Vec::with_capacity(state.holdings.len());
    for (symbol, qty, price, target) in &state.holdings {
        let value = qty.0.checked_mul(price.0)?;
        total = total.checked_add(value)?;
        values.push((symbol.clone(), *qty, *price, *target, value));
    }
    let mut holdings = Vec::with_capacity(values.len());
    for (symbol, quantity, price, target_weight_bps, value) in values {
        let current = if total.0 == 0 { 0 } else { ((value.0.saturating_mul(BPS)) / total.0).clamp(0, 10_000) as u32 };
        holdings.push(IndexHolding { symbol, quantity, price, value, current_weight_bps: current, target_weight_bps, drift_bps: current as i64 - target_weight_bps as i64 });
    }
    Ok(IndexProductSnapshot { id: state.config.id.clone(), name: state.config.name.clone(), quote_asset: state.config.quote_asset.clone(), nav: total, unit_supply: state.unit_supply, management_fee_bps: state.config.management_fee_bps, rebalance_threshold_bps: state.config.rebalance_threshold_bps, holdings, sequence: state.sequence, timestamp_ms: now_ms() })
}

fn mul_bps(value: Fixed, bps: u32) -> Result<Fixed, GravityError> {
    value.0.checked_mul(bps as i128).and_then(|v| v.checked_div(BPS)).map(Fixed::raw).ok_or(GravityError::Overflow)
}

fn validate_id(value: &str) -> Result<(), GravityError> {
    if value.len() < 3 || value.len() > 48 { return Err(GravityError::InvalidConfig("index id must be 3-48 characters".into())); }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) { return Err(GravityError::InvalidConfig("index id contains unsupported characters".into())); }
    Ok(())
}

fn validate_account(value: &str) -> Result<(), GravityError> {
    if value.len() < 3 || value.len() > 96 { return Err(GravityError::InvalidConfig("account must be 3-96 characters".into())); }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')) { return Err(GravityError::InvalidConfig("account contains unsupported characters".into())); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(v: i128) -> Price { Price::new(Fixed::from_units(v)).unwrap() }
    fn q(v: i128) -> Quantity { Quantity::new(Fixed::from_units(v)).unwrap() }

    #[test]
    fn creates_nav_and_rebalance_plan() {
        let mut engine = IndexEngine::new();
        let cfg = IndexProductConfig { id: "ASC10".into(), name: "Ascension Top 10".into(), quote_asset: "USDx".into(), management_fee_bps: 25, rebalance_threshold_bps: 500, min_mint_notional: Fixed::from_units(100), assets: vec![IndexAsset { symbol: Symbol::new("BTC-USDx").unwrap(), target_weight_bps: 6000, oracle_price: p(100_000) }, IndexAsset { symbol: Symbol::new("ETH-USDx").unwrap(), target_weight_bps: 4000, oracle_price: p(5_000) }] };
        let snap = engine.create_product(cfg, Fixed::from_units(1_000_000)).unwrap();
        assert_eq!(snap.holdings.len(), 2);
        let nav = engine.nav("ASC10").unwrap();
        assert!(nav.nav.0 > 0);
        let plan = engine.rebalance_plan("ASC10").unwrap();
        assert!(!plan.product_id.is_empty());
        let mint = engine.mint_plan("ASC10", "acct-1".into(), Fixed::from_units(10_000)).unwrap();
        assert!(mint.estimated_units.0.0 > 0);
        let redeem = engine.redeem_plan("ASC10", "acct-1".into(), q(10)).unwrap();
        assert_eq!(redeem.side, "redeem");
    }
}
