use dashmap::DashMap;
use gravity_book::{AmendRequest, AmendResult, BookEvent, BookEventKind, BookSnapshot, CancelResult, MarketStatus, OrderBook, OrderRequest, OrderResult, ReplaceResult};
use gravity_amm::{AmmBook, AmmGuardResult, LiquidityResult, PoolConfig, PoolEvent, PoolSnapshot, RemoveLiquidityResult, SwapQuote, SwapResult, SwapSide};
use gravity_risk::{AccountRiskInput, AccountRiskSnapshot, RiskEngine, RiskEvent, RiskStats};
use gravity_liquidator::{LiquidationCandidate, LiquidationEngine, LiquidationEvent, LiquidationMode, LiquidationPlan, LiquidationStats};
use gravity_perps::{FundingUpdateRequest, PerpEngine, PerpEvent, PerpMarketConfig, PerpMarketSnapshot, PerpPosition, PerpPositionRequest, PerpStats};
use gravity_index::{IndexEngine, IndexEvent, IndexNavReport, IndexProductConfig, IndexProductSnapshot, IndexStats, MintRedeemPlan, RebalancePlan};
use gravity_settlement::{SettlementClient, SettlementFinalizationRecord, SettlementStats};
use gravity_wal::{CheckpointRecord, RecoveryReport, ReplayPlan, WalManager, WalRecord, WalStats};
use gravity_stream::{StreamHub, StreamStats, StreamRecord};
use gravity_tile::{TileRuntimeSnapshot, TileSupervisor};
use gravity_hardware::{build_plan, simulate, HardwarePlan, HardwareProfile, PlacementSimulation, RuntimeProfile};
use gravity_oracle::{OracleSourceView, OracleStats, OracleSourceStatus};
use gravity_types::{AuditRecord, Fixed, GravityError, MarketEvent, OracleReport, Price, Quantity, Symbol, now_ms};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, oneshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreMode { Memory, Postgres, PostgresRedis }

