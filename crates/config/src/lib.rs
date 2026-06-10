use gravity_types::{GravityError, Market};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct GravityConfig {
    pub name: String,
    pub bind: String,
    pub market_queue: usize,
    pub worker_tick_ms: u64,
    pub settlement_endpoint: String,
    pub storage_mode: String,
    pub postgres_url: String,
    pub redis_url: String,
}

impl Default for GravityConfig {
    fn default() -> Self {
        Self {
            name: "gravity".to_string(),
            bind: "127.0.0.1:8787".to_string(),
            market_queue: 65_536,
            worker_tick_ms: 250,
            settlement_endpoint: "127.0.0.1:9099".to_string(),
            storage_mode: "memory".to_string(),
            postgres_url: "postgres://gravity:gravity@127.0.0.1:5432/gravity".to_string(),
            redis_url: "redis://127.0.0.1:6379".to_string(),
        }
    }
}

impl GravityConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, GravityError> {
        let path = path.as_ref();
        if !path.exists() { return Ok(Self::default()); }
        let raw = fs::read_to_string(path)?;
        let mut cfg = Self::default();
        for line in raw.lines() {
            let line = clean(line);
            if line.is_empty() || line.starts_with('[') { continue; }
            let Some((key, value)) = line.split_once('=') else { continue; };
            let key = key.trim();
            let value = trim(value);
            match key {
                "name" => cfg.name = value,
                "bind" => cfg.bind = value,
                "market_queue" => cfg.market_queue = number(&value, "market_queue")?,
                "worker_tick_ms" => cfg.worker_tick_ms = number(&value, "worker_tick_ms")?,
                "settlement_endpoint" => cfg.settlement_endpoint = value,
                "storage_mode" => cfg.storage_mode = value,
                "postgres_url" => cfg.postgres_url = value,
                "redis_url" => cfg.redis_url = value,
                _ => {}
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), GravityError> {
        if self.name.trim().is_empty() { return Err(GravityError::InvalidConfig("name is required".into())); }
        if !self.bind.contains(':') { return Err(GravityError::InvalidConfig("bind must include host:port".into())); }
        if self.market_queue < 1024 { return Err(GravityError::InvalidConfig("market_queue must be at least 1024".into())); }
        if self.worker_tick_ms == 0 { return Err(GravityError::InvalidConfig("worker_tick_ms must be positive".into())); }
        match self.storage_mode.as_str() {
            "memory" | "postgres" | "postgres-redis" => {}
            _ => return Err(GravityError::InvalidConfig("storage_mode must be memory, postgres, or postgres-redis".into())),
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct OracleConfig {
    pub method: String,
    pub min_sources: usize,
    pub max_stale_ms: u64,
    pub max_deviation_bps: u32,
    pub report_ttl_ms: u64,
    pub signing_enabled: bool,
    pub signing_key_id: String,
    pub signing_secret: String,
    pub min_confidence_bps: u32,
    pub source_weight_bps: u32,
    pub stale_penalty_bps: u32,
    pub outlier_penalty_bps: u32,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            method: "median".into(),
            min_sources: 3,
            max_stale_ms: 5000,
            max_deviation_bps: 250,
            report_ttl_ms: 10000,
            signing_enabled: true,
            signing_key_id: "gravity-local-dev".into(),
            signing_secret: "replace-me-before-production".into(),
            min_confidence_bps: 6_000,
            source_weight_bps: 1_500,
            stale_penalty_bps: 2_000,
            outlier_penalty_bps: 3_000,
        }
    }
}

impl OracleConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, GravityError> {
        let path = path.as_ref();
        if !path.exists() { return Ok(Self::default()); }
        let raw = fs::read_to_string(path)?;
        let mut cfg = Self::default();
        for line in raw.lines() {
            let line = clean(line);
            if line.is_empty() || line.starts_with('[') { continue; }
            let Some((key, value)) = line.split_once('=') else { continue; };
            let key = key.trim();
            let value = trim(value);
            match key {
                "method" => cfg.method = value,
                "min_sources" => cfg.min_sources = number(&value, "min_sources")?,
                "max_stale_ms" => cfg.max_stale_ms = number(&value, "max_stale_ms")?,
                "max_deviation_bps" => cfg.max_deviation_bps = number(&value, "max_deviation_bps")?,
                "report_ttl_ms" => cfg.report_ttl_ms = number(&value, "report_ttl_ms")?,
                "signing_enabled" => cfg.signing_enabled = boolean(&value, "signing_enabled")?,
                "signing_key_id" => cfg.signing_key_id = value,
                "signing_secret" => cfg.signing_secret = value,
                "min_confidence_bps" => cfg.min_confidence_bps = number(&value, "min_confidence_bps")?,
                "source_weight_bps" => cfg.source_weight_bps = number(&value, "source_weight_bps")?,
                "stale_penalty_bps" => cfg.stale_penalty_bps = number(&value, "stale_penalty_bps")?,
                "outlier_penalty_bps" => cfg.outlier_penalty_bps = number(&value, "outlier_penalty_bps")?,
                _ => {}
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), GravityError> {
        match self.method.as_str() {
            "median" | "vwap" | "twap" | "ewma" | "median-vwap" => {}
            _ => return Err(GravityError::InvalidConfig("method must be median, vwap, twap, ewma, or median-vwap".into())),
        }
        if self.min_sources == 0 { return Err(GravityError::InvalidConfig("min_sources must be positive".into())); }
        if self.max_stale_ms == 0 { return Err(GravityError::InvalidConfig("max_stale_ms must be positive".into())); }
        if self.min_confidence_bps > 10_000 { return Err(GravityError::InvalidConfig("min_confidence_bps must be <= 10000".into())); }
        if self.source_weight_bps > 10_000 { return Err(GravityError::InvalidConfig("source_weight_bps must be <= 10000".into())); }
        if self.stale_penalty_bps > 10_000 { return Err(GravityError::InvalidConfig("stale_penalty_bps must be <= 10000".into())); }
        if self.outlier_penalty_bps > 10_000 { return Err(GravityError::InvalidConfig("outlier_penalty_bps must be <= 10000".into())); }
        if self.signing_enabled && self.signing_secret.trim().is_empty() {
            return Err(GravityError::InvalidConfig("signing_secret is required when oracle signing is enabled".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FeedConfig {
    pub enabled: bool,
    pub adapter_mode: String,
    pub reconnect_ms: u64,
    pub max_gap: u64,
    pub venues: Vec<String>,
}

impl Default for FeedConfig {
    fn default() -> Self {
        Self { enabled: true, adapter_mode: "replay".into(), reconnect_ms: 1000, max_gap: 100, venues: vec!["binance".into(), "coinbase".into(), "kraken".into()] }
    }
}

impl FeedConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, GravityError> {
        let path = path.as_ref();
        if !path.exists() { return Ok(Self::default()); }
        let raw = fs::read_to_string(path)?;
        let mut cfg = Self::default();
        for line in raw.lines() {
            let line = clean(line);
            if line.is_empty() || line.starts_with('[') { continue; }
            let Some((key, value)) = line.split_once('=') else { continue; };
            let key = key.trim();
            let value = trim(value);
            match key {
                "enabled" => cfg.enabled = boolean(&value, "enabled")?,
                "adapter_mode" => cfg.adapter_mode = value,
                "reconnect_ms" => cfg.reconnect_ms = number(&value, "reconnect_ms")?,
                "max_gap" => cfg.max_gap = number(&value, "max_gap")?,
                "venues" => cfg.venues = split_list(&value),
                _ => {}
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<(), GravityError> {
        if self.adapter_mode != "replay" && self.adapter_mode != "live" && self.adapter_mode != "mock" { return Err(GravityError::InvalidConfig("adapter_mode must be replay, live, or mock".into())); }
        if self.reconnect_ms == 0 { return Err(GravityError::InvalidConfig("reconnect_ms must be positive".into())); }
        if self.venues.is_empty() { return Err(GravityError::InvalidConfig("at least one venue is required".into())); }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct GravityPaths { pub root: PathBuf, pub config: PathBuf, pub runtime: PathBuf }

impl GravityPaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { config: root.join("config"), runtime: root.join("runtime"), root }
    }
}

pub fn default_markets() -> Result<Vec<Market>, GravityError> {
    Ok(vec![Market::new("BTC-USDx", "BTC", "USDx")?, Market::new("ETH-USDx", "ETH", "USDx")?])
}

fn clean(line: &str) -> String { line.split('#').next().unwrap_or("").trim().to_string() }
fn trim(value: &str) -> String { value.trim().trim_matches('"').to_string() }

fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, GravityError> {
    value.parse().map_err(|_| GravityError::InvalidConfig(format!("{name} must be a number")))
}

fn boolean(value: &str, name: &str) -> Result<bool, GravityError> {
    match value {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(GravityError::InvalidConfig(format!("{name} must be true or false"))),
    }
}

fn split_list(value: &str) -> Vec<String> {
    value.trim_matches('[').trim_matches(']').split(',').map(|v| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty()).collect()
}
