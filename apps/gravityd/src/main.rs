use gravity_config::{FeedConfig, GravityConfig, GravityPaths, OracleConfig};
use gravity_core::GravityCore;
use gravity_worker::GravityWorker;
use std::path::PathBuf;
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    init_logs();
    if let Err(err) = run().await {
        eprintln!("[gravity] error: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let paths = GravityPaths::from_root(root);
    let gravity = GravityConfig::load(paths.config.join("gravity.toml"))?;
    let oracle = OracleConfig::load(paths.config.join("oracle.toml"))?;
    let feeds = FeedConfig::load(paths.config.join("feeds.toml"))?;
    print_banner(&gravity, &oracle, &feeds);

    tracing::info!(name=%gravity.name, bind=%gravity.bind, queue=gravity.market_queue, settlement=%gravity.settlement_endpoint, "gravity boot");
    tracing::info!(enabled=feeds.enabled, mode=%feeds.adapter_mode, venues=%feeds.venues.join(","), "gravity feeds configured");
    tracing::info!(method=%oracle.method, signing=oracle.signing_enabled, key=%oracle.signing_key_id, "gravity oracle configured");

    let core = GravityCore::new(gravity.clone(), oracle, feeds)?;
    let worker = GravityWorker::new(core.clone());
    let store = core.store();
    let bind = gravity.bind.clone();
    let demo_seconds = parse_seconds();

    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let api = tokio::spawn(async move {
        if let Err(err) = gravity_api::serve(bind, store, async move { let _ = stop_rx.await; }).await {
            tracing::error!(error=%err, "gravity api stopped");
        }
    });

    tracing::info!(bind=%gravity.bind, storage=%core.storage.summary(), "gravity api ready");
    if demo_seconds > 0 {
        tracing::info!(seconds=demo_seconds, "gravity demo started");
        worker.run_demo(demo_seconds).await?;
        let _ = stop_tx.send(());
    } else {
        let _processor = worker.spawn_processor();
        let _market_workers = worker.spawn_market_workers();
        let _feeds = if core.feeds.enabled { worker.spawn_feed_adapters()? } else { Vec::new() };
        tracing::info!("gravity running until Ctrl+C");
        shutdown_signal().await;
        let _ = stop_tx.send(());
    }

    let _ = api.await;
    tracing::info!("gravity stopped cleanly");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("gravity shutdown requested");
}

fn parse_seconds() -> u64 {
    for arg in std::env::args() {
        if let Some(value) = arg.strip_prefix("--demo-seconds=") {
            return value.parse().unwrap_or(0);
        }
    }
    0
}

fn print_banner(gravity: &GravityConfig, oracle: &OracleConfig, feeds: &FeedConfig) {
    println!("============================================================");
    println!(" Gravity v{} | Ascension DeFi Service", env!("CARGO_PKG_VERSION"));
    println!("------------------------------------------------------------");
    println!(" bind       : {}", gravity.bind);
    println!(" storage    : {}", gravity.storage_mode);
    println!(" settlement : {}", gravity.settlement_endpoint);
    println!(" oracle     : {} signing={}", oracle.method, oracle.signing_enabled);
    println!(" feeds      : enabled={} mode={} venues={}", feeds.enabled, feeds.adapter_mode, feeds.venues.join(","));
    println!(" probes     : /live /ready /health /metrics");
    println!(" streams    : /ws/book/{{symbol}} /ws/oracle");
    println!("============================================================");
}

fn init_logs() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "gravity=info,gravityd=info,tower_http=warn".into());
    tracing_subscriber::fmt().with_env_filter(filter).compact().init();
}