impl StoreMode {
    pub fn parse(value: &str) -> Result<Self, GravityError> {
        match value {
            "memory" => Ok(Self::Memory),
            "postgres" => Ok(Self::Postgres),
            "postgres-redis" => Ok(Self::PostgresRedis),
            other => Err(GravityError::InvalidConfig(format!("unsupported storage mode: {other}"))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoragePlan {
    pub mode: StoreMode,
    pub postgres_url: String,
    pub redis_url: String,
}

impl StoragePlan {
    pub fn new(mode: impl AsRef<str>, postgres_url: impl Into<String>, redis_url: impl Into<String>) -> Result<Self, GravityError> {
        Ok(Self { mode: StoreMode::parse(mode.as_ref())?, postgres_url: postgres_url.into(), redis_url: redis_url.into() })
    }

    pub fn summary(&self) -> String {
        format!("mode={:?} postgres={} redis={}", self.mode, redact_url(&self.postgres_url), redact_url(&self.redis_url))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketWorkerStats {
    pub symbol: Symbol,
    pub commands: u64,
    pub submitted: u64,
    pub canceled: u64,
    pub snapshots: u64,
    pub fills: u64,
    pub rejected: u64,
    pub max_batch: usize,
    pub last_latency_us: u64,
    pub max_latency_us: u64,
    pub avg_latency_us: u64,
    pub total_latency_us: u64,
    pub last_sequence: u64,
    pub queue_capacity: usize,
    pub queue_available: usize,
    pub queue_depth: usize,
    pub started_ms: u64,
    pub updated_ms: u64,
}

impl MarketWorkerStats {
    fn new(symbol: Symbol, capacity: usize) -> Self {
        let now = now_ms();
        Self {
            symbol,
            commands: 0,
            submitted: 0,
            canceled: 0,
            snapshots: 0,
            fills: 0,
            rejected: 0,
            max_batch: 0,
            last_latency_us: 0,
            max_latency_us: 0,
            avg_latency_us: 0,
            total_latency_us: 0,
            last_sequence: 0,
            queue_capacity: capacity,
            queue_available: capacity,
            queue_depth: 0,
            started_ms: now,
            updated_ms: now,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistenceRecord {
    pub kind: String,
    pub target: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub body: Value,
}

impl PersistenceRecord {
    fn new(kind: impl Into<String>, target: impl Into<String>, sequence: u64, body: Value) -> Self {
        Self { kind: kind.into(), target: target.into(), sequence, timestamp_ms: now_ms(), body }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistenceStats {
    pub queued: usize,
    pub capacity: usize,
    pub dropped: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatabaseHealthReport {
    pub mode: String,
    pub postgres_enabled: bool,
    pub postgres_ok: bool,
    pub redis_enabled: bool,
    pub redis_ok: bool,
    pub migration_count: usize,
    pub missing_migrations: Vec<String>,
    pub persistence: PersistenceStats,
    pub checked_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationStatus {
    pub expected: Vec<String>,
    pub applied: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageBackpressureReport {
    pub queued: usize,
    pub capacity: usize,
    pub available: usize,
    pub pressure_bps: u32,
    pub dropped: u64,
}

const REQUIRED_MIGRATIONS: &[&str] = &[
    "0001-oracle.sql",
    "0002-market-events.sql",
    "0003-counters.sql",
    "0004-orderbook.sql",
    "0005-book-events.sql",
    "0006-audit.sql",
    "0007-persistence.sql",
    "0008-clob-production.sql",
    "0009-settlement-finalization.sql",
    "0010-production-oracle.sql",
    "0011-amm.sql",
    "0012-risk.sql",
    "0013-liquidations.sql",
    "0014-wal.sql",
    "0015-amm-hardening.sql",
    "0016-perps.sql",
    "0017-index.sql",
    "0018-stream.sql",
    "0019-database-integration.sql",
];


#[derive(Clone)]
struct MarketHandle {
    tx: mpsc::Sender<BookCommand>,
    stats: Arc<Mutex<MarketWorkerStats>>,
}

enum BookCommand {
    Submit { req: OrderRequest, reply: oneshot::Sender<Result<OrderResult, GravityError>> },
    SubmitBatch { reqs: Vec<OrderRequest>, reply: oneshot::Sender<Result<Vec<OrderResult>, GravityError>> },
    Cancel { order_id: String, reply: oneshot::Sender<Result<CancelResult, GravityError>> },
    Amend { order_id: String, amend: AmendRequest, reply: oneshot::Sender<Result<AmendResult, GravityError>> },
    Replace { order_id: String, req: OrderRequest, reply: oneshot::Sender<Result<ReplaceResult, GravityError>> },
    Status { status: MarketStatus, reply: oneshot::Sender<Result<MarketStatus, GravityError>> },
    Snapshot { depth: usize, reply: oneshot::Sender<Result<BookSnapshot, GravityError>> },
}

#[derive(Clone)]
pub struct MemoryStore {
    oracle: Arc<DashMap<String, OracleReport>>,
    oracle_sources: Arc<DashMap<String, OracleSourceView>>,
    counters: Arc<DashMap<String, u64>>,
    events: Arc<DashMap<String, MarketEvent>>,
    workers: Arc<DashMap<String, MarketHandle>>,
    depth: Arc<DashMap<String, BookSnapshot>>,
    book_events: Arc<Mutex<VecDeque<BookEvent>>>,
    audit: Arc<Mutex<VecDeque<AuditRecord>>>,
    persist_queue: Arc<Mutex<VecDeque<PersistenceRecord>>>,
    amm: Arc<Mutex<AmmBook>>,
    risk: Arc<Mutex<RiskEngine>>,
    liquidator: Arc<Mutex<LiquidationEngine>>,
    perps: Arc<Mutex<PerpEngine>>,
    index: Arc<Mutex<IndexEngine>>,
    wal: WalManager,
    stream: StreamHub,
    tiles: TileSupervisor,
    persist_dropped: Arc<Mutex<u64>>,
    oracle_tx: broadcast::Sender<OracleReport>,
    book_tx: broadcast::Sender<BookEvent>,
    event_limit: usize,
    persist_limit: usize,
    worker_queue: usize,
}

impl Default for MemoryStore {
    fn default() -> Self {
        let (oracle_tx, _) = broadcast::channel(8192);
        let (book_tx, _) = broadcast::channel(32768);
        Self {
            oracle: Arc::new(DashMap::new()),
            oracle_sources: Arc::new(DashMap::new()),
            counters: Arc::new(DashMap::new()),
            events: Arc::new(DashMap::new()),
            workers: Arc::new(DashMap::new()),
            depth: Arc::new(DashMap::new()),
            book_events: Arc::new(Mutex::new(VecDeque::with_capacity(100_000))),
            audit: Arc::new(Mutex::new(VecDeque::with_capacity(100_000))),
            persist_queue: Arc::new(Mutex::new(VecDeque::with_capacity(262_144))),
            amm: Arc::new(Mutex::new(AmmBook::new())),
            risk: Arc::new(Mutex::new(RiskEngine::default())),
            liquidator: Arc::new(Mutex::new(LiquidationEngine::default())),
            perps: Arc::new(Mutex::new(PerpEngine::new())),
            index: Arc::new(Mutex::new(IndexEngine::new())),
            wal: WalManager::default(),
            stream: StreamHub::default(),
            tiles: TileSupervisor::default_runtime(),
            persist_dropped: Arc::new(Mutex::new(0)),
            oracle_tx,
            book_tx,
            event_limit: 100_000,
            persist_limit: 262_144,
            worker_queue: 65_536,
        }
    }
}

impl MemoryStore {
    pub async fn put_oracle(&self, report: OracleReport) -> Result<(), GravityError> {
        self.oracle.insert(report.symbol.0.clone(), report.clone());
        let _ = self.oracle_tx.send(report.clone());
        self.publish_stream_json("oracle", &report.symbol.0, report.timestamp_ms, serde_json::json!({ "type": "oracle_report", "report": report.clone() }))?;
        self.push_audit(AuditRecord::new("oracle", report.symbol.0.clone(), report.timestamp_ms, "oracle report stored"))?;
        self.enqueue_persistence(PersistenceRecord::new("oracle", report.symbol.0.clone(), report.timestamp_ms, serde_json::to_value(&report)?))?;
        self.bump("oracle_reports", 1).await?;
        Ok(())
    }

    pub async fn get_oracle(&self, symbol: &str) -> Result<Option<OracleReport>, GravityError> {
        Ok(self.oracle.get(symbol).map(|v| v.value().clone()))
    }

    pub async fn all_oracles(&self) -> Result<Vec<OracleReport>, GravityError> {
        let mut reports = self.oracle.iter().map(|v| v.value().clone()).collect::<Vec<_>>();
        reports.sort_by(|a, b| a.symbol.0.cmp(&b.symbol.0));
        Ok(reports)
    }

    pub async fn put_oracle_sources(&self, sources: Vec<OracleSourceView>) -> Result<(), GravityError> {
        for source in sources {
            let key = format!("{}:{}", source.symbol, source.venue);
            self.oracle_sources.insert(key, source.clone());
            self.enqueue_persistence(PersistenceRecord::new("oracle_source", source.symbol.0.clone(), source.sequence, serde_json::to_value(&source)?))?;
        }
        Ok(())
    }

    pub async fn oracle_sources(&self) -> Result<Vec<OracleSourceView>, GravityError> {
        let mut sources = self.oracle_sources.iter().map(|v| v.value().clone()).collect::<Vec<_>>();
        sources.sort_by(|a, b| a.symbol.0.cmp(&b.symbol.0).then(a.venue.cmp(&b.venue)));
        Ok(sources)
    }

    pub async fn oracle_stats(&self) -> Result<OracleStats, GravityError> {
        let reports = self.all_oracles().await?;
        let sources = self.oracle_sources().await?;
        Ok(OracleStats {
            symbols: reports.len(),
            sources: sources.len(),
            reports: reports.len(),
            accepted_sources: sources.iter().filter(|v| v.status == OracleSourceStatus::Accepted).count(),
            stale_sources: sources.iter().filter(|v| v.status == OracleSourceStatus::Stale).count(),
            outlier_sources: sources.iter().filter(|v| v.status == OracleSourceStatus::Outlier).count(),
            waiting_sources: sources.iter().filter(|v| v.status == OracleSourceStatus::WaitingForQuorum).count(),
            min_sources: 0,
            max_stale_ms: 0,
            max_deviation_bps: 0,
            method: "store".into(),
        })
    }

    pub async fn put_market_event(&self, event: MarketEvent) -> Result<(), GravityError> {
        let key = format!("{}:{}:{}", event.venue(), event.symbol(), event.sequence());
        self.events.insert(key, event.clone());
        self.push_audit(AuditRecord::new("market_event", event.symbol().0.clone(), event.sequence(), event.kind()))?;
        self.enqueue_persistence(PersistenceRecord::new("market_event", event.symbol().0.clone(), event.sequence(), serde_json::to_value(&event)?))?;
        self.bump("market_events", 1).await?;
        Ok(())
    }

    pub async fn submit_order(&self, req: OrderRequest) -> Result<OrderResult, GravityError> {
        let symbol = req.symbol.clone();
        let worker = self.worker_for(symbol).await?;
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Submit { req, reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped submit reply".into()))?
    }

    pub async fn submit_orders(&self, orders: Vec<OrderRequest>) -> Result<Vec<OrderResult>, GravityError> {
        if orders.is_empty() { return Ok(Vec::new()); }
        let total = orders.len();
        let mut grouped: BTreeMap<String, Vec<(usize, OrderRequest)>> = BTreeMap::new();
        for (index, req) in orders.into_iter().enumerate() {
            grouped.entry(req.symbol.0.clone()).or_default().push((index, req));
        }

        // Parallel execution happens naturally here: each symbol gets one SubmitBatch
        // command sent before replies are awaited, so independent market workers can
        // execute on separate Tokio worker threads at the same time.
        let mut pending = Vec::with_capacity(grouped.len());
        for (_, entries) in grouped {
            let symbol = entries[0].1.symbol.clone();
            let worker = self.worker_for(symbol).await?;
            let (indexes, reqs): (Vec<_>, Vec<_>) = entries.into_iter().unzip();
            let (reply, rx) = oneshot::channel();
            worker.tx.try_send(BookCommand::SubmitBatch { reqs, reply }).map_err(|err| {
                bump(&self.counters, "worker_enqueue_failed", 1);
                GravityError::Network(format!("market worker unavailable or overloaded: {err}"))
            })?;
            pending.push((indexes, rx));
        }

        let mut results: Vec<Option<OrderResult>> = vec![None; total];
        for (indexes, rx) in pending {
            let batch = rx.await.map_err(|_| GravityError::Network("market worker dropped batch reply".into()))??;
            for (index, result) in indexes.into_iter().zip(batch) {
                if let Some(slot) = results.get_mut(index) { *slot = Some(result); }
            }
        }
        bump(&self.counters, "parallel_batch_groups", results.len() as u64);
        results.into_iter().map(|result| result.ok_or_else(|| GravityError::Database("missing batch order result".into()))).collect()
    }

    pub async fn cancel_order(&self, symbol: Symbol, order_id: &str) -> Result<CancelResult, GravityError> {
        let Some(worker) = self.workers.get(&symbol.0).map(|entry| entry.value().clone()) else {
            return Ok(CancelResult { canceled: false, order_id: order_id.into(), message: "book not found".into() });
        };
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Cancel { order_id: order_id.into(), reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped cancel reply".into()))?
    }

    pub async fn amend_order(&self, symbol: Symbol, order_id: &str, amend: AmendRequest) -> Result<AmendResult, GravityError> {
        let Some(worker) = self.workers.get(&symbol.0).map(|entry| entry.value().clone()) else {
            return Ok(AmendResult { amended: false, order_id: order_id.into(), remaining: None, message: "book not found".into() });
        };
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Amend { order_id: order_id.into(), amend, reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped amend reply".into()))?
    }

    pub async fn replace_order(&self, symbol: Symbol, order_id: &str, req: OrderRequest) -> Result<ReplaceResult, GravityError> {
        let worker = self.worker_for(symbol).await?;
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Replace { order_id: order_id.into(), req, reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped replace reply".into()))?
    }

    pub async fn set_market_status(&self, symbol: Symbol, status: MarketStatus) -> Result<MarketStatus, GravityError> {
        let worker = self.worker_for(symbol).await?;
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Status { status, reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped status reply".into()))?
    }


    pub async fn book_snapshot(&self, symbol: Symbol, depth: usize) -> Result<BookSnapshot, GravityError> {
        let normalized_depth = normalize_depth(depth);
        if let Some(snapshot) = self.depth.get(&depth_key(&symbol, normalized_depth)) {
            let mut snap = snapshot.value().clone();
            snap.bids.truncate(depth);
            snap.asks.truncate(depth);
            return Ok(snap);
        }
        if depth <= 50 {
            if let Some(snapshot) = self.depth.get(&symbol.0) {
                let mut snap = snapshot.value().clone();
                snap.bids.truncate(depth);
                snap.asks.truncate(depth);
                return Ok(snap);
            }
        }
        let worker = self.worker_for(symbol).await?;
        let (reply, rx) = oneshot::channel();
        worker.tx.try_send(BookCommand::Snapshot { depth, reply }).map_err(|err| GravityError::Network(format!("market worker unavailable or overloaded: {err}")))?;
        rx.await.map_err(|_| GravityError::Network("market worker dropped snapshot reply".into()))?
    }

    pub async fn create_amm_pool(&self, config: PoolConfig, base: gravity_types::Quantity, quote: gravity_types::Quantity) -> Result<PoolSnapshot, GravityError> {
        let snapshot = {
            let mut amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
            amm.create_pool(config, base, quote)?
        };
        self.enqueue_persistence(PersistenceRecord::new("amm_pool", snapshot.symbol.0.clone(), snapshot.sequence, serde_json::to_value(&snapshot)?))?;
        self.bump("amm_pools", 1).await?;
        Ok(snapshot)
    }

    pub async fn amm_pools(&self) -> Result<Vec<PoolSnapshot>, GravityError> {
        let amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
        amm.snapshots()
    }

    pub async fn amm_pool(&self, symbol: &str) -> Result<Option<PoolSnapshot>, GravityError> {
        let amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
        amm.snapshot(symbol)
    }

    pub async fn amm_quote(&self, symbol: &str, side: SwapSide, amount_in: gravity_types::Quantity) -> Result<SwapQuote, GravityError> {
        let amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
        amm.quote(symbol, side, amount_in)
    }

    pub async fn amm_swap(&self, symbol: &str, side: SwapSide, amount_in: gravity_types::Quantity, min_out: Option<gravity_types::Quantity>) -> Result<SwapResult, GravityError> {
        let result = {
            let mut amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
            amm.swap(symbol, side, amount_in, min_out)?
        };
        self.enqueue_persistence(PersistenceRecord::new("amm_swap", symbol.to_owned(), result.snapshot.sequence, serde_json::to_value(&result)?))?;
        self.bump("amm_swaps", 1).await?;
        Ok(result)
    }

    pub async fn amm_add_liquidity(&self, symbol: &str, base: gravity_types::Quantity, quote: gravity_types::Quantity) -> Result<LiquidityResult, GravityError> {
        let result = {
            let mut amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
            amm.add_liquidity(symbol, base, quote)?
        };
        self.enqueue_persistence(PersistenceRecord::new("amm_liquidity", symbol.to_owned(), result.snapshot.sequence, serde_json::to_value(&result)?))?;
        self.bump("amm_liquidity_events", 1).await?;
        Ok(result)
    }

    pub async fn amm_remove_liquidity(&self, symbol: &str, lp: gravity_types::Quantity, min_base: Option<gravity_types::Quantity>, min_quote: Option<gravity_types::Quantity>) -> Result<RemoveLiquidityResult, GravityError> {
        let result = {
            let mut amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
            amm.remove_liquidity(symbol, lp, min_base, min_quote)?
        };
        self.enqueue_persistence(PersistenceRecord::new("amm_remove_liquidity", symbol.to_owned(), result.snapshot.sequence, serde_json::to_value(&result)?))?;
        self.bump("amm_liquidity_events", 1).await?;
        Ok(result)
    }

    pub async fn amm_oracle_guard(&self, symbol: &str, oracle_price: Price, max_deviation_bps: u32) -> Result<AmmGuardResult, GravityError> {
        let amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
        amm.oracle_guard(symbol, oracle_price, max_deviation_bps)
    }

    pub async fn recent_amm_events(&self, limit: usize) -> Result<Vec<PoolEvent>, GravityError> {
        let amm = self.amm.lock().map_err(|_| GravityError::Database("AMM book lock poisoned".into()))?;
        Ok(amm.recent_events(limit))
    }

    pub async fn risk_check(&self, input: AccountRiskInput) -> Result<AccountRiskSnapshot, GravityError> {
        let snapshot = {
            let mut risk = self.risk.lock().map_err(|_| GravityError::Database("risk engine lock poisoned".into()))?;
            risk.check(input)?
        };
        self.enqueue_persistence(PersistenceRecord::new("risk_snapshot", snapshot.account.clone(), snapshot.timestamp_ms, serde_json::to_value(&snapshot)?))?;
        self.bump("risk_checks", 1).await?;
        Ok(snapshot)
    }

    pub async fn risk_account(&self, account: &str) -> Result<Option<AccountRiskSnapshot>, GravityError> {
        let risk = self.risk.lock().map_err(|_| GravityError::Database("risk engine lock poisoned".into()))?;
        Ok(risk.snapshot(account))
    }

    pub async fn risk_events(&self, limit: usize) -> Result<Vec<RiskEvent>, GravityError> {
        let risk = self.risk.lock().map_err(|_| GravityError::Database("risk engine lock poisoned".into()))?;
        Ok(risk.events(limit))
    }

    pub async fn risk_stats(&self) -> Result<RiskStats, GravityError> {
        let risk = self.risk.lock().map_err(|_| GravityError::Database("risk engine lock poisoned".into()))?;
        Ok(risk.stats())
    }


pub async fn liquidation_scan(&self, limit: usize) -> Result<Vec<LiquidationCandidate>, GravityError> {
    let snapshots = {
        let risk = self.risk.lock().map_err(|_| GravityError::Database("risk engine lock poisoned".into()))?;
        risk.snapshots()
    };
    let candidates = {
        let mut liquidator = self.liquidator.lock().map_err(|_| GravityError::Database("liquidator lock poisoned".into()))?;
        liquidator.scan(snapshots, limit)?
    };
    self.enqueue_persistence(PersistenceRecord::new("liquidation_scan", "all", now_ms(), serde_json::to_value(&candidates)?))?;
    self.bump("liquidation_scans", 1).await?;
    self.bump("liquidation_candidates", candidates.len() as u64).await?;
    Ok(candidates)
}

pub async fn liquidation_plan(&self, account: &str, mode: LiquidationMode) -> Result<Option<LiquidationPlan>, GravityError> {
    let plan = {
        let mut liquidator = self.liquidator.lock().map_err(|_| GravityError::Database("liquidator lock poisoned".into()))?;
        liquidator.plan_for_account(account, mode)?
    };
    if let Some(plan) = &plan {
        self.enqueue_persistence(PersistenceRecord::new("liquidation_plan", account.to_owned(), plan.timestamp_ms, serde_json::to_value(plan)?))?;
        self.bump("liquidation_plans", 1).await?;
    }
    Ok(plan)
}

pub async fn liquidation_candidates(&self, limit: usize) -> Result<Vec<LiquidationCandidate>, GravityError> {
    let liquidator = self.liquidator.lock().map_err(|_| GravityError::Database("liquidator lock poisoned".into()))?;
    Ok(liquidator.candidates(limit))
}

pub async fn liquidation_events(&self, limit: usize) -> Result<Vec<LiquidationEvent>, GravityError> {
    let liquidator = self.liquidator.lock().map_err(|_| GravityError::Database("liquidator lock poisoned".into()))?;
    Ok(liquidator.events(limit))
}

pub async fn liquidation_stats(&self) -> Result<LiquidationStats, GravityError> {
    let liquidator = self.liquidator.lock().map_err(|_| GravityError::Database("liquidator lock poisoned".into()))?;
    Ok(liquidator.stats())
}

pub async fn create_perp_market(&self, config: PerpMarketConfig) -> Result<PerpMarketSnapshot, GravityError> {
    let snapshot = {
        let mut perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
        perps.create_market(config)?
    };
    self.enqueue_persistence(PersistenceRecord::new("perp_market", snapshot.symbol.0.clone(), snapshot.sequence, serde_json::to_value(&snapshot)?))?;
    self.bump("perp_markets", 1).await?;
    Ok(snapshot)
}

pub async fn perp_markets(&self) -> Result<Vec<PerpMarketSnapshot>, GravityError> {
    let perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
    perps.markets()
}

pub async fn perp_market(&self, symbol: &str) -> Result<Option<PerpMarketSnapshot>, GravityError> {
    let perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
    perps.market_snapshot(symbol)
}

pub async fn open_perp_position(&self, request: PerpPositionRequest) -> Result<PerpPosition, GravityError> {
    let position = {
        let mut perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
        perps.open_position(request)?
    };
    self.enqueue_persistence(PersistenceRecord::new("perp_position", position.symbol.0.clone(), position.updated_ms, serde_json::to_value(&position)?))?;
    self.bump("perp_positions", 1).await?;
    Ok(position)
}

pub async fn update_perp_funding(&self, request: FundingUpdateRequest) -> Result<PerpMarketSnapshot, GravityError> {
    let snapshot = {
        let mut perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
        perps.update_funding(request)?
    };
    self.enqueue_persistence(PersistenceRecord::new("perp_funding", snapshot.symbol.0.clone(), snapshot.sequence, serde_json::to_value(&snapshot)?))?;
    self.bump("perp_funding_updates", 1).await?;
    Ok(snapshot)
}

pub async fn perp_positions(&self, account: Option<&str>, limit: usize) -> Result<Vec<PerpPosition>, GravityError> {
    let perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
    Ok(match account { Some(account) => perps.positions_for_account(account), None => perps.all_positions(limit) })
}

pub async fn perp_events(&self, limit: usize) -> Result<Vec<PerpEvent>, GravityError> {
    let perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
    Ok(perps.events(limit))
}

pub async fn perp_stats(&self) -> Result<PerpStats, GravityError> {
    let perps = self.perps.lock().map_err(|_| GravityError::Database("perps lock poisoned".into()))?;
    Ok(perps.stats())
}


pub async fn create_index_product(&self, config: IndexProductConfig, seed_notional: Fixed) -> Result<IndexProductSnapshot, GravityError> {
    let snapshot = {
        let mut index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
        index.create_product(config, seed_notional)?
    };
    self.enqueue_persistence(PersistenceRecord::new("index_product", snapshot.id.clone(), snapshot.sequence, serde_json::to_value(&snapshot)?))?;
    self.bump("index_products", 1).await?;
    Ok(snapshot)
}

pub async fn index_products(&self) -> Result<Vec<IndexProductSnapshot>, GravityError> {
    let index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
    index.products()
}

pub async fn index_product(&self, id: &str) -> Result<Option<IndexProductSnapshot>, GravityError> {
    let index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
    index.product(id)
}

pub async fn index_nav(&self, id: &str) -> Result<IndexNavReport, GravityError> {
    let report = {
        let mut index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
        index.nav(id)?
    };
    self.enqueue_persistence(PersistenceRecord::new("index_nav", id.to_owned(), report.sequence, serde_json::to_value(&report)?))?;
    self.bump("index_nav_reports", 1).await?;
    Ok(report)
}

pub async fn index_rebalance(&self, id: &str) -> Result<RebalancePlan, GravityError> {
    let plan = {
        let mut index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
        index.rebalance_plan(id)?
    };
    self.enqueue_persistence(PersistenceRecord::new("index_rebalance", id.to_owned(), plan.sequence, serde_json::to_value(&plan)?))?;
    self.bump("index_rebalance_plans", 1).await?;
    Ok(plan)
}

pub async fn index_mint_plan(&self, id: &str, account: String, notional: Fixed) -> Result<MintRedeemPlan, GravityError> {
    let plan = {
        let mut index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
        index.mint_plan(id, account, notional)?
    };
    self.enqueue_persistence(PersistenceRecord::new("index_mint", id.to_owned(), plan.sequence, serde_json::to_value(&plan)?))?;
    self.bump("index_mint_plans", 1).await?;
    Ok(plan)
}

pub async fn index_redeem_plan(&self, id: &str, account: String, units: Quantity) -> Result<MintRedeemPlan, GravityError> {
    let plan = {
        let mut index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
        index.redeem_plan(id, account, units)?
    };
    self.enqueue_persistence(PersistenceRecord::new("index_redeem", id.to_owned(), plan.sequence, serde_json::to_value(&plan)?))?;
    self.bump("index_redeem_plans", 1).await?;
    Ok(plan)
}

pub async fn index_events(&self, limit: usize) -> Result<Vec<IndexEvent>, GravityError> {
    let index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
    Ok(index.events(limit))
}

pub async fn index_stats(&self) -> Result<IndexStats, GravityError> {
    let index = self.index.lock().map_err(|_| GravityError::Database("index lock poisoned".into()))?;
    Ok(index.stats())
}

    fn publish_stream_json(&self, topic: &str, key: &str, sequence: u64, value: Value) -> Result<(), GravityError> {
        self.stream.publish_json(topic.to_owned(), key.to_owned(), sequence, now_ms(), &value)?;
        Ok(())
    }

    pub async fn stream_stats(&self) -> Result<StreamStats, GravityError> { Ok(self.stream.stats()) }

    pub async fn recent_stream_records(&self, topic: Option<&str>, limit: usize) -> Result<Vec<StreamRecord>, GravityError> {
        Ok(self.stream.recent(topic, limit))
    }

    pub async fn tile_snapshot(&self) -> Result<TileRuntimeSnapshot, GravityError> { Ok(self.tiles.snapshot()) }

    pub async fn tile_ping(&self) -> Result<TileRuntimeSnapshot, GravityError> {
        let _ = self.tiles.ping_all(now_ms());
        Ok(self.tiles.snapshot())
    }

    pub async fn tile_restart_all(&self) -> Result<TileRuntimeSnapshot, GravityError> {
        let _ = self.tiles.restart_all();
        Ok(self.tiles.snapshot())
    }

    pub fn subscribe_oracles(&self) -> broadcast::Receiver<OracleReport> { self.oracle_tx.subscribe() }

    pub fn subscribe_book_events(&self) -> broadcast::Receiver<BookEvent> { self.book_tx.subscribe() }

    pub async fn recent_book_events(&self, symbol: Option<Symbol>, limit: usize) -> Result<Vec<BookEvent>, GravityError> {
        let limit = limit.min(10_000);
        let guard = self.book_events.lock().map_err(|_| GravityError::Database("book event ring poisoned".into()))?;
        let mut events = guard.iter()
            .rev()
            .filter(|event| symbol.as_ref().map_or(true, |s| event.symbol == *s))
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        events.reverse();
        Ok(events)
    }

    pub async fn bump(&self, key: &str, amount: u64) -> Result<(), GravityError> {
        self.counters.entry(key.to_string()).and_modify(|v| *v += amount).or_insert(amount);
        Ok(())
    }

    pub async fn counters(&self) -> Result<BTreeMap<String, u64>, GravityError> {
        Ok(self.counters.iter().map(|v| (v.key().clone(), *v.value())).collect())
    }

    pub async fn recent_audit(&self, limit: usize) -> Result<Vec<AuditRecord>, GravityError> {
        let limit = limit.min(10_000);
        let guard = self.audit.lock().map_err(|_| GravityError::Database("audit ring poisoned".into()))?;
        let mut records = guard.iter().rev().take(limit).cloned().collect::<Vec<_>>();
        records.reverse();
        Ok(records)
    }

    pub async fn persistence_stats(&self) -> Result<PersistenceStats, GravityError> {
        let queue = self.persist_queue.lock().map_err(|_| GravityError::Database("persistence queue poisoned".into()))?;
        let dropped = *self.persist_dropped.lock().map_err(|_| GravityError::Database("persistence dropped counter poisoned".into()))?;
        Ok(PersistenceStats {
            queued: queue.len(),
            capacity: self.persist_limit,
            dropped,
        })
    }

    pub async fn storage_backpressure(&self) -> Result<StorageBackpressureReport, GravityError> {
        let stats = self.persistence_stats().await?;
        let available = stats.capacity.saturating_sub(stats.queued);
        let pressure_bps = if stats.capacity == 0 { 0 } else { ((stats.queued as u128 * 10_000) / stats.capacity as u128) as u32 };
        Ok(StorageBackpressureReport { queued: stats.queued, capacity: stats.capacity, available, pressure_bps, dropped: stats.dropped })
    }

    pub async fn recent_persistence(&self, limit: usize) -> Result<Vec<PersistenceRecord>, GravityError> {
        let limit = limit.min(10_000);
        let queue = self.persist_queue.lock().map_err(|_| GravityError::Database("persistence queue poisoned".into()))?;
        let mut records = queue.iter().rev().take(limit).cloned().collect::<Vec<_>>();
        records.reverse();
        Ok(records)
    }

    pub async fn worker_stats(&self) -> Result<Vec<MarketWorkerStats>, GravityError> {
        let mut stats = Vec::new();
        for entry in self.workers.iter() {
            let handle = entry.value();
            let mut value = handle.stats.lock().map_err(|_| GravityError::Database("worker stats lock poisoned".into()))?.clone();
            value.queue_available = handle.tx.capacity();
            value.queue_depth = value.queue_capacity.saturating_sub(value.queue_available);
            stats.push(value);
        }
        stats.sort_by(|a, b| a.symbol.0.cmp(&b.symbol.0));
        Ok(stats)
    }

    async fn worker_for(&self, symbol: Symbol) -> Result<MarketHandle, GravityError> {
        if let Some(handle) = self.workers.get(&symbol.0) {
            return Ok(handle.value().clone());
        }
        let (tx, rx) = mpsc::channel(self.worker_queue);
        let stats = Arc::new(Mutex::new(MarketWorkerStats::new(symbol.clone(), self.worker_queue)));
        let handle = MarketHandle { tx, stats: stats.clone() };
        self.workers.insert(symbol.0.clone(), handle.clone());
        let ctx = WorkerContext {
            symbol,
            depth: self.depth.clone(),
            book_events: self.book_events.clone(),
            audit: self.audit.clone(),
            persist_queue: self.persist_queue.clone(),
            amm: self.amm.clone(),
            risk: self.risk.clone(),
            liquidator: self.liquidator.clone(),
            perps: self.perps.clone(),
            index: self.index.clone(),
            wal: self.wal.clone(),
            persist_dropped: self.persist_dropped.clone(),
            stream: self.stream.clone(),
            tiles: self.tiles.clone(),
            book_tx: self.book_tx.clone(),
            counters: self.counters.clone(),
            event_limit: self.event_limit,
            persist_limit: self.persist_limit,
            stats,
        };
        tokio::spawn(async move { run_market_worker(ctx, rx).await; });
        Ok(handle)
    }

    fn push_audit(&self, record: AuditRecord) -> Result<(), GravityError> {
        let mut guard = self.audit.lock().map_err(|_| GravityError::Database("audit ring poisoned".into()))?;
        if guard.len() >= self.event_limit { guard.pop_front(); }
        guard.push_back(record);
        Ok(())
    }

    fn enqueue_persistence(&self, record: PersistenceRecord) -> Result<(), GravityError> {
        let _ = self.wal.append(record.kind.clone(), record.target.clone(), record.sequence, record.body.clone())?;
        enqueue_persistence_record(&self.persist_queue, &self.persist_dropped, self.persist_limit, record)
    }

    pub async fn wal_stats(&self) -> Result<WalStats, GravityError> { self.wal.stats() }

    pub async fn recent_wal(&self, limit: usize) -> Result<Vec<WalRecord>, GravityError> { self.wal.recent(limit) }

    pub async fn wal_checkpoint(&self, note: impl Into<String>) -> Result<CheckpointRecord, GravityError> { self.wal.checkpoint(note) }

    pub async fn wal_replay_plan(&self) -> Result<ReplayPlan, GravityError> { self.wal.replay_plan() }

    pub async fn wal_checkpoints(&self, limit: usize) -> Result<Vec<CheckpointRecord>, GravityError> { self.wal.checkpoints(limit) }

    pub async fn wal_recovery_report(&self) -> Result<RecoveryReport, GravityError> { self.wal.recovery_report() }

    pub async fn wal_replay_dry_run(&self) -> Result<RecoveryReport, GravityError> { self.wal.replay_dry_run() }
}

#[allow(dead_code)]
struct WorkerContext {
    symbol: Symbol,
    depth: Arc<DashMap<String, BookSnapshot>>,
    book_events: Arc<Mutex<VecDeque<BookEvent>>>,
    audit: Arc<Mutex<VecDeque<AuditRecord>>>,
    persist_queue: Arc<Mutex<VecDeque<PersistenceRecord>>>,
    amm: Arc<Mutex<AmmBook>>,
    risk: Arc<Mutex<RiskEngine>>,
    liquidator: Arc<Mutex<LiquidationEngine>>,
    perps: Arc<Mutex<PerpEngine>>,
    index: Arc<Mutex<IndexEngine>>,
    wal: WalManager,
    stream: StreamHub,
    tiles: TileSupervisor,
    persist_dropped: Arc<Mutex<u64>>,
    book_tx: broadcast::Sender<BookEvent>,
    counters: Arc<DashMap<String, u64>>,
    event_limit: usize,
    persist_limit: usize,
    stats: Arc<Mutex<MarketWorkerStats>>,
}

async fn run_market_worker(ctx: WorkerContext, mut rx: mpsc::Receiver<BookCommand>) {
    let mut book = OrderBook::new(ctx.symbol.clone());
    while let Some(first) = rx.recv().await {
        let mut batch = Vec::with_capacity(1024);
        batch.push(first);
        while batch.len() < 1024 {
            match rx.try_recv() {
                Ok(cmd) => batch.push(cmd),
                Err(_) => break,
            }
        }
        let batch_len = batch.len();
        bump(&ctx.counters, "microbatches", 1);
        bump(&ctx.counters, "microbatch_commands", batch_len as u64);
        for cmd in batch {
            process_book_command(&ctx, &mut book, cmd);
        }
        if let Ok(mut stats) = ctx.stats.lock() {
            stats.max_batch = stats.max_batch.max(batch_len);
            stats.queue_available = rx.capacity();
            stats.queue_depth = stats.queue_capacity.saturating_sub(stats.queue_available);
            stats.updated_ms = now_ms();
        }
    }
    tracing::warn!(symbol=%ctx.symbol, "market worker stopped");
}

fn process_book_command(ctx: &WorkerContext, book: &mut OrderBook, cmd: BookCommand) {
    let start = Instant::now();
    match cmd {
        BookCommand::Submit { req, reply } => {
            let response = execute_submit(ctx, book, req);
            if response.is_ok() { refresh_depth_cache(ctx, book); }
            let _ = reply.send(response);
        }
        BookCommand::SubmitBatch { reqs, reply } => {
            let mut out = Vec::with_capacity(reqs.len());
            let mut failed = None;
            for req in reqs {
                match execute_submit(ctx, book, req) {
                    Ok(result) => out.push(result),
                    Err(err) => { failed = Some(err); break; }
                }
            }
            if failed.is_none() { refresh_depth_cache(ctx, book); }
            let _ = reply.send(match failed { Some(err) => Err(err), None => Ok(out) });
        }
        BookCommand::Cancel { order_id, reply } => {
            let result = book.cancel(&order_id);
            let sequence = book.stats().sequence;
            refresh_depth_cache(ctx, book);
            if result.canceled {
                bump(&ctx.counters, "orders_canceled", 1);
                push_audit(ctx, AuditRecord::new("cancel", ctx.symbol.0.clone(), sequence, result.message.clone()));
                if let Ok(body) = serde_json::to_value(&result) {
                    enqueue_persistence(ctx, PersistenceRecord::new("cancel", ctx.symbol.0.clone(), sequence, body));
                }
                push_book_event(ctx, BookEvent {
                    kind: BookEventKind::OrderCanceled,
                    symbol: ctx.symbol.clone(),
                    order_id: result.order_id.clone(),
                    fill_id: None,
                    price: None,
                    quantity: None,
                    sequence,
                    timestamp_ms: now_ms(),
                    message: result.message.clone(),
                });
                if let Ok(mut stats) = ctx.stats.lock() { stats.canceled += 1; stats.last_sequence = sequence; }
            }
            let _ = reply.send(Ok(result));
        }
        BookCommand::Amend { order_id, amend, reply } => {
            let result = book.amend(&order_id, amend);
            let sequence = book.stats().sequence;
            if result.as_ref().map(|v| v.amended).unwrap_or(false) {
                refresh_depth_cache(ctx, book);
                bump(&ctx.counters, "orders_amended", 1);
                push_audit(ctx, AuditRecord::new("amend", ctx.symbol.0.clone(), sequence, "amended"));
                if let Ok(value) = &result {
                    if let Ok(body) = serde_json::to_value(value) { enqueue_persistence(ctx, PersistenceRecord::new("amend", ctx.symbol.0.clone(), sequence, body)); }
                    push_book_event(ctx, BookEvent {
                        kind: BookEventKind::OrderAmended,
                        symbol: ctx.symbol.clone(),
                        order_id: value.order_id.clone(),
                        fill_id: None,
                        price: None,
                        quantity: value.remaining,
                        sequence,
                        timestamp_ms: now_ms(),
                        message: value.message.clone(),
                    });
                }
            }
            let _ = reply.send(result);
        }
        BookCommand::Replace { order_id, req, reply } => {
            let response = book.replace(&order_id, req);
            let sequence = book.stats().sequence;
            if response.is_ok() { refresh_depth_cache(ctx, book); }
            if let Ok(value) = &response {
                bump(&ctx.counters, "orders_replaced", u64::from(value.replacement.is_some()));
                push_audit(ctx, AuditRecord::new("replace", ctx.symbol.0.clone(), sequence, "cancel-replace"));
                if let Ok(body) = serde_json::to_value(value) { enqueue_persistence(ctx, PersistenceRecord::new("replace", ctx.symbol.0.clone(), sequence, body)); }
                if value.replacement.is_some() {
                    push_book_event(ctx, BookEvent {
                        kind: BookEventKind::OrderReplaced,
                        symbol: ctx.symbol.clone(),
                        order_id: order_id.clone(),
                        fill_id: None,
                        price: None,
                        quantity: None,
                        sequence,
                        timestamp_ms: now_ms(),
                        message: "replaced".into(),
                    });
                }
            }
            let _ = reply.send(response);
        }
        BookCommand::Status { status, reply } => {
            book.set_status(status);
            bump(&ctx.counters, "market_status_changes", 1);
            let _ = reply.send(Ok(book.status()));
        }
        BookCommand::Snapshot { depth, reply } => {
            let snapshot = book.snapshot(depth);
            ctx.depth.insert(depth_key(&ctx.symbol, normalize_depth(depth)), snapshot.clone());
            if depth <= 50 { ctx.depth.insert(ctx.symbol.0.clone(), snapshot.clone()); }
            if let Ok(mut stats) = ctx.stats.lock() { stats.snapshots += 1; stats.last_sequence = snapshot.sequence; }
            let _ = reply.send(Ok(snapshot));
        }
    }
    if let Ok(mut stats) = ctx.stats.lock() {
        let latency = start.elapsed().as_micros() as u64;
        stats.commands += 1;
        stats.last_latency_us = latency;
        stats.max_latency_us = stats.max_latency_us.max(latency);
        stats.total_latency_us = stats.total_latency_us.saturating_add(latency);
        stats.avg_latency_us = if stats.commands == 0 { 0 } else { stats.total_latency_us / stats.commands };
        stats.updated_ms = now_ms();
    }
}

fn execute_submit(ctx: &WorkerContext, book: &mut OrderBook, req: OrderRequest) -> Result<OrderResult, GravityError> {
    let result = book.submit(req)?;
    let sequence = book.stats().sequence;
    record_order_events(ctx, &ctx.symbol, &result, sequence);
    push_audit(ctx, AuditRecord::new("order", ctx.symbol.0.clone(), sequence, result.status.clone()));
    if let Ok(body) = serde_json::to_value(&result) {
        enqueue_persistence(ctx, PersistenceRecord::new("order_result", ctx.symbol.0.clone(), sequence, body));
    }
    bump(&ctx.counters, "orders_submitted", 1);
    bump(&ctx.counters, "fills", result.fills.len() as u64);
    if let Ok(mut stats) = ctx.stats.lock() {
        stats.submitted += 1;
        stats.fills += result.fills.len() as u64;
        stats.rejected += u64::from(!result.accepted);
        stats.last_sequence = sequence;
    }
    Ok(result)
}

fn refresh_depth_cache(ctx: &WorkerContext, book: &OrderBook) {
    for depth in [10_usize, 25, 50, 100] {
        ctx.depth.insert(depth_key(&ctx.symbol, depth), book.snapshot(depth));
    }
    ctx.depth.insert(ctx.symbol.0.clone(), book.snapshot(50));
}

fn normalize_depth(depth: usize) -> usize {
    match depth {
        0..=10 => 10,
        11..=25 => 25,
        26..=50 => 50,
        _ => 100,
    }
}

fn depth_key(symbol: &Symbol, depth: usize) -> String { format!("{}:{depth}", symbol.0) }

fn record_order_events(ctx: &WorkerContext, symbol: &Symbol, result: &OrderResult, sequence: u64) {
    let kind = if result.accepted { BookEventKind::OrderAccepted } else { BookEventKind::OrderRejected };
    push_book_event(ctx, BookEvent {
        kind,
        symbol: symbol.clone(),
        order_id: result.order_id.clone(),
        fill_id: None,
        price: None,
        quantity: Some(result.remaining),
        sequence,
        timestamp_ms: now_ms(),
        message: result.status.clone(),
    });
    for fill in &result.fills {
        push_book_event(ctx, BookEvent {
            kind: BookEventKind::Fill,
            symbol: fill.symbol.clone(),
            order_id: fill.taker_order.clone(),
            fill_id: Some(fill.id.clone()),
            price: Some(fill.price),
            quantity: Some(fill.quantity),
            sequence,
            timestamp_ms: fill.timestamp_ms,
            message: format!("fill maker={} taker={}", fill.maker_order, fill.taker_order),
        });
    }
}

fn push_book_event(ctx: &WorkerContext, event: BookEvent) {
    if let Ok(mut guard) = ctx.book_events.lock() {
        if guard.len() >= ctx.event_limit { guard.pop_front(); }
        guard.push_back(event.clone());
    }
    let _ = ctx.stream.publish_json("book", event.symbol.0.clone(), event.sequence, event.timestamp_ms, &serde_json::json!({ "type": "book_event", "event": event.clone() }));
    let _ = ctx.book_tx.send(event);
}

fn push_audit(ctx: &WorkerContext, record: AuditRecord) {
    if let Ok(mut guard) = ctx.audit.lock() {
        if guard.len() >= ctx.event_limit { guard.pop_front(); }
        guard.push_back(record);
    }
}

fn enqueue_persistence(ctx: &WorkerContext, record: PersistenceRecord) {
    let _ = enqueue_persistence_record(&ctx.persist_queue, &ctx.persist_dropped, ctx.persist_limit, record);
}

fn enqueue_persistence_record(
    queue: &Arc<Mutex<VecDeque<PersistenceRecord>>>,
    dropped: &Arc<Mutex<u64>>,
    limit: usize,
    record: PersistenceRecord,
) -> Result<(), GravityError> {
    let mut guard = queue.lock().map_err(|_| GravityError::Database("persistence queue poisoned".into()))?;
    if guard.len() >= limit {
        guard.pop_front();
        if let Ok(mut d) = dropped.lock() { *d = d.saturating_add(1); }
    }
    guard.push_back(record);
    Ok(())
}


fn bump(counters: &DashMap<String, u64>, key: &str, amount: u64) {
    counters.entry(key.to_string()).and_modify(|v| *v += amount).or_insert(amount);
}

#[derive(Clone)]
pub struct PostgresStore { pool: PgPool }

impl PostgresStore {
    pub fn connect_lazy(url: &str) -> Result<Self, GravityError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .connect_lazy(url)
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(Self { pool })
    }

    pub async fn health(&self) -> Result<(), GravityError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn put_oracle(&self, report: &OracleReport) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(report)?;
        sqlx::query(
            "INSERT INTO oracle_reports(symbol, price_raw, confidence_bps, sources, method, timestamp_ms, key_id, payload_hash, signature, body) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) \
             ON CONFLICT(symbol) DO UPDATE SET price_raw=$2, confidence_bps=$3, sources=$4, method=$5, timestamp_ms=$6, key_id=$7, payload_hash=$8, signature=$9, body=$10"
        )
        .bind(&report.symbol.0)
        .bind(report.price.0.as_raw().to_string())
        .bind(i64::from(report.confidence_bps))
        .bind(i64::from(report.sources))
        .bind(&report.method)
        .bind(report.timestamp_ms as i64)
        .bind(report.key_id.as_deref())
        .bind(&report.payload_hash)
        .bind(report.signature.as_deref())
        .bind(body)
        .execute(&self.pool)
        .await
        .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn put_market_event(&self, event: &MarketEvent) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(event)?;
        sqlx::query("INSERT INTO market_events(symbol, venue, kind, sequence, timestamp_ms, body) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
            .bind(&event.symbol().0)
            .bind(event.venue())
            .bind(event.kind())
            .bind(event.sequence() as i64)
            .bind(event.timestamp_ms() as i64)
            .bind(body)
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn put_order_result(&self, symbol: &Symbol, result: &OrderResult) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(result)?;
        sqlx::query("INSERT INTO orders(id, account, symbol, side, kind, tif, price_raw, quantity_raw, remaining_raw, status, client_id, created_ms, updated_ms, body) VALUES ($1,'unknown',$2,'unknown','unknown','unknown',NULL,'0',$4,$3,NULL,0,0,$5) ON CONFLICT(id) DO UPDATE SET status=$3, remaining_raw=$4, body=$5")
            .bind(&result.order_id)
            .bind(&symbol.0)
            .bind(&result.status)
            .bind(result.remaining.0.as_raw().to_string())
            .bind(body)
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        for fill in &result.fills {
            let fill_body: Value = serde_json::to_value(fill)?;
            sqlx::query("INSERT INTO fills(id, symbol, maker_order, taker_order, maker_account, taker_account, price_raw, quantity_raw, taker_side, timestamp_ms, body) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING")
                .bind(&fill.id)
                .bind(&fill.symbol.0)
                .bind(&fill.maker_order)
                .bind(&fill.taker_order)
                .bind(&fill.maker_account)
                .bind(&fill.taker_account)
                .bind(fill.price.0.as_raw().to_string())
                .bind(fill.quantity.0.as_raw().to_string())
                .bind(fill.taker_side.as_str())
                .bind(fill.timestamp_ms as i64)
                .bind(fill_body)
                .execute(&self.pool)
                .await
                .map_err(|err| GravityError::Database(err.to_string()))?;
        }
        Ok(())
    }

    pub async fn put_book_event(&self, event: &BookEvent) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(event)?;
        sqlx::query("INSERT INTO book_events(kind, symbol, order_id, fill_id, price_raw, quantity_raw, sequence, timestamp_ms, message, body) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(format!("{:?}", event.kind))
            .bind(&event.symbol.0)
            .bind(&event.order_id)
            .bind(event.fill_id.as_deref())
            .bind(event.price.map(|v| v.0.as_raw().to_string()))
            .bind(event.quantity.map(|v| v.0.as_raw().to_string()))
            .bind(event.sequence as i64)
            .bind(event.timestamp_ms as i64)
            .bind(&event.message)
            .bind(body)
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn put_audit(&self, record: &AuditRecord) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(record)?;
        sqlx::query("INSERT INTO audit_records(id, kind, target, sequence, timestamp_ms, payload_hash, message, body) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING")
            .bind(&record.id)
            .bind(&record.kind)
            .bind(&record.target)
            .bind(record.sequence as i64)
            .bind(record.timestamp_ms as i64)
            .bind(&record.payload_hash)
            .bind(&record.message)
            .bind(body)
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn put_persistence_record(&self, record: &PersistenceRecord) -> Result<(), GravityError> {
        let body: Value = serde_json::to_value(record)?;
        sqlx::query("INSERT INTO persistence_records(kind, target, sequence, timestamp_ms, body) VALUES ($1,$2,$3,$4,$5)")
            .bind(&record.kind)
            .bind(&record.target)
            .bind(record.sequence as i64)
            .bind(record.timestamp_ms as i64)
            .bind(body)
            .execute(&self.pool)
            .await
            .map_err(|err| GravityError::Database(err.to_string()))?;
        Ok(())
    }

    pub async fn migration_status(&self) -> Result<MigrationStatus, GravityError> {
        let rows = sqlx::query_as::<_, (String,)>("SELECT file FROM schema_migrations ORDER BY file")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        let mut applied = rows.into_iter().map(|v| v.0).collect::<Vec<_>>();
        applied.sort();
        let expected = REQUIRED_MIGRATIONS.iter().map(|v| (*v).to_string()).collect::<Vec<_>>();
        let missing = expected.iter().filter(|file| !applied.contains(file)).cloned().collect::<Vec<_>>();
        Ok(MigrationStatus { expected, applied, missing })
    }
}

#[derive(Clone)]
pub struct RedisCache { client: redis::Client }

impl RedisCache {
    pub fn new(url: &str) -> Result<Self, GravityError> {
        let client = redis::Client::open(url).map_err(|err| GravityError::Cache(err.to_string()))?;
        Ok(Self { client })
    }

    pub async fn health(&self) -> Result<(), GravityError> {
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(|err| GravityError::Cache(err.to_string()))?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await.map_err(|err| GravityError::Cache(err.to_string()))?;
        Ok(())
    }

    pub async fn put_oracle(&self, report: &OracleReport) -> Result<(), GravityError> {
        let key = format!("gravity:oracle:{}", report.symbol);
        let body = serde_json::to_string(report)?;
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(|err| GravityError::Cache(err.to_string()))?;
        let _: () = redis::cmd("SET").arg(key).arg(body).arg("PX").arg(10_000_u64).query_async(&mut conn).await.map_err(|err| GravityError::Cache(err.to_string()))?;
        Ok(())
    }

    pub async fn put_book_snapshot(&self, snapshot: &BookSnapshot) -> Result<(), GravityError> {
        let key = format!("gravity:book:{}:depth", snapshot.symbol);
        let body = serde_json::to_string(snapshot)?;
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(|err| GravityError::Cache(err.to_string()))?;
        let _: () = redis::cmd("SET").arg(key).arg(body).arg("PX").arg(2_000_u64).query_async(&mut conn).await.map_err(|err| GravityError::Cache(err.to_string()))?;
        Ok(())
    }

    pub async fn publish_book_event(&self, event: &BookEvent) -> Result<(), GravityError> {
        let key = format!("gravity:stream:book:{}", event.symbol);
        let body = serde_json::to_string(event)?;
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(|err| GravityError::Cache(err.to_string()))?;
        let _: () = redis::cmd("XADD").arg(key).arg("MAXLEN").arg("~").arg(100_000_u64).arg("*").arg("body").arg(body).query_async(&mut conn).await.map_err(|err| GravityError::Cache(err.to_string()))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct GravityStore {
    plan: StoragePlan,
    memory: MemoryStore,
    postgres: Option<PostgresStore>,
    redis: Option<RedisCache>,
    settlement: SettlementClient,
    hardware: HardwareProfile,
}

impl GravityStore {
    pub fn new(plan: StoragePlan) -> Result<Self, GravityError> {
        let postgres = match plan.mode {
            StoreMode::Postgres | StoreMode::PostgresRedis => Some(PostgresStore::connect_lazy(&plan.postgres_url)?),
            StoreMode::Memory => None,
        };
        let redis = match plan.mode {
            StoreMode::PostgresRedis => Some(RedisCache::new(&plan.redis_url)?),
            StoreMode::Memory | StoreMode::Postgres => None,
        };
        Ok(Self { plan, memory: MemoryStore::default(), postgres, redis, settlement: SettlementClient::new("local://stargate-settlement"), hardware: HardwareProfile::detect() })
    }

    pub fn plan(&self) -> &StoragePlan { &self.plan }
    pub fn memory(&self) -> MemoryStore { self.memory.clone() }

    pub async fn database_report(&self) -> Result<DatabaseHealthReport, GravityError> {
        let persistence = self.memory.persistence_stats().await?;
        let postgres_ok = match &self.postgres {
            Some(pg) => pg.health().await.is_ok(),
            None => matches!(self.plan.mode, StoreMode::Memory),
        };
        let redis_ok = match &self.redis {
            Some(cache) => cache.health().await.is_ok(),
            None => !matches!(self.plan.mode, StoreMode::PostgresRedis),
        };
        let migrations = match &self.postgres {
            Some(pg) => pg.migration_status().await.unwrap_or(MigrationStatus { expected: REQUIRED_MIGRATIONS.iter().map(|v| (*v).to_string()).collect(), applied: Vec::new(), missing: REQUIRED_MIGRATIONS.iter().map(|v| (*v).to_string()).collect() }),
            None => MigrationStatus { expected: REQUIRED_MIGRATIONS.iter().map(|v| (*v).to_string()).collect(), applied: Vec::new(), missing: Vec::new() },
        };
        Ok(DatabaseHealthReport {
            mode: format!("{:?}", self.plan.mode),
            postgres_enabled: self.postgres.is_some(),
            postgres_ok,
            redis_enabled: self.redis.is_some(),
            redis_ok,
            migration_count: migrations.applied.len(),
            missing_migrations: migrations.missing,
            persistence,
            checked_ms: now_ms(),
        })
    }

    pub async fn migration_status(&self) -> Result<MigrationStatus, GravityError> {
        if let Some(pg) = &self.postgres { return pg.migration_status().await; }
        Ok(MigrationStatus { expected: REQUIRED_MIGRATIONS.iter().map(|v| (*v).to_string()).collect(), applied: Vec::new(), missing: Vec::new() })
    }

    pub async fn storage_backpressure(&self) -> Result<StorageBackpressureReport, GravityError> { self.memory.storage_backpressure().await }


    pub async fn hardware_profile(&self) -> Result<HardwareProfile, GravityError> { Ok(self.hardware.clone()) }

    pub async fn hardware_plan(&self, profile: Option<&str>) -> Result<HardwarePlan, GravityError> {
        let selected = profile.map(RuntimeProfile::parse).unwrap_or(RuntimeProfile::Balanced);
        Ok(build_plan(selected, &self.hardware))
    }

    pub async fn hardware_simulate(&self, profile: Option<&str>) -> Result<PlacementSimulation, GravityError> {
        let selected = profile.map(RuntimeProfile::parse).unwrap_or(RuntimeProfile::Balanced);
        Ok(simulate(selected))
    }

    pub async fn put_oracle(&self, report: OracleReport) -> Result<(), GravityError> {
        self.memory.put_oracle(report.clone()).await?;
        if let Some(pg) = &self.postgres { pg.put_oracle(&report).await?; }
        if let Some(cache) = &self.redis { cache.put_oracle(&report).await?; }
        Ok(())
    }

    pub async fn get_oracle(&self, symbol: &str) -> Result<Option<OracleReport>, GravityError> { self.memory.get_oracle(symbol).await }
    pub async fn all_oracles(&self) -> Result<Vec<OracleReport>, GravityError> { self.memory.all_oracles().await }

    pub async fn put_oracle_sources(&self, sources: Vec<OracleSourceView>) -> Result<(), GravityError> {
        self.memory.put_oracle_sources(sources).await
    }

    pub async fn oracle_sources(&self) -> Result<Vec<OracleSourceView>, GravityError> {
        self.memory.oracle_sources().await
    }

    pub async fn oracle_stats(&self) -> Result<OracleStats, GravityError> {
        self.memory.oracle_stats().await
    }

    pub fn subscribe_oracles(&self) -> broadcast::Receiver<OracleReport> { self.memory.subscribe_oracles() }
    pub fn subscribe_book_events(&self) -> broadcast::Receiver<BookEvent> { self.memory.subscribe_book_events() }

    pub async fn submit_order(&self, req: OrderRequest) -> Result<OrderResult, GravityError> {
        let symbol = req.symbol.clone();
        let result = self.memory.submit_order(req).await?;
        self.persist_order_side_effects(&symbol, &result).await?;
        Ok(result)
    }

    pub async fn submit_orders(&self, orders: Vec<OrderRequest>) -> Result<Vec<OrderResult>, GravityError> {
        let symbols = orders.iter().map(|req| req.symbol.clone()).collect::<Vec<_>>();
        let results = self.memory.submit_orders(orders).await?;
        for (symbol, result) in symbols.iter().zip(results.iter()) {
            self.persist_order_side_effects(symbol, result).await?;
        }
        Ok(results)
    }

    async fn persist_order_side_effects(&self, symbol: &Symbol, result: &OrderResult) -> Result<(), GravityError> {
        if let Some(pg) = &self.postgres {
            pg.put_order_result(symbol, result).await?;
            for event in self.memory.recent_book_events(Some(symbol.clone()), result.fills.len().saturating_add(1)).await? {
                let _ = pg.put_book_event(&event).await;
            }
        }
        if let Some(cache) = &self.redis {
            if let Ok(snapshot) = self.memory.book_snapshot(symbol.clone(), 50).await { let _ = cache.put_book_snapshot(&snapshot).await; }
            for event in self.memory.recent_book_events(Some(symbol.clone()), result.fills.len().saturating_add(1)).await? {
                let _ = cache.publish_book_event(&event).await;
            }
        }
        if !result.fills.is_empty() {
            let _ = self.settlement.submit_order_result(result).await?;
            let _ = self.memory.bump("settlement_batches", 1).await;
            let _ = self.memory.bump("settlement_fills", result.fills.len() as u64).await;
        }
        Ok(())
    }

    pub async fn cancel_order(&self, symbol: Symbol, order_id: &str) -> Result<CancelResult, GravityError> {
        let result = self.memory.cancel_order(symbol.clone(), order_id).await?;
        if result.canceled {
            if let Some(event) = self.memory.recent_book_events(Some(symbol.clone()), 1).await?.into_iter().next() {
                if let Some(pg) = &self.postgres { let _ = pg.put_book_event(&event).await; }
                if let Some(cache) = &self.redis { let _ = cache.publish_book_event(&event).await; }
            }
        }
        Ok(result)
    }

    pub async fn amend_order(&self, symbol: Symbol, order_id: &str, amend: AmendRequest) -> Result<AmendResult, GravityError> {
        let result = self.memory.amend_order(symbol.clone(), order_id, amend).await?;
        if result.amended {
            if let Some(event) = self.memory.recent_book_events(Some(symbol.clone()), 1).await?.into_iter().next() {
                if let Some(pg) = &self.postgres { let _ = pg.put_book_event(&event).await; }
                if let Some(cache) = &self.redis { let _ = cache.publish_book_event(&event).await; }
            }
        }
        Ok(result)
    }

    pub async fn replace_order(&self, symbol: Symbol, order_id: &str, req: OrderRequest) -> Result<ReplaceResult, GravityError> {
        let result = self.memory.replace_order(symbol.clone(), order_id, req).await?;
        if let Some(replacement) = &result.replacement {
            self.persist_order_side_effects(&symbol, replacement).await?;
        }
        Ok(result)
    }

    pub async fn set_market_status(&self, symbol: Symbol, status: MarketStatus) -> Result<MarketStatus, GravityError> {
        self.memory.set_market_status(symbol, status).await
    }

    pub async fn book_snapshot(&self, symbol: Symbol, depth: usize) -> Result<BookSnapshot, GravityError> { self.memory.book_snapshot(symbol, depth).await }
    pub async fn recent_book_events(&self, symbol: Option<Symbol>, limit: usize) -> Result<Vec<BookEvent>, GravityError> { self.memory.recent_book_events(symbol, limit).await }
    pub async fn persistence_stats(&self) -> Result<PersistenceStats, GravityError> { self.memory.persistence_stats().await }

    pub async fn recent_persistence(&self, limit: usize) -> Result<Vec<PersistenceRecord>, GravityError> { self.memory.recent_persistence(limit).await }

    pub async fn wal_stats(&self) -> Result<WalStats, GravityError> { self.memory.wal_stats().await }
    pub async fn recent_wal(&self, limit: usize) -> Result<Vec<WalRecord>, GravityError> { self.memory.recent_wal(limit).await }
    pub async fn wal_checkpoint(&self, note: impl Into<String>) -> Result<CheckpointRecord, GravityError> { self.memory.wal_checkpoint(note).await }
    pub async fn wal_replay_plan(&self) -> Result<ReplayPlan, GravityError> { self.memory.wal_replay_plan().await }
    pub async fn wal_checkpoints(&self, limit: usize) -> Result<Vec<CheckpointRecord>, GravityError> { self.memory.wal_checkpoints(limit).await }
    pub async fn wal_recovery_report(&self) -> Result<RecoveryReport, GravityError> { self.memory.wal_recovery_report().await }
    pub async fn wal_replay_dry_run(&self) -> Result<RecoveryReport, GravityError> { self.memory.wal_replay_dry_run().await }

    pub async fn worker_stats(&self) -> Result<Vec<MarketWorkerStats>, GravityError> { self.memory.worker_stats().await }

    pub async fn settlement_stats(&self) -> Result<SettlementStats, GravityError> { Ok(self.settlement.stats().await) }

    pub async fn recent_settlements(&self, limit: usize) -> Result<Vec<SettlementFinalizationRecord>, GravityError> { Ok(self.settlement.recent(limit).await) }

    pub async fn dead_letter_settlements(&self, limit: usize) -> Result<Vec<SettlementFinalizationRecord>, GravityError> { Ok(self.settlement.dead_letters(limit).await) }

    pub async fn retry_dead_letter_settlements(&self, limit: usize) -> Result<gravity_settlement::SettlementBatchReceipt, GravityError> {
        self.settlement.retry_dead_letters(limit).await
    }

    pub async fn create_amm_pool(&self, config: PoolConfig, base: gravity_types::Quantity, quote: gravity_types::Quantity) -> Result<PoolSnapshot, GravityError> { self.memory.create_amm_pool(config, base, quote).await }
    pub async fn amm_pools(&self) -> Result<Vec<PoolSnapshot>, GravityError> { self.memory.amm_pools().await }
    pub async fn amm_pool(&self, symbol: &str) -> Result<Option<PoolSnapshot>, GravityError> { self.memory.amm_pool(symbol).await }
    pub async fn amm_quote(&self, symbol: &str, side: SwapSide, amount_in: gravity_types::Quantity) -> Result<SwapQuote, GravityError> { self.memory.amm_quote(symbol, side, amount_in).await }
    pub async fn amm_swap(&self, symbol: &str, side: SwapSide, amount_in: gravity_types::Quantity, min_out: Option<gravity_types::Quantity>) -> Result<SwapResult, GravityError> { self.memory.amm_swap(symbol, side, amount_in, min_out).await }
    pub async fn amm_add_liquidity(&self, symbol: &str, base: gravity_types::Quantity, quote: gravity_types::Quantity) -> Result<LiquidityResult, GravityError> { self.memory.amm_add_liquidity(symbol, base, quote).await }
    pub async fn amm_remove_liquidity(&self, symbol: &str, lp: gravity_types::Quantity, min_base: Option<gravity_types::Quantity>, min_quote: Option<gravity_types::Quantity>) -> Result<RemoveLiquidityResult, GravityError> { self.memory.amm_remove_liquidity(symbol, lp, min_base, min_quote).await }
    pub async fn amm_oracle_guard(&self, symbol: &str, oracle_price: Price, max_deviation_bps: u32) -> Result<AmmGuardResult, GravityError> { self.memory.amm_oracle_guard(symbol, oracle_price, max_deviation_bps).await }
    pub async fn recent_amm_events(&self, limit: usize) -> Result<Vec<PoolEvent>, GravityError> { self.memory.recent_amm_events(limit).await }

    pub async fn risk_check(&self, input: AccountRiskInput) -> Result<AccountRiskSnapshot, GravityError> { self.memory.risk_check(input).await }
    pub async fn risk_account(&self, account: &str) -> Result<Option<AccountRiskSnapshot>, GravityError> { self.memory.risk_account(account).await }
    pub async fn risk_events(&self, limit: usize) -> Result<Vec<RiskEvent>, GravityError> { self.memory.risk_events(limit).await }
    pub async fn risk_stats(&self) -> Result<RiskStats, GravityError> { self.memory.risk_stats().await }

pub async fn liquidation_scan(&self, limit: usize) -> Result<Vec<LiquidationCandidate>, GravityError> { self.memory.liquidation_scan(limit).await }
pub async fn liquidation_plan(&self, account: &str, mode: LiquidationMode) -> Result<Option<LiquidationPlan>, GravityError> { self.memory.liquidation_plan(account, mode).await }
pub async fn liquidation_candidates(&self, limit: usize) -> Result<Vec<LiquidationCandidate>, GravityError> { self.memory.liquidation_candidates(limit).await }
pub async fn liquidation_events(&self, limit: usize) -> Result<Vec<LiquidationEvent>, GravityError> { self.memory.liquidation_events(limit).await }
pub async fn liquidation_stats(&self) -> Result<LiquidationStats, GravityError> { self.memory.liquidation_stats().await }

pub async fn create_perp_market(&self, config: PerpMarketConfig) -> Result<PerpMarketSnapshot, GravityError> { self.memory.create_perp_market(config).await }
pub async fn perp_markets(&self) -> Result<Vec<PerpMarketSnapshot>, GravityError> { self.memory.perp_markets().await }
pub async fn perp_market(&self, symbol: &str) -> Result<Option<PerpMarketSnapshot>, GravityError> { self.memory.perp_market(symbol).await }
pub async fn open_perp_position(&self, request: PerpPositionRequest) -> Result<PerpPosition, GravityError> { self.memory.open_perp_position(request).await }
pub async fn update_perp_funding(&self, request: FundingUpdateRequest) -> Result<PerpMarketSnapshot, GravityError> { self.memory.update_perp_funding(request).await }
pub async fn perp_positions(&self, account: Option<&str>, limit: usize) -> Result<Vec<PerpPosition>, GravityError> { self.memory.perp_positions(account, limit).await }
pub async fn perp_events(&self, limit: usize) -> Result<Vec<PerpEvent>, GravityError> { self.memory.perp_events(limit).await }
pub async fn perp_stats(&self) -> Result<PerpStats, GravityError> { self.memory.perp_stats().await }

pub async fn create_index_product(&self, config: IndexProductConfig, seed_notional: Fixed) -> Result<IndexProductSnapshot, GravityError> { self.memory.create_index_product(config, seed_notional).await }
pub async fn index_products(&self) -> Result<Vec<IndexProductSnapshot>, GravityError> { self.memory.index_products().await }
pub async fn index_product(&self, id: &str) -> Result<Option<IndexProductSnapshot>, GravityError> { self.memory.index_product(id).await }
pub async fn index_nav(&self, id: &str) -> Result<IndexNavReport, GravityError> { self.memory.index_nav(id).await }
pub async fn index_rebalance(&self, id: &str) -> Result<RebalancePlan, GravityError> { self.memory.index_rebalance(id).await }
pub async fn index_mint_plan(&self, id: &str, account: String, notional: Fixed) -> Result<MintRedeemPlan, GravityError> { self.memory.index_mint_plan(id, account, notional).await }
pub async fn index_redeem_plan(&self, id: &str, account: String, units: Quantity) -> Result<MintRedeemPlan, GravityError> { self.memory.index_redeem_plan(id, account, units).await }
pub async fn index_events(&self, limit: usize) -> Result<Vec<IndexEvent>, GravityError> { self.memory.index_events(limit).await }
pub async fn index_stats(&self) -> Result<IndexStats, GravityError> { self.memory.index_stats().await }

pub async fn stream_stats(&self) -> Result<StreamStats, GravityError> { self.memory.stream_stats().await }
pub async fn recent_stream_records(&self, topic: Option<&str>, limit: usize) -> Result<Vec<StreamRecord>, GravityError> { self.memory.recent_stream_records(topic, limit).await }

pub async fn tile_snapshot(&self) -> Result<TileRuntimeSnapshot, GravityError> { self.memory.tile_snapshot().await }
pub async fn tile_ping(&self) -> Result<TileRuntimeSnapshot, GravityError> { self.memory.tile_ping().await }
pub async fn tile_restart_all(&self) -> Result<TileRuntimeSnapshot, GravityError> { self.memory.tile_restart_all().await }

    pub async fn put_market_event(&self, event: MarketEvent) -> Result<(), GravityError> {
        self.memory.put_market_event(event.clone()).await?;
        if let Some(pg) = &self.postgres { pg.put_market_event(&event).await?; }
        Ok(())
    }

    pub async fn bump(&self, key: &str, amount: u64) -> Result<(), GravityError> { self.memory.bump(key, amount).await }
    pub async fn counters(&self) -> Result<BTreeMap<String, u64>, GravityError> { self.memory.counters().await }
    pub async fn recent_audit(&self, limit: usize) -> Result<Vec<AuditRecord>, GravityError> { self.memory.recent_audit(limit).await }

    pub async fn health(&self) -> Result<String, GravityError> {
        if let Some(pg) = &self.postgres { pg.health().await?; }
        Ok(self.plan.summary())
    }
}

fn redact_url(value: &str) -> String {
    if value.contains('@') { return "<redacted>".into(); }
    value.into()
}
