use gravity_config::OracleConfig;
use gravity_types::{deviation_bps, median_price, now_ms, stable_hash_hex, weighted_price, Fixed, GravityError, MarketEvent, OracleReport, Price, Quantity, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
struct SourcePrice { price: Price, quantity: Option<Quantity>, sequence: u64, timestamp_ms: u64, kind: &'static str }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OracleSourceStatus { Accepted, Stale, Outlier, WaitingForQuorum }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleSourceView {
    pub symbol: Symbol,
    pub venue: String,
    pub price: Price,
    pub quantity: Option<Quantity>,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub age_ms: u64,
    pub deviation_bps: Option<u32>,
    pub status: OracleSourceStatus,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleStats {
    pub symbols: usize,
    pub sources: usize,
    pub reports: usize,
    pub accepted_sources: usize,
    pub stale_sources: usize,
    pub outlier_sources: usize,
    pub waiting_sources: usize,
    pub min_sources: usize,
    pub max_stale_ms: u64,
    pub max_deviation_bps: u32,
    pub method: String,
}

#[derive(Clone, Debug)]
pub struct OracleSigner {
    key_id: String,
    secret: String,
}

impl OracleSigner {
    pub fn new(key_id: impl Into<String>, secret: impl Into<String>) -> Self { Self { key_id: key_id.into(), secret: secret.into() } }
    pub fn key_id(&self) -> &str { &self.key_id }

    pub fn sign(&self, payload: &str) -> String { stable_hash_hex(&format!("{}:{}:{payload}", self.key_id, self.secret)) }

    pub fn verify(&self, payload: &str, signature: &str) -> bool { self.sign(payload) == signature }
}

#[derive(Clone, Debug)]
pub struct OracleEngine {
    config: OracleConfig,
    signer: Option<OracleSigner>,
    prices: BTreeMap<String, BTreeMap<String, SourcePrice>>,
    reports: BTreeMap<String, OracleReport>,
}

impl OracleEngine {
    pub fn new(config: OracleConfig) -> Self {
        let signer = if config.signing_enabled { Some(OracleSigner::new(config.signing_key_id.clone(), config.signing_secret.clone())) } else { None };
        Self { config, signer, prices: BTreeMap::new(), reports: BTreeMap::new() }
    }

    pub fn ingest(&mut self, event: MarketEvent) -> Result<Option<OracleReport>, GravityError> {
        let Some(price) = event.price() else { return Ok(None); };
        let symbol = event.symbol().clone();
        let venue = event.venue().to_string();
        let source = SourcePrice { price, quantity: event.quantity(), sequence: event.sequence(), timestamp_ms: event.timestamp_ms(), kind: event.kind() };
        self.prices.entry(symbol.0.clone()).or_default().insert(venue, source);
        let report = self.compute(&symbol)?;
        if let Some(report) = &report { self.reports.insert(symbol.0.clone(), report.clone()); }
        Ok(report)
    }

    pub fn latest(&self, symbol: &str) -> Option<&OracleReport> { self.reports.get(symbol) }

    pub fn all(&self) -> Vec<OracleReport> { self.reports.values().cloned().collect() }

    pub fn verify(&self, report: &OracleReport) -> bool {
        let Some(signature) = &report.signature else { return !self.config.signing_enabled; };
        let Some(signer) = &self.signer else { return false; };
        signer.verify(&report.signing_payload(), signature)
    }

    pub fn source_health(&self) -> Vec<OracleSourceView> {
        let now = now_ms();
        let mut out = Vec::new();
        for (symbol_key, by_source) in &self.prices {
            let Ok(symbol) = Symbol::new(symbol_key.clone()) else { continue; };
            let report = self.reports.get(symbol_key);
            let quorum_ready = by_source.len() >= self.config.min_sources;
            for (venue, source) in by_source {
                let age_ms = now.saturating_sub(source.timestamp_ms);
                let deviation = report.map(|r| deviation_bps(r.price, source.price));
                let status = if age_ms > self.config.max_stale_ms {
                    OracleSourceStatus::Stale
                } else if !quorum_ready {
                    OracleSourceStatus::WaitingForQuorum
                } else if deviation.map_or(false, |d| d > self.config.max_deviation_bps) {
                    OracleSourceStatus::Outlier
                } else {
                    OracleSourceStatus::Accepted
                };
                out.push(OracleSourceView {
                    symbol: symbol.clone(),
                    venue: venue.clone(),
                    price: source.price,
                    quantity: source.quantity,
                    sequence: source.sequence,
                    timestamp_ms: source.timestamp_ms,
                    age_ms,
                    deviation_bps: deviation,
                    status,
                    kind: source.kind.to_string(),
                });
            }
        }
        out.sort_by(|a, b| a.symbol.0.cmp(&b.symbol.0).then(a.venue.cmp(&b.venue)));
        out
    }

    pub fn stats(&self) -> OracleStats {
        let health = self.source_health();
        OracleStats {
            symbols: self.prices.len(),
            sources: health.len(),
            reports: self.reports.len(),
            accepted_sources: health.iter().filter(|v| v.status == OracleSourceStatus::Accepted).count(),
            stale_sources: health.iter().filter(|v| v.status == OracleSourceStatus::Stale).count(),
            outlier_sources: health.iter().filter(|v| v.status == OracleSourceStatus::Outlier).count(),
            waiting_sources: health.iter().filter(|v| v.status == OracleSourceStatus::WaitingForQuorum).count(),
            min_sources: self.config.min_sources,
            max_stale_ms: self.config.max_stale_ms,
            max_deviation_bps: self.config.max_deviation_bps,
            method: self.config.method.clone(),
        }
    }

    fn compute(&self, symbol: &Symbol) -> Result<Option<OracleReport>, GravityError> {
        let Some(by_source) = self.prices.get(&symbol.0) else { return Ok(None); };
        let now = now_ms();
        let mut prices = Vec::new();
        let mut weighted = Vec::new();
        let mut source_digest = String::new();
        for (venue, source) in by_source {
            if now.saturating_sub(source.timestamp_ms) <= self.config.max_stale_ms {
                prices.push(source.price);
                if let Some(quantity) = source.quantity { weighted.push((source.price, quantity)); }
                source_digest.push_str(&format!("{venue}:{}:{}:{}:{};", source.price, source.sequence, source.timestamp_ms, source.kind));
            }
        }
        if prices.len() < self.config.min_sources { return Ok(None); }
        let Some(median) = median_price(prices.clone()) else { return Ok(None); };
        let accepted = prices.into_iter().filter(|p| deviation_bps(median, *p) <= self.config.max_deviation_bps).collect::<Vec<_>>();
        if accepted.len() < self.config.min_sources { return Ok(None); }
        let filtered = match self.config.method.as_str() {
            "vwap" => weighted_price(&weighted).or_else(|| median_price(accepted.clone())),
            "twap" => twap_price(&accepted),
            "ewma" => ewma_price(&accepted),
            "median-vwap" => weighted_price(&weighted).or_else(|| median_price(accepted.clone())),
            _ => median_price(accepted.clone()),
        };
        let Some(price) = filtered else { return Ok(None); };
        let confidence_bps = self.confidence(accepted.len(), by_source.len(), &accepted, price);
        if confidence_bps < self.config.min_confidence_bps { return Ok(None); }
        let payload_hash = stable_hash_hex(&format!("{}:{price}:{confidence_bps}:{}:{}:{source_digest}", symbol, accepted.len(), now));
        let mut report = OracleReport {
            symbol: symbol.clone(),
            price,
            confidence_bps,
            sources: accepted.len() as u32,
            method: self.config.method.clone(),
            timestamp_ms: now,
            key_id: self.signer.as_ref().map(|s| s.key_id().to_string()),
            payload_hash,
            signature: None,
        };
        if let Some(signer) = &self.signer {
            report.signature = Some(signer.sign(&report.signing_payload()));
        }
        Ok(Some(report))
    }

    fn confidence(&self, accepted: usize, total: usize, prices: &[Price], final_price: Price) -> u32 {
        let quorum = ((accepted.saturating_mul(10_000) / self.config.min_sources.max(1)) as u32).min(10_000);
        let coverage = if total == 0 { 0 } else { ((accepted.saturating_mul(10_000) / total) as u32).min(10_000) };
        let worst_deviation = prices.iter().map(|p| deviation_bps(final_price, *p)).max().unwrap_or(0);
        let deviation_penalty = worst_deviation.min(self.config.max_deviation_bps).saturating_mul(10_000) / self.config.max_deviation_bps.max(1);
        let score = (quorum as u64 * 40 + coverage as u64 * 40 + (10_000_u32.saturating_sub(deviation_penalty)) as u64 * 20) / 100;
        score.min(10_000) as u32
    }
}

fn twap_price(prices: &[Price]) -> Option<Price> { median_price(prices.to_vec()) }

fn ewma_price(prices: &[Price]) -> Option<Price> {
    let mut iter = prices.iter();
    let first = *iter.next()?;
    let mut acc = first.0;
    for price in iter { acc = (acc * Fixed::raw(7_000_000) + price.0 * Fixed::raw(3_000_000)) / Fixed::from_units(10); }
    Price::new(acc).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gravity_types::{Fixed, Price, Quantity, Side, Symbol, Trade};

    #[test]
    fn waits_for_min_sources() {
        let mut cfg = OracleConfig::default();
        cfg.min_sources = 3;
        let mut engine = OracleEngine::new(cfg);
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let trade = |venue: &str, price: i128| MarketEvent::Trade(Trade {
            symbol: symbol.clone(), venue: venue.into(), price: Price::new(Fixed::from_units(price)).unwrap(),
            quantity: Quantity::new(Fixed::from_units(1)).unwrap(), side: Side::Buy, sequence: 1, timestamp_ms: now_ms(),
        });
        assert!(engine.ingest(trade("a", 100)).unwrap().is_none());
        assert!(engine.ingest(trade("b", 101)).unwrap().is_none());
        assert!(engine.ingest(trade("c", 102)).unwrap().is_some());
    }

    #[test]
    fn signed_reports_verify() {
        let mut cfg = OracleConfig::default();
        cfg.min_sources = 1;
        cfg.signing_secret = "test".into();
        let mut engine = OracleEngine::new(cfg);
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let event = MarketEvent::Trade(Trade {
            symbol, venue: "a".into(), price: Price::new(Fixed::from_units(100)).unwrap(),
            quantity: Quantity::new(Fixed::from_units(1)).unwrap(), side: Side::Buy, sequence: 1, timestamp_ms: now_ms(),
        });
        let report = engine.ingest(event).unwrap().unwrap();
        assert!(engine.verify(&report));
        assert_eq!(engine.stats().reports, 1);
    }

    #[test]
    fn marks_outlier_source() {
        let mut cfg = OracleConfig::default();
        cfg.min_sources = 2;
        cfg.max_deviation_bps = 500;
        let mut engine = OracleEngine::new(cfg);
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let trade = |venue: &str, price: i128| MarketEvent::Trade(Trade {
            symbol: symbol.clone(), venue: venue.into(), price: Price::new(Fixed::from_units(price)).unwrap(),
            quantity: Quantity::new(Fixed::from_units(1)).unwrap(), side: Side::Buy, sequence: 1, timestamp_ms: now_ms(),
        });
        let _ = engine.ingest(trade("a", 100));
        let _ = engine.ingest(trade("b", 101));
        let _ = engine.ingest(trade("c", 500));
        let health = engine.source_health();
        assert!(health.iter().any(|v| v.venue == "c" && v.status == OracleSourceStatus::Outlier));
    }
}
