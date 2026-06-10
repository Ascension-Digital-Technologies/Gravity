use gravity_adapters::adapters_for_venues;
use gravity_core::GravityCore;
use gravity_market::{MarketAdapter, MockAdapter};
use gravity_types::{GravityError, Symbol};
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

#[derive(Clone, Debug)]
pub struct MarketWorkerPlan {
    pub symbol: Symbol,
    pub depth_cache_ms: u64,
}

#[derive(Clone)]
pub struct GravityWorker { core: GravityCore }

impl GravityWorker {
    pub fn new(core: GravityCore) -> Self { Self { core } }

    pub fn spawn_processor(&self) -> JoinHandle<()> {
        let core = self.core.clone();
        tokio::spawn(async move {
            if let Err(err) = core.run_processor().await {
                tracing::error!(error=%err, "gravity processor stopped");
            }
        })
    }

    pub fn spawn_market_workers(&self) -> Vec<JoinHandle<()>> {
        let symbols = ["BTC-USDx".to_string(), "ETH-USDx".to_string()];
        let mut handles = Vec::with_capacity(symbols.len());
        for symbol_text in symbols {
            let Ok(symbol) = Symbol::new(symbol_text.clone()) else {
                tracing::warn!(symbol=%symbol_text, "skipping invalid market worker symbol");
                continue;
            };
            let store = self.core.store();
            handles.push(tokio::spawn(async move {
                let plan = MarketWorkerPlan { symbol, depth_cache_ms: 250 };
                let mut interval = tokio::time::interval(Duration::from_millis(plan.depth_cache_ms));
                loop {
                    interval.tick().await;
                    if let Err(err) = store.book_snapshot(plan.symbol.clone(), 50).await {
                        tracing::warn!(symbol=%plan.symbol, error=%err, "market worker snapshot refresh failed");
                    }
                }
            }));
        }
        handles
    }

    pub fn spawn_feed_adapters(&self) -> Result<Vec<JoinHandle<()>>, GravityError> {
        if self.core.feeds.adapter_mode == "mock" {
            return self.spawn_mock_feeds();
        }
        if self.core.feeds.adapter_mode == "live" {
            tracing::warn!("live exchange sockets are not enabled yet; using replay adapters through the same normalization path");
        }
        let mut handles = Vec::new();
        for mut adapter in adapters_for_venues(&self.core.feeds.venues, self.core.feeds.reconnect_ms)? {
            let sender = self.core.sender();
            handles.push(tokio::spawn(async move {
                let name = adapter.name().to_string();
                if let Err(err) = adapter.run(sender).await {
                    tracing::error!(adapter=%name, error=%err, "exchange adapter stopped");
                }
            }));
        }
        Ok(handles)
    }

    pub fn spawn_mock_feeds(&self) -> Result<Vec<JoinHandle<()>>, GravityError> {
        let mut handles = Vec::new();
        for mut adapter in MockAdapter::default_set()? {
            let sender = self.core.sender();
            handles.push(tokio::spawn(async move {
                let name = adapter.name().to_string();
                if let Err(err) = adapter.run(sender).await {
                    tracing::error!(adapter=%name, error=%err, "market adapter stopped");
                }
            }));
        }
        Ok(handles)
    }

    pub async fn run_demo(&self, seconds: u64) -> Result<(), GravityError> {
        let _processor = self.spawn_processor();
        let _market_workers = self.spawn_market_workers();
        let _feeds = self.spawn_feed_adapters()?;
        let start = Instant::now();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let counters = self.core.store().counters().await?;
            let accepted = self.core.settlement().accepted_count().await;
            tracing::info!(?counters, accepted, "gravity demo heartbeat");
            if seconds > 0 && start.elapsed().as_secs() >= seconds { break; }
        }
        Ok(())
    }
}
