//! Portfolio risk runtime for Gravity.
//!
//! The risk crate is deterministic and fixed-point only. It calculates account
//! health, collateral haircuts, margin requirements, and risk events. Gravity
//! uses this layer for liquidation preparation, lending/perps/synthetics, and
//! AMM/CLOB collateral safety checks while Stargate/L3 remains final truth.

use gravity_types::{Fixed, GravityError, Price, Quantity, Symbol, now_ms, stable_hash_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskStatus { Healthy, Watch, MarginCall, Liquidatable }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskAssetRule {
    pub asset: String,
    pub collateral_factor_bps: u32,
    pub maintenance_margin_bps: u32,
    pub liquidation_threshold_bps: u32,
    pub max_exposure: Option<Quantity>,
}

impl RiskAssetRule {
    pub fn default_for(asset: impl Into<String>) -> Self {
        Self {
            asset: asset.into(),
            collateral_factor_bps: 8_500,
            maintenance_margin_bps: 1_500,
            liquidation_threshold_bps: 1_100,
            max_exposure: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionInput {
    pub symbol: Symbol,
    pub quantity: Quantity,
    pub mark_price: Price,
    pub side: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollateralInput {
    pub asset: String,
    pub quantity: Quantity,
    pub price: Price,
    pub collateral_factor_bps: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRiskInput {
    pub account: String,
    pub collaterals: Vec<CollateralInput>,
    pub positions: Vec<PositionInput>,
    pub debt_value: Fixed,
    pub maintenance_margin_bps: u32,
    pub initial_margin_bps: u32,
    pub timestamp_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRiskSnapshot {
    pub id: String,
    pub account: String,
    pub status: RiskStatus,
    pub collateral_value: Fixed,
    pub discounted_collateral_value: Fixed,
    pub position_notional: Fixed,
    pub debt_value: Fixed,
    pub initial_margin_required: Fixed,
    pub maintenance_margin_required: Fixed,
    pub equity: Fixed,
    pub free_collateral: Fixed,
    pub health_factor_bps: i128,
    pub leverage_bps: i128,
    pub oracle_dependencies: Vec<Symbol>,
    pub warnings: Vec<String>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskEvent {
    pub id: String,
    pub account: String,
    pub kind: String,
    pub status: RiskStatus,
    pub health_factor_bps: i128,
    pub timestamp_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskStats {
    pub accounts: usize,
    pub events: usize,
    pub healthy: usize,
    pub watch: usize,
    pub margin_call: usize,
    pub liquidatable: usize,
    pub last_updated_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskConfig {
    pub default_initial_margin_bps: u32,
    pub default_maintenance_margin_bps: u32,
    pub watch_health_bps: i128,
    pub margin_call_health_bps: i128,
    pub liquidation_health_bps: i128,
    pub event_limit: usize,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            default_initial_margin_bps: 2_000,
            default_maintenance_margin_bps: 1_000,
            watch_health_bps: 15_000,
            margin_call_health_bps: 12_000,
            liquidation_health_bps: 10_000,
            event_limit: 100_000,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RiskEngine {
    config: RiskConfig,
    snapshots: BTreeMap<String, AccountRiskSnapshot>,
    events: VecDeque<RiskEvent>,
    last_updated_ms: u64,
}

impl Default for RiskEngine {
    fn default() -> Self { Self::new(RiskConfig::default()) }
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self { config, snapshots: BTreeMap::new(), events: VecDeque::with_capacity(100_000), last_updated_ms: 0 }
    }

    pub fn check(&mut self, input: AccountRiskInput) -> Result<AccountRiskSnapshot, GravityError> {
        validate_account(&input.account)?;
        let timestamp_ms = input.timestamp_ms.unwrap_or_else(now_ms);
        let mut collateral_value = Fixed::ZERO;
        let mut discounted_collateral_value = Fixed::ZERO;
        let mut position_notional = Fixed::ZERO;
        let mut warnings = Vec::new();
        let mut dependencies = Vec::new();

        for collateral in &input.collaterals {
            let raw = collateral.price.0.checked_mul(collateral.quantity.0)?;
            collateral_value = collateral_value.checked_add(raw)?;
            discounted_collateral_value = discounted_collateral_value.checked_add(apply_bps(raw, collateral.collateral_factor_bps)?)?;
            if collateral.collateral_factor_bps > 10_000 { warnings.push(format!("{} collateral factor exceeds 100%", collateral.asset)); }
        }

        for position in &input.positions {
            let notional = position.mark_price.0.checked_mul(position.quantity.0)?;
            position_notional = position_notional.checked_add(notional.abs())?;
            dependencies.push(position.symbol.clone());
        }
        dependencies.sort_by(|a, b| a.0.cmp(&b.0));
        dependencies.dedup_by(|a, b| a.0 == b.0);

        let initial_bps = if input.initial_margin_bps == 0 { self.config.default_initial_margin_bps } else { input.initial_margin_bps };
        let maintenance_bps = if input.maintenance_margin_bps == 0 { self.config.default_maintenance_margin_bps } else { input.maintenance_margin_bps };
        let initial_margin_required = apply_bps(position_notional, initial_bps)?;
        let maintenance_margin_required = apply_bps(position_notional, maintenance_bps)?;
        let equity = discounted_collateral_value.checked_sub(input.debt_value)?;
        let free_collateral = equity.checked_sub(initial_margin_required)?;
        let denominator = input.debt_value.checked_add(maintenance_margin_required)?.checked_add(Fixed::ONE)?;
        let health_factor_bps = ratio_bps(equity, denominator)?;
        let leverage_bps = ratio_bps(position_notional, equity.abs().checked_add(Fixed::ONE)?)?;
        let status = classify(health_factor_bps, &self.config);
        if equity.0 < 0 { warnings.push("negative equity".into()); }
        if free_collateral.0 < 0 { warnings.push("free collateral below initial margin".into()); }
        if dependencies.is_empty() && !input.positions.is_empty() { warnings.push("positions missing oracle dependencies".into()); }

        let seed = format!("risk:{}:{}:{}:{}:{}", input.account, health_factor_bps, collateral_value, position_notional, timestamp_ms);
        let snapshot = AccountRiskSnapshot {
            id: stable_hash_hex(&seed),
            account: input.account.clone(),
            status,
            collateral_value,
            discounted_collateral_value,
            position_notional,
            debt_value: input.debt_value,
            initial_margin_required,
            maintenance_margin_required,
            equity,
            free_collateral,
            health_factor_bps,
            leverage_bps,
            oracle_dependencies: dependencies,
            warnings,
            timestamp_ms,
        };
        self.snapshots.insert(input.account.clone(), snapshot.clone());
        self.last_updated_ms = timestamp_ms;
        self.push_event(RiskEvent {
            id: stable_hash_hex(&format!("risk-event:{}:{}:{}", input.account, health_factor_bps, timestamp_ms)),
            account: input.account,
            kind: "risk_check".into(),
            status,
            health_factor_bps,
            timestamp_ms,
            message: format!("risk status {:?}", status),
        });
        Ok(snapshot)
    }

    pub fn snapshot(&self, account: &str) -> Option<AccountRiskSnapshot> { self.snapshots.get(account).cloned() }

    pub fn snapshots(&self) -> Vec<AccountRiskSnapshot> { self.snapshots.values().cloned().collect() }

    pub fn events(&self, limit: usize) -> Vec<RiskEvent> {
        let mut out = self.events.iter().rev().take(limit.min(self.config.event_limit)).cloned().collect::<Vec<_>>();
        out.reverse();
        out
    }

    pub fn stats(&self) -> RiskStats {
        let mut healthy = 0;
        let mut watch = 0;
        let mut margin_call = 0;
        let mut liquidatable = 0;
        for snapshot in self.snapshots.values() {
            match snapshot.status {
                RiskStatus::Healthy => healthy += 1,
                RiskStatus::Watch => watch += 1,
                RiskStatus::MarginCall => margin_call += 1,
                RiskStatus::Liquidatable => liquidatable += 1,
            }
        }
        RiskStats { accounts: self.snapshots.len(), events: self.events.len(), healthy, watch, margin_call, liquidatable, last_updated_ms: self.last_updated_ms }
    }

    fn push_event(&mut self, event: RiskEvent) {
        if self.events.len() >= self.config.event_limit { self.events.pop_front(); }
        self.events.push_back(event);
    }
}

fn apply_bps(value: Fixed, bps: u32) -> Result<Fixed, GravityError> {
    value.0.checked_mul(i128::from(bps)).and_then(|v| v.checked_div(10_000)).map(Fixed::raw).ok_or(GravityError::Overflow)
}

fn ratio_bps(numerator: Fixed, denominator: Fixed) -> Result<i128, GravityError> {
    if denominator.0 == 0 { return Ok(i128::MAX); }
    numerator.0.checked_mul(10_000).and_then(|v| v.checked_div(denominator.0)).ok_or(GravityError::Overflow)
}

fn classify(health_factor_bps: i128, config: &RiskConfig) -> RiskStatus {
    if health_factor_bps <= config.liquidation_health_bps { RiskStatus::Liquidatable }
    else if health_factor_bps <= config.margin_call_health_bps { RiskStatus::MarginCall }
    else if health_factor_bps <= config.watch_health_bps { RiskStatus::Watch }
    else { RiskStatus::Healthy }
}

fn validate_account(value: &str) -> Result<(), GravityError> {
    if value.len() < 3 || value.len() > 96 || !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')) {
        return Err(GravityError::InvalidConfig(format!("invalid account id: {value}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_account_has_high_health() {
        let mut engine = RiskEngine::default();
        let snapshot = engine.check(AccountRiskInput {
            account: "acct-1".into(),
            collaterals: vec![CollateralInput { asset: "USDx".into(), quantity: Quantity::new(Fixed::from_units(10_000)).unwrap(), price: Price::new(Fixed::ONE).unwrap(), collateral_factor_bps: 9_500 }],
            positions: vec![PositionInput { symbol: Symbol::new("BTC-USDx").unwrap(), quantity: Quantity::new(Fixed::from_units(1)).unwrap(), mark_price: Price::new(Fixed::from_units(1_000)).unwrap(), side: "long".into() }],
            debt_value: Fixed::from_units(1_000),
            maintenance_margin_bps: 1_000,
            initial_margin_bps: 2_000,
            timestamp_ms: Some(1),
        }).unwrap();
        assert_eq!(snapshot.status, RiskStatus::Healthy);
        assert!(snapshot.health_factor_bps > 10_000);
    }
}
