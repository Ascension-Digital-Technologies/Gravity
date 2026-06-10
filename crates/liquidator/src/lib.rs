//! Liquidation runtime for Gravity.
//!
//! The liquidator consumes risk snapshots and prepares deterministic liquidation
//! candidates/plans. Gravity still only prepares and routes liquidation payloads;
//! Stargate/L3 remains the final settlement truth.

use gravity_risk::{AccountRiskSnapshot, RiskStatus};
use gravity_types::{Fixed, GravityError, Symbol, now_ms, stable_hash_hex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidationMode { Partial, Full }

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidationCandidateStatus { Candidate, Planned, Submitted, Finalized, Rejected, DeadLetter }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationConfig {
    pub liquidation_health_bps: i128,
    pub partial_close_bps: u32,
    pub max_close_bps: u32,
    pub min_profit_bps: u32,
    pub stale_oracle_ms: u64,
    pub candidate_limit: usize,
    pub event_limit: usize,
}

impl Default for LiquidationConfig {
    fn default() -> Self {
        Self {
            liquidation_health_bps: 10_000,
            partial_close_bps: 2_500,
            max_close_bps: 10_000,
            min_profit_bps: 25,
            stale_oracle_ms: 5_000,
            candidate_limit: 100_000,
            event_limit: 100_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationCandidate {
    pub id: String,
    pub account: String,
    pub status: LiquidationCandidateStatus,
    pub health_factor_bps: i128,
    pub equity: Fixed,
    pub debt_value: Fixed,
    pub maintenance_margin_required: Fixed,
    pub deficit_value: Fixed,
    pub priority_score: i128,
    pub oracle_dependencies: Vec<Symbol>,
    pub snapshot_id: String,
    pub timestamp_ms: u64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationPlan {
    pub id: String,
    pub candidate_id: String,
    pub account: String,
    pub mode: LiquidationMode,
    pub close_bps: u32,
    pub repay_value: Fixed,
    pub seize_value: Fixed,
    pub estimated_profit_value: Fixed,
    pub settlement_hint: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationEvent {
    pub id: String,
    pub account: String,
    pub kind: String,
    pub candidate_id: Option<String>,
    pub plan_id: Option<String>,
    pub timestamp_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiquidationStats {
    pub candidates: usize,
    pub events: usize,
    pub scanned: u64,
    pub planned: u64,
    pub submitted: u64,
    pub finalized: u64,
    pub rejected: u64,
    pub dead_letter: u64,
    pub last_updated_ms: u64,
}

#[derive(Clone, Debug)]
pub struct LiquidationEngine {
    config: LiquidationConfig,
    candidates: BTreeMap<String, LiquidationCandidate>,
    account_index: BTreeMap<String, String>,
    events: VecDeque<LiquidationEvent>,
    stats: LiquidationStats,
}

impl Default for LiquidationEngine { fn default() -> Self { Self::new(LiquidationConfig::default()) } }

impl LiquidationEngine {
    pub fn new(config: LiquidationConfig) -> Self {
        Self {
            config,
            candidates: BTreeMap::new(),
            account_index: BTreeMap::new(),
            events: VecDeque::with_capacity(100_000),
            stats: LiquidationStats { candidates: 0, events: 0, scanned: 0, planned: 0, submitted: 0, finalized: 0, rejected: 0, dead_letter: 0, last_updated_ms: 0 },
        }
    }

    pub fn scan(&mut self, snapshots: Vec<AccountRiskSnapshot>, limit: usize) -> Result<Vec<LiquidationCandidate>, GravityError> {
        self.stats.scanned = self.stats.scanned.saturating_add(snapshots.len() as u64);
        let mut out = Vec::new();
        for snapshot in snapshots {
            if !self.is_liquidatable(&snapshot) { continue; }
            let candidate = self.candidate_from_snapshot(&snapshot)?;
            self.account_index.insert(candidate.account.clone(), candidate.id.clone());
            self.candidates.insert(candidate.id.clone(), candidate.clone());
            self.push_event(LiquidationEvent {
                id: stable_hash_hex(&format!("liq-event:candidate:{}:{}", candidate.id, candidate.timestamp_ms)),
                account: candidate.account.clone(),
                kind: "candidate".into(),
                candidate_id: Some(candidate.id.clone()),
                plan_id: None,
                timestamp_ms: candidate.timestamp_ms,
                message: candidate.reason.clone(),
            });
            out.push(candidate);
            if out.len() >= limit.min(self.config.candidate_limit) { break; }
        }
        out.sort_by(|a,b| b.priority_score.cmp(&a.priority_score).then(a.account.cmp(&b.account)));
        self.update_stats();
        Ok(out)
    }

    pub fn plan_for_account(&mut self, account: &str, mode: LiquidationMode) -> Result<Option<LiquidationPlan>, GravityError> {
        let Some(candidate_id) = self.account_index.get(account).cloned() else { return Ok(None); };
        let Some(candidate) = self.candidates.get(&candidate_id).cloned() else { return Ok(None); };
        let plan = self.plan(candidate, mode)?;
        if let Some(candidate) = self.candidates.get_mut(&candidate_id) { candidate.status = LiquidationCandidateStatus::Planned; }
        self.stats.planned = self.stats.planned.saturating_add(1);
        self.push_event(LiquidationEvent {
            id: stable_hash_hex(&format!("liq-event:plan:{}:{}", plan.id, plan.timestamp_ms)),
            account: plan.account.clone(),
            kind: "plan".into(),
            candidate_id: Some(plan.candidate_id.clone()),
            plan_id: Some(plan.id.clone()),
            timestamp_ms: plan.timestamp_ms,
            message: format!("liquidation plan {:?} close_bps={}", plan.mode, plan.close_bps),
        });
        self.update_stats();
        Ok(Some(plan))
    }

    pub fn candidates(&self, limit: usize) -> Vec<LiquidationCandidate> {
        let mut out = self.candidates.values().cloned().collect::<Vec<_>>();
        out.sort_by(|a,b| b.priority_score.cmp(&a.priority_score).then(a.account.cmp(&b.account)));
        out.truncate(limit.min(self.config.candidate_limit));
        out
    }

    pub fn events(&self, limit: usize) -> Vec<LiquidationEvent> {
        let mut out = self.events.iter().rev().take(limit.min(self.config.event_limit)).cloned().collect::<Vec<_>>();
        out.reverse();
        out
    }

    pub fn stats(&self) -> LiquidationStats { self.stats.clone() }

    fn is_liquidatable(&self, snapshot: &AccountRiskSnapshot) -> bool {
        snapshot.status == RiskStatus::Liquidatable || snapshot.health_factor_bps <= self.config.liquidation_health_bps
    }

    fn candidate_from_snapshot(&self, snapshot: &AccountRiskSnapshot) -> Result<LiquidationCandidate, GravityError> {
        let timestamp_ms = now_ms();
        let deficit_value = if snapshot.maintenance_margin_required.0 > snapshot.equity.0 { snapshot.maintenance_margin_required.checked_sub(snapshot.equity)? } else { Fixed::ZERO };
        let priority_score = self.priority_score(snapshot, deficit_value)?;
        let id = stable_hash_hex(&format!("liq-candidate:{}:{}:{}:{}", snapshot.account, snapshot.id, snapshot.health_factor_bps, timestamp_ms));
        Ok(LiquidationCandidate {
            id,
            account: snapshot.account.clone(),
            status: LiquidationCandidateStatus::Candidate,
            health_factor_bps: snapshot.health_factor_bps,
            equity: snapshot.equity,
            debt_value: snapshot.debt_value,
            maintenance_margin_required: snapshot.maintenance_margin_required,
            deficit_value,
            priority_score,
            oracle_dependencies: snapshot.oracle_dependencies.clone(),
            snapshot_id: snapshot.id.clone(),
            timestamp_ms,
            reason: format!("account health {} bps is liquidatable", snapshot.health_factor_bps),
        })
    }

    fn priority_score(&self, snapshot: &AccountRiskSnapshot, deficit_value: Fixed) -> Result<i128, GravityError> {
        let health_gap = self.config.liquidation_health_bps.saturating_sub(snapshot.health_factor_bps).max(0);
        let deficit_units = deficit_value.0.checked_div(gravity_types::SCALE).unwrap_or(0).max(0);
        Ok(health_gap.saturating_mul(1_000_000).saturating_add(deficit_units))
    }

    fn plan(&self, candidate: LiquidationCandidate, mode: LiquidationMode) -> Result<LiquidationPlan, GravityError> {
        let close_bps = match mode { LiquidationMode::Partial => self.config.partial_close_bps.min(self.config.max_close_bps), LiquidationMode::Full => self.config.max_close_bps };
        let repay_base = if candidate.debt_value.0 > 0 { candidate.debt_value } else { candidate.maintenance_margin_required };
        let repay_value = apply_bps(repay_base, close_bps)?;
        let bonus_bps = self.config.min_profit_bps.saturating_add(50);
        let seize_value = repay_value.checked_add(apply_bps(repay_value, bonus_bps)?)?;
        let estimated_profit_value = seize_value.checked_sub(repay_value)?;
        let timestamp_ms = now_ms();
        let id = stable_hash_hex(&format!("liq-plan:{}:{:?}:{}:{}", candidate.id, mode, close_bps, timestamp_ms));
        Ok(LiquidationPlan {
            id: id.clone(),
            candidate_id: candidate.id,
            account: candidate.account,
            mode,
            close_bps,
            repay_value,
            seize_value,
            estimated_profit_value,
            settlement_hint: format!("liquidation:{}:{}", id, close_bps),
            timestamp_ms,
        })
    }

    fn push_event(&mut self, event: LiquidationEvent) {
        if self.events.len() >= self.config.event_limit { self.events.pop_front(); }
        self.events.push_back(event);
        self.update_stats();
    }

    fn update_stats(&mut self) {
        let mut submitted = 0;
        let mut finalized = 0;
        let mut rejected = 0;
        let mut dead_letter = 0;
        for candidate in self.candidates.values() {
            match candidate.status {
                LiquidationCandidateStatus::Submitted => submitted += 1,
                LiquidationCandidateStatus::Finalized => finalized += 1,
                LiquidationCandidateStatus::Rejected => rejected += 1,
                LiquidationCandidateStatus::DeadLetter => dead_letter += 1,
                _ => {}
            }
        }
        self.stats.candidates = self.candidates.len();
        self.stats.events = self.events.len();
        self.stats.submitted = submitted;
        self.stats.finalized = finalized;
        self.stats.rejected = rejected;
        self.stats.dead_letter = dead_letter;
        self.stats.last_updated_ms = now_ms();
    }
}

fn apply_bps(value: Fixed, bps: u32) -> Result<Fixed, GravityError> {
    value.0.checked_mul(i128::from(bps)).and_then(|v| v.checked_div(10_000)).map(Fixed::raw).ok_or(GravityError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gravity_risk::{AccountRiskSnapshot, RiskStatus};

    #[test]
    fn liquidatable_snapshot_becomes_candidate() {
        let mut engine = LiquidationEngine::default();
        let snapshot = AccountRiskSnapshot {
            id: "risk-1".into(), account: "acct-1".into(), status: RiskStatus::Liquidatable,
            collateral_value: Fixed::from_units(100), discounted_collateral_value: Fixed::from_units(90),
            position_notional: Fixed::from_units(1000), debt_value: Fixed::from_units(500),
            initial_margin_required: Fixed::from_units(200), maintenance_margin_required: Fixed::from_units(100),
            equity: Fixed::from_units(50), free_collateral: Fixed::from_units(-150), health_factor_bps: 5000,
            leverage_bps: 20_000, oracle_dependencies: vec![], warnings: vec![], timestamp_ms: 1,
        };
        let candidates = engine.scan(vec![snapshot], 10).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(engine.plan_for_account("acct-1", LiquidationMode::Partial).unwrap().is_some());
    }
}
