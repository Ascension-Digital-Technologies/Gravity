use gravity_config::{FeedConfig, GravityConfig, OracleConfig};
use gravity_database::{GravityStore, StoragePlan};
use gravity_market::{market_bus, FeedMonitor, MarketReceiver, MarketSender};
use gravity_oracle::OracleEngine;
use gravity_settlement::SettlementClient;
use gravity_types::{GravityError, MarketEvent};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct GravityCore {
    pub config: GravityConfig,
    pub feeds: FeedConfig,
    pub storage: StoragePlan,
    store: GravityStore,
    sender: MarketSender,
    receiver: Arc<Mutex<MarketReceiver>>,
    monitor: Arc<Mutex<FeedMonitor>>,
    oracle: Arc<Mutex<OracleEngine>>,
    settlement: SettlementClient,
}

impl GravityCore {
    pub fn new(config: GravityConfig, oracle_config: OracleConfig, feeds: FeedConfig) -> Result<Self, GravityError> {
        let (sender, receiver) = market_bus(config.market_queue);
        let settlement = SettlementClient::new(config.settlement_endpoint.clone());
        let storage = StoragePlan::new(&config.storage_mode, config.postgres_url.clone(), config.redis_url.clone())?;
        let store = GravityStore::new(storage.clone())?;
        Ok(Self {
            config,
            monitor: Arc::new(Mutex::new(FeedMonitor::with_max_gap(feeds.max_gap))),
            feeds,
            storage,
            store,
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            oracle: Arc::new(Mutex::new(OracleEngine::new(oracle_config))),
            settlement,
        })
    }

    pub fn sender(&self) -> MarketSender { self.sender.clone() }
    pub fn store(&self) -> GravityStore { self.store.clone() }
    pub fn settlement(&self) -> SettlementClient { self.settlement.clone() }

    pub async fn recv_event(&self) -> Option<MarketEvent> {
        self.receiver.lock().await.recv().await
    }

    pub async fn process_event(&self, event: MarketEvent) -> Result<bool, GravityError> {
        self.store.put_market_event(event.clone()).await?;
        let accepted = self.monitor.lock().await.accept(&event);
        if !accepted {
            self.store.bump("feed_rejected", 1).await?;
            return Ok(false);
        }
        let (report, sources, stats) = {
            let mut oracle = self.oracle.lock().await;
            let report = oracle.ingest(event)?;
            let sources = oracle.source_health();
            let stats = oracle.stats();
            (report, sources, stats)
        };
        self.store.put_oracle_sources(sources).await?;
        self.store.bump("oracle_sources", stats.sources as u64).await?;
        self.store.bump("oracle_outliers", stats.outlier_sources as u64).await?;
        if let Some(report) = report {
            let verified = {
                let oracle = self.oracle.lock().await;
                oracle.verify(&report)
            };
            if verified { self.store.bump("oracle_verified", 1).await?; }
            self.store.put_oracle(report.clone()).await?;
            let _receipt = self.settlement.submit_oracle(&report).await?;
            self.store.bump("settlement_payloads", 1).await?;
        }
        Ok(true)
    }

    pub async fn run_processor(&self) -> Result<(), GravityError> {
        while let Some(event) = self.recv_event().await {
            if let Err(err) = self.process_event(event).await {
                self.store.bump("processor_errors", 1).await?;
                tracing::warn!(error=%err, "gravity processor rejected event");
            }
        }
        Ok(())
    }
}
