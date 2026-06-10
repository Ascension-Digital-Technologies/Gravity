use gravity_book::{Fill, OrderBook, OrderRequest};
use gravity_amm::{AmmPool, PoolConfig, PoolKind, SwapSide};
use gravity_settlement::CompressedSettlementBatch;
use gravity_types::{Fixed, MarketEvent, OrderKind, Price, Quantity, Side, Symbol, TimeInForce, Trade};
use gravity_wire::{decode_order_batch, decode_order_message, encode_order_batch, encode_order_message, OrderWire};
use gravity_tile::{JitMode, JitRegistry, KernelInput, KernelKind, TileCommand, TileConfig, TileKind, TileSupervisor, TileWorker};
use gravity_hardware::{build_plan, HardwareProfile, RuntimeProfile};
use gravity_perf::PerfArena;
use gravity_config::OracleConfig;
use gravity_oracle::OracleEngine;
use gravity_risk::{AccountRiskInput, CollateralInput, PositionInput, RiskEngine};
use gravity_liquidator::{LiquidationEngine, LiquidationMode};
use gravity_perps::{FundingUpdateRequest, PerpEngine, PerpMarketConfig, PerpPositionRequest, PerpSide};
use gravity_index::{IndexAsset, IndexEngine, IndexProductConfig};
use serde::Serialize;
use std::fs;
use std::io;
use std::thread;
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
struct PhaseReport {
    name: &'static str,
    operations: u64,
    elapsed_ms: f64,
    ops_per_sec: f64,
    p50_us: u64,
    p95_us: u64,
    p99_us: u64,
    max_us: u64,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

#[derive(Clone, Debug, Serialize)]
struct BenchReport {
    version: &'static str,
    orders_requested: u64,
    open_orders: usize,
    sequence: u64,
    best_ops_per_sec: f64,
    total_operations: u64,
    phases: Vec<PhaseReport>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let orders = parse_arg("--orders", 100_000_u64);
    let json_out = parse_string_arg("--json-out").unwrap_or_else(|| "runtime/reports/gravity-bench.json".into());
    let csv_out = parse_string_arg("--csv-out").unwrap_or_else(|| "runtime/reports/gravity-bench.csv".into());
    let md_out = parse_string_arg("--md-out").unwrap_or_else(|| "runtime/reports/gravity-release-report.md".into());

    fs::create_dir_all("runtime/reports")?;

    let symbol = Symbol::new("BTC-USDx")?;
    let mut book = OrderBook::new(symbol.clone());
    let mut phases = Vec::new();

    let mut ids = Vec::with_capacity(orders as usize);
    let insert = measure("insert", orders, |i| {
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        let price_units = if side == Side::Buy { 100_000 - (i % 100) as i128 } else { 100_001 + (i % 100) as i128 };
        let result = book.submit(order(&symbol, i, side, TimeInForce::Gtc, price_units, "0.01"))?;
        ids.push(result.order_id);
        Ok(())
    })?;
    phases.push(insert);

    let batch_size = parse_arg("--batch-size", 1024).max(1);
    let batch_orders = parse_arg("--batch-orders", orders).max(1);
    let mut batch_book = OrderBook::new(symbol.clone());
    let batch = measure("batch-insert", batch_orders, |i| {
        let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
        let price_units = if side == Side::Buy { 99_000 - (i % 50) as i128 } else { 101_000 + (i % 50) as i128 };
        let _ = batch_book.submit(order(&symbol, i, side, TimeInForce::Gtc, price_units, "0.01"))?;
        if i % batch_size == 0 { std::hint::black_box(batch_book.stats()); }
        Ok(())
    })?;
    phases.push(batch);

    let snapshot_ops = parse_arg("--snapshots", 10_000).min(orders.max(1));
    let snapshot = measure("snapshot", snapshot_ops, |i| {
        let depth = match i % 4 { 0 => 10, 1 => 25, 2 => 50, _ => 100 };
        let _ = book.snapshot(depth as usize);
        Ok(())
    })?;
    phases.push(snapshot);

    let cancel_ops = parse_arg("--cancels", (orders / 10).max(1)).min(ids.len() as u64);
    let cancel = measure("cancel", cancel_ops, |i| {
        let _ = book.cancel(&ids[i as usize]);
        Ok(())
    })?;
    phases.push(cancel);

    let match_ops = parse_arg("--matches", (orders / 4).max(1));
    let mut match_book = OrderBook::new(symbol.clone());
    for i in 0..match_ops {
        let _ = match_book.submit(order(&symbol, i, Side::Sell, TimeInForce::Gtc, 100_000, "0.01"))?;
    }
    let mut matched_fills: Vec<Fill> = Vec::with_capacity(match_ops as usize);
    let matching = measure("match", match_ops, |i| {
        let result = match_book.submit(order(&symbol, i + match_ops, Side::Buy, TimeInForce::Ioc, 100_001, "0.01"))?;
        matched_fills.extend(result.fills);
        Ok(())
    })?;
    phases.push(matching);

    let compression_ops = parse_arg("--compressions", 1_000).max(1);
    let compression = measure("compress", compression_ops, |_| {
        let batch = CompressedSettlementBatch::from_fills(symbol.clone(), &matched_fills);
        std::hint::black_box(batch.report.compression_ratio_bps);
        Ok(())
    })?;
    phases.push(compression);

    let wire_ops = parse_arg("--wire-ops", orders.min(100_000).max(1));
    let wire_price = Price::new(Fixed::from_units(100_000))?;
    let wire_qty = Quantity::new("0.01".parse::<Fixed>()?)?;
    let wire = measure("wire", wire_ops, |i| {
        let bytes = encode_order_message(&symbol, "bench-wire", Side::Buy, wire_price, wire_qty, i)?;
        let decoded = decode_order_message(&bytes)?;
        std::hint::black_box(decoded.sequence);
        Ok(())
    })?;
    phases.push(wire);

    let wire_batch_size = parse_arg("--wire-batch-size", 1024).max(1).min(65_536) as usize;
    let mut wire_orders = Vec::with_capacity(wire_batch_size);
    for i in 0..wire_batch_size {
        wire_orders.push(OrderWire {
            symbol: symbol.clone(),
            account: format!("wire-batch-{i}"),
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            kind: OrderKind::Limit,
            tif: TimeInForce::Gtc,
            price_raw: Fixed::from_units(100_000 + (i % 10) as i128).as_raw(),
            quantity_raw: wire_qty.0.as_raw(),
            client_id: Some(format!("wb-{i}")),
            sequence: i as u64,
        });
    }
    let wire_batch_ops = parse_arg("--wire-batches", 1_000).max(1);
    let wire_batch = measure("wire-batch", wire_batch_ops, |i| {
        let bytes = encode_order_batch(&wire_orders, i.saturating_mul(wire_batch_size as u64))?;
        let decoded = decode_order_batch(&bytes)?;
        std::hint::black_box(decoded.len());
        Ok(())
    })?;
    phases.push(wire_batch);

    let oracle_ops = parse_arg("--oracle-events", orders.min(100_000).max(1));
    let mut oracle_config = OracleConfig::default();
    oracle_config.min_sources = 3;
    oracle_config.method = "median-vwap".into();
    let mut oracle = OracleEngine::new(oracle_config);
    let venues = ["binance", "coinbase", "kraken", "okx"];
    let oracle_phase = measure("oracle", oracle_ops, |i| {
        let venue = venues[(i as usize) % venues.len()];
        let price_units = 100_000 + ((i % 7) as i128 - 3);
        let event = MarketEvent::Trade(Trade {
            symbol: symbol.clone(),
            venue: venue.into(),
            price: Price::new(Fixed::from_units(price_units))?,
            quantity: Quantity::new(Fixed::from_units(1))?,
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            sequence: i,
            timestamp_ms: gravity_types::now_ms(),
        });
        let report = oracle.ingest(event)?;
        if let Some(report) = report { std::hint::black_box(report.confidence_bps); }
        Ok(())
    })?;

phases.push(oracle_phase);

let amm_ops = parse_arg("--amm-quotes", orders.min(100_000).max(1));
let amm_config = PoolConfig::normalized(symbol.clone(), PoolKind::ConstantProduct, 30, Quantity::new(Fixed::from_units(1))?);
let mut amm_pool = AmmPool::new(amm_config, Quantity::new(Fixed::from_units(100))?, Quantity::new(Fixed::from_units(10_000_000))?)?;
let amm_amount = Quantity::new("0.01".parse::<Fixed>()?)?;
let amm_phase = measure("amm-quote", amm_ops, |i| {
    let side = if i % 2 == 0 { SwapSide::BaseIn } else { SwapSide::QuoteIn };
    let quote = amm_pool.quote(side, amm_amount)?;
    std::hint::black_box(quote.amount_out);
    if i % 1024 == 0 {
        let _ = amm_pool.swap(SwapSide::BaseIn, amm_amount, None)?;
    }
    Ok(())
})?;
phases.push(amm_phase);

let mut stable_cfg = PoolConfig::normalized(Symbol::new("USDC-USDx")?, PoolKind::Stable, 5, Quantity::new(Fixed::from_units(1))?);
stable_cfg.amplification_bps = 100_000;
let mut stable_pool = AmmPool::new(stable_cfg, Quantity::new(Fixed::from_units(5_000_000))?, Quantity::new(Fixed::from_units(5_000_000))?)?;
let mut weighted_cfg = PoolConfig::normalized(Symbol::new("ARC-USDx")?, PoolKind::Weighted, 30, Quantity::new(Fixed::from_units(1))?);
weighted_cfg.base_weight_bps = 8_000;
weighted_cfg.quote_weight_bps = 2_000;
let weighted_pool = AmmPool::new(weighted_cfg, Quantity::new(Fixed::from_units(1_000_000))?, Quantity::new(Fixed::from_units(2_500_000))?)?;
let amm_hardening = measure("amm-hardening", amm_ops, |i| {
    if i % 2 == 0 {
        let quote = stable_pool.quote(SwapSide::BaseIn, amm_amount)?;
        std::hint::black_box(quote.price_impact_bps);
    } else {
        let quote = weighted_pool.quote(SwapSide::QuoteIn, amm_amount)?;
        std::hint::black_box(quote.amount_out);
    }
    if i % 4096 == 0 {
        let snap = stable_pool.snapshot()?;
        let lp = Quantity::new(Fixed::raw((snap.lp_supply.0.as_raw() / 10_000).max(1)))?;
        let _ = stable_pool.remove_liquidity(lp, None, None)?;
    }
    Ok(())
})?;
phases.push(amm_hardening);

let risk_ops = parse_arg("--risk-checks", orders.min(100_000).max(1));
let mut risk = RiskEngine::default();
let risk_phase = measure("risk", risk_ops, |i| {
    let input = AccountRiskInput {
        account: format!("risk-acct-{}", i % 4096),
        collaterals: vec![CollateralInput {
            asset: "USDx".into(),
            quantity: Quantity::new(Fixed::from_units(10_000 + (i % 100) as i128))?,
            price: Price::new(Fixed::ONE)?,
            collateral_factor_bps: 9_000,
        }],
        positions: vec![PositionInput {
            symbol: symbol.clone(),
            quantity: Quantity::new("0.01".parse::<Fixed>()?)?,
            mark_price: Price::new(Fixed::from_units(100_000 + (i % 50) as i128))?,
            side: "long".into(),
        }],
        debt_value: Fixed::from_units(1_000 + (i % 10) as i128),
        maintenance_margin_bps: 1_000,
        initial_margin_bps: 2_000,
        timestamp_ms: Some(i),
    };
    let snapshot = risk.check(input)?;
    std::hint::black_box(snapshot.health_factor_bps);
    Ok(())
})?;
phases.push(risk_phase);

let liquidation_ops = parse_arg("--liquidations", orders.min(100_000).max(1));
let mut liquidation_risk = RiskEngine::default();
let mut snapshots = Vec::with_capacity(liquidation_ops.min(100_000) as usize);
for i in 0..liquidation_ops.min(100_000) {
    let input = AccountRiskInput {
        account: format!("liq-acct-{}", i),
        collaterals: vec![CollateralInput {
            asset: "USDx".into(),
            quantity: Quantity::new(Fixed::from_units(100 + (i % 25) as i128))?,
            price: Price::new(Fixed::ONE)?,
            collateral_factor_bps: 8_500,
        }],
        positions: vec![PositionInput {
            symbol: symbol.clone(),
            quantity: Quantity::new("0.10".parse::<Fixed>()?)?,
            mark_price: Price::new(Fixed::from_units(100_000 + (i % 100) as i128))?,
            side: "long".into(),
        }],
        debt_value: Fixed::from_units(10_000),
        maintenance_margin_bps: 1_000,
        initial_margin_bps: 2_000,
        timestamp_ms: Some(i),
    };
    snapshots.push(liquidation_risk.check(input)?);
}
let mut liquidator = LiquidationEngine::default();
let liquidation_phase = measure("liquidation", liquidation_ops, |i| {
    let idx = (i as usize) % snapshots.len().max(1);
    let candidates = liquidator.scan(vec![snapshots[idx].clone()], 1)?;
    if let Some(candidate) = candidates.first() {
        let plan = liquidator.plan_for_account(&candidate.account, if i % 8 == 0 { LiquidationMode::Full } else { LiquidationMode::Partial })?;
        std::hint::black_box(plan);
    }
    Ok(())
})?;
phases.push(liquidation_phase);


let perp_ops = parse_arg("--perps", orders.min(100_000).max(1));
let mut perps = PerpEngine::new();
let perp_symbol = Symbol::new("BTC-PERP")?;
let perp_cfg = PerpMarketConfig::new(perp_symbol.clone(), symbol.clone());
let _ = perps.create_market(perp_cfg)?;
let perp_phase = measure("perps", perp_ops, |i| {
    if i % 4096 == 0 {
        let _ = perps.update_funding(FundingUpdateRequest {
            symbol: perp_symbol.clone(),
            index_price: Price::new(Fixed::from_units(100_000 + (i % 100) as i128))?,
            mark_price: Price::new(Fixed::from_units(100_010 + (i % 50) as i128))?,
            funding_rate_bps: (i % 25) as i64 - 12,
        })?;
    }
    let position = perps.open_position(PerpPositionRequest {
        account: format!("perp-acct-{}", i % 8192),
        symbol: perp_symbol.clone(),
        side: if i % 2 == 0 { PerpSide::Long } else { PerpSide::Short },
        quantity: Quantity::new("0.01".parse::<Fixed>()?)?,
        entry_price: Price::new(Fixed::from_units(100_000 + (i % 10) as i128))?,
        collateral: Fixed::from_units(2_000),
        leverage_bps: 10_000,
    })?;
    std::hint::black_box(position.equity);
    Ok(())
})?;
phases.push(perp_phase);



let index_ops = parse_arg("--index", orders.min(100_000).max(1));
let mut index = IndexEngine::new();
let index_cfg = IndexProductConfig {
    id: "ASC10".into(),
    name: "Ascension Top 10".into(),
    quote_asset: "USDx".into(),
    management_fee_bps: 25,
    rebalance_threshold_bps: 500,
    min_mint_notional: Fixed::from_units(100),
    assets: vec![
        IndexAsset { symbol: Symbol::new("BTC-USDx")?, target_weight_bps: 5000, oracle_price: Price::new(Fixed::from_units(100_000))? },
        IndexAsset { symbol: Symbol::new("ETH-USDx")?, target_weight_bps: 3000, oracle_price: Price::new(Fixed::from_units(5_000))? },
        IndexAsset { symbol: Symbol::new("ARC-USDx")?, target_weight_bps: 2000, oracle_price: Price::new(Fixed::from_units(10))? },
    ],
};
let _ = index.create_product(index_cfg, Fixed::from_units(1_000_000))?;
let index_phase = measure("index", index_ops, |i| {
    if i % 16 == 0 {
        let plan = index.rebalance_plan("ASC10")?;
        std::hint::black_box(plan.required);
    } else if i % 3 == 0 {
        let plan = index.mint_plan("ASC10", format!("idx-acct-{}", i % 4096), Fixed::from_units(1_000 + (i % 100) as i128))?;
        std::hint::black_box(plan.estimated_units);
    } else if i % 7 == 0 {
        let plan = index.redeem_plan("ASC10", format!("idx-acct-{}", i % 4096), Quantity::new(Fixed::from_units(1))?)?;
        std::hint::black_box(plan.notional);
    } else {
        let nav = index.nav("ASC10")?;
        std::hint::black_box(nav.nav_per_unit);
    }
    Ok(())
})?;
phases.push(index_phase);

let parallel_markets = parse_arg("--parallel-markets", 4).max(1).min(64);
    let parallel_orders = parse_arg("--parallel-orders", orders.max(1));
    let parallel = measure_parallel_markets("parallel-markets", parallel_markets, parallel_orders)?;
    phases.push(parallel);


    let jit_ops = parse_arg("--jit-kernels", orders.min(100_000).max(1));
    let jit_phase = measure_jit_kernels("jit-kernels", jit_ops)?;
    phases.push(jit_phase);

    let hardware_ops = parse_arg("--hardware-plans", 100_000).max(1);
    let hw = HardwareProfile::detect();
    let hardware_phase = measure("hardware-placement", hardware_ops, |i| {
        let profile = match i % 7 {
            0 => RuntimeProfile::Balanced,
            1 => RuntimeProfile::LowLatency,
            2 => RuntimeProfile::HighThroughput,
            3 => RuntimeProfile::MarketMaker,
            4 => RuntimeProfile::OracleHeavy,
            5 => RuntimeProfile::StreamHeavy,
            _ => RuntimeProfile::StorageHeavy,
        };
        let plan = build_plan(profile, &hw);
        std::hint::black_box(plan.placements.len());
        Ok(())
    })?;
    phases.push(hardware_phase);

    let tile_jobs = parse_arg("--tile-jobs", orders.min(250_000).max(1));
    let tile_count = parse_arg("--tiles", parallel_markets).max(1).min(64);
    let tiles = measure_tiles("tiles", tile_count, tile_jobs)?;
    phases.push(tiles);
    let supervisor = measure_tile_supervisor("tile-supervisor", tile_count, tile_jobs)?;
    phases.push(supervisor);


    let perf_ops = parse_arg("--perf-pool", orders.min(250_000).max(1));
    let mut arena = PerfArena::new(64, 65_536, 8192, 4096);
    let perf_phase = measure("perf-pool", perf_ops, |i| {
        let sym = if i % 2 == 0 { "BTC-USDx" } else { "ETH-USDx" };
        let account = format!("perf-acct-{}", i % 8192);
        arena.record(sym, &account, i);
        if i % 4096 == 0 { std::hint::black_box(arena.stats()); }
        Ok(())
    })?;
    phases.push(perf_phase);

    let best_ops_per_sec = phases.iter().map(|p| p.ops_per_sec).fold(0.0, f64::max);
    let total_operations = phases.iter().map(|p| p.operations).sum();
    let report = BenchReport { version: env!("CARGO_PKG_VERSION"), orders_requested: orders, open_orders: book.open_orders(), sequence: book.stats().sequence, best_ops_per_sec, total_operations, phases };

    print_report(&report);
    fs::write(&json_out, serde_json::to_string_pretty(&report)?)?;
    fs::write(&csv_out, csv_report(&report))?;
    fs::write(&md_out, markdown_report(&report))?;

    println!("reports       : {json_out}, {csv_out}, {md_out}");
    Ok(())
}

fn order(symbol: &Symbol, i: u64, side: Side, tif: TimeInForce, price_units: i128, quantity: &str) -> OrderRequest {
    OrderRequest {
        account: format!("bench-{i}"),
        symbol: symbol.clone(),
        side,
        kind: OrderKind::Limit,
        tif,
        price: Some(Price::new(Fixed::from_units(price_units)).expect("static benchmark price is valid")),
        quantity: Quantity::new(quantity.parse::<Fixed>().expect("static benchmark quantity is valid")).expect("static benchmark quantity is positive"),
        client_id: Some(format!("bench-{i}")),
    }
}

fn measure<F>(name: &'static str, operations: u64, mut f: F) -> Result<PhaseReport, Box<dyn std::error::Error>>
where
    F: FnMut(u64) -> Result<(), Box<dyn std::error::Error>>,
{
    let mut latencies_ns = Vec::with_capacity(operations as usize);
    let start = Instant::now();
    for i in 0..operations {
        let op_start = Instant::now();
        f(i)?;
        latencies_ns.push(op_start.elapsed().as_nanos());
    }
    let elapsed = start.elapsed();
    latencies_ns.sort_unstable();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 { operations as f64 / elapsed.as_secs_f64() } else { 0.0 };
    Ok(PhaseReport {
        name,
        operations,
        elapsed_ms,
        ops_per_sec,
        p50_us: (percentile_ns(&latencies_ns, 50.0) / 1_000) as u64,
        p95_us: (percentile_ns(&latencies_ns, 95.0) / 1_000) as u64,
        p99_us: (percentile_ns(&latencies_ns, 99.0) / 1_000) as u64,
        max_us: (latencies_ns.last().copied().unwrap_or(0) / 1_000) as u64,
        p50_ns: percentile_ns(&latencies_ns, 50.0),
        p95_ns: percentile_ns(&latencies_ns, 95.0),
        p99_ns: percentile_ns(&latencies_ns, 99.0),
        max_ns: latencies_ns.last().copied().unwrap_or(0),
    })
}

fn measure_parallel_markets(name: &'static str, markets: u64, operations: u64) -> Result<PhaseReport, Box<dyn std::error::Error>> {
    let per_market = (operations / markets).max(1);
    let start = Instant::now();
    let mut handles = Vec::new();
    for market in 0..markets {
        handles.push(thread::spawn(move || -> Result<u64, String> {
            let symbol = Symbol::new(format!("P{market}-USDx")).map_err(|e| e.to_string())?;
            let mut book = OrderBook::new(symbol.clone());
            for i in 0..per_market {
                let side = if i % 2 == 0 { Side::Buy } else { Side::Sell };
                let price_units = if side == Side::Buy { 100_000 - (i % 100) as i128 } else { 100_001 + (i % 100) as i128 };
                book.submit(order(&symbol, i, side, TimeInForce::Gtc, price_units, "0.01")).map_err(|e| e.to_string())?;
            }
            Ok(per_market)
        }));
    }
    let mut total = 0_u64;
    for handle in handles {
        let joined = handle.join().map_err(|_| io::Error::new(io::ErrorKind::Other, "parallel worker panicked"))?;
        let value = joined.map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        total = total.saturating_add(value);
    }
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 { total as f64 / elapsed.as_secs_f64() } else { 0.0 };
    Ok(PhaseReport { name, operations: total, elapsed_ms, ops_per_sec, p50_us: 0, p95_us: 0, p99_us: 0, max_us: 0, p50_ns: 0, p95_ns: 0, p99_ns: 0, max_ns: 0 })
}


fn measure_jit_kernels(name: &'static str, operations: u64) -> Result<PhaseReport, Box<dyn std::error::Error>> {
    let registry = JitRegistry::with_default_kernels();
    let kinds = [
        KernelKind::FeeBps,
        KernelKind::MarginRequirement,
        KernelKind::HealthBps,
        KernelKind::NetDelta,
        KernelKind::AmmQuote,
        KernelKind::IndexNav,
    ];
    let phase = measure(name, operations, |i| {
        let kind = kinds[(i as usize) % kinds.len()];
        let input = KernelInput::new(
            100_000 + (i % 257) as i128,
            25_000 + (i % 97) as i128,
            (i % 31) as i128,
            25 + (i % 750) as i128,
        );
        let checked = registry.execute_checked(kind, input, JitMode::Warm);
        if !checked.equivalent {
            return Err(format!("JIT equivalence failed for {:?}", kind).into());
        }
        std::hint::black_box(checked.accelerated.value);
        Ok(())
    })?;
    std::hint::black_box(registry.stats());
    Ok(phase)
}

fn measure_tiles(name: &'static str, tiles: u64, operations: u64) -> Result<PhaseReport, Box<dyn std::error::Error>> {
    let per_tile = (operations / tiles).max(1);
    let mut workers = Vec::new();
    for i in 0..tiles {
        let config = TileConfig::new(format!("bench-{i}"), TileKind::Bench)
            .capacity(65_536)
            .batch(2048);
        workers.push(TileWorker::spawn(config));
    }
    let start = Instant::now();
    let mut sent = 0_u64;
    for worker in &workers {
        for i in 0..per_tile {
            if worker.handle.try_send(TileCommand::Ping(i)).is_ok() { sent = sent.saturating_add(1); }
        }
    }
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let processed: u64 = workers.iter().map(|w| w.handle.stats().processed).sum();
        if processed >= sent || Instant::now() >= deadline { break; }
        thread::yield_now();
    }
    let elapsed = start.elapsed();
    for worker in workers { worker.stop(); }
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 { sent as f64 / elapsed.as_secs_f64() } else { 0.0 };
    Ok(PhaseReport { name, operations: sent, elapsed_ms, ops_per_sec, p50_us: 0, p95_us: 0, p99_us: 0, max_us: 0, p50_ns: 0, p95_ns: 0, p99_ns: 0, max_ns: 0 })
}

fn measure_tile_supervisor(name: &'static str, tiles: u64, operations: u64) -> Result<PhaseReport, Box<dyn std::error::Error>> {
    let configs = (0..tiles).map(|i| TileConfig::new(format!("super-{i}"), TileKind::Bench).capacity(65_536).batch(2048)).collect::<Vec<_>>();
    let supervisor = TileSupervisor::start(configs);
    let start = Instant::now();
    let mut sent = 0_u64;
    for i in 0..operations { sent = sent.saturating_add(supervisor.ping_all(i) as u64); }
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = supervisor.snapshot();
        if snapshot.stats.processed >= sent || Instant::now() >= deadline { break; }
        thread::yield_now();
    }
    let elapsed = start.elapsed();
    let snapshot = supervisor.snapshot();
    std::hint::black_box(snapshot.stats.max_pressure_bps);
    let _ = supervisor.stop_all();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let ops_per_sec = if elapsed.as_secs_f64() > 0.0 { sent as f64 / elapsed.as_secs_f64() } else { 0.0 };
    Ok(PhaseReport { name, operations: sent, elapsed_ms, ops_per_sec, p50_us: 0, p95_us: 0, p99_us: 0, max_us: 0, p50_ns: 0, p95_ns: 0, p99_ns: 0, max_ns: 0 })
}

fn percentile_ns(values: &[u128], pct: f64) -> u128 {
    if values.is_empty() { return 0; }
    let rank = ((pct / 100.0) * ((values.len() - 1) as f64)).round() as usize;
    values[rank.min(values.len() - 1)]
}

fn print_report(report: &BenchReport) {
    println!("Gravity performance benchmark v{}", report.version);
    println!("orders requested: {}", report.orders_requested);
    println!("open orders     : {}", report.open_orders);
    println!("sequence        : {}", report.sequence);
    println!("total ops       : {}", report.total_operations);
    println!("best ops/sec    : {:.0}", report.best_ops_per_sec);
    println!();
    println!("{:<16} {:>12} {:>14} {:>12} {:>8} {:>8} {:>8} {:>8} {:>10} {:>10} {:>12} {:>10}", "phase", "ops", "ops/sec", "elapsed_ms", "p50us", "p95us", "p99us", "maxus", "p50ns", "p99ns", "target", "verdict");
    for p in &report.phases {
        let target = target_ops(p.name);
        println!("{:<16} {:>12} {:>14.0} {:>12.3} {:>8} {:>8} {:>8} {:>8} {:>10} {:>10} {:>12} {:>10}", p.name, p.operations, p.ops_per_sec, p.elapsed_ms, p.p50_us, p.p95_us, p.p99_us, p.max_us, p.p50_ns, p.p99_ns, target, verdict(p.ops_per_sec, target));
    }
}

fn target_ops(name: &str) -> u64 {
    match name {
        "insert" => 1_000_000,
        "batch-insert" => 2_000_000,
        "snapshot" => 500_000,
        "cancel" => 100_000,
        "match" => 1_000_000,
        "compress" => 50_000,
        "wire" => 5_000_000,
        "wire-batch" => 10_000,
        "parallel-markets" => 3_000_000,
        "amm-quote" => 1_000_000,
        "amm-hardening" => 750_000,
        "risk" => 1_000_000,
        "liquidation" => 100_000,
        "perps" => 250_000,
        "tiles" => 10_000_000,
        "tile-supervisor" => 5_000_000,
        "jit-kernels" => 1_000_000,
        "hardware-placement" => 1_000_000,
        "perf-pool" => 5_000_000,
        _ => 0,
    }
}

fn verdict(actual: f64, target: u64) -> &'static str {
    if target == 0 { return "n/a"; }
    if actual >= target as f64 { "pass" } else if actual >= (target as f64 * 0.50) { "watch" } else { "fail" }
}

fn csv_report(report: &BenchReport) -> String {
    let mut out = String::from("phase,operations,elapsed_ms,ops_per_sec,p50_us,p95_us,p99_us,max_us,p50_ns,p95_ns,p99_ns,max_ns,target_ops_sec,verdict\n");
    for p in &report.phases {
        let target = target_ops(p.name);
        out.push_str(&format!("{},{},{:.3},{:.0},{},{},{},{},{},{},{},{},{},{}\n", p.name, p.operations, p.elapsed_ms, p.ops_per_sec, p.p50_us, p.p95_us, p.p99_us, p.max_us, p.p50_ns, p.p95_ns, p.p99_ns, p.max_ns, target, verdict(p.ops_per_sec, target)));
    }
    out
}

fn markdown_report(report: &BenchReport) -> String {
    let mut out = format!("# Gravity Release Performance Report\n\nVersion: `{}`\n\nOrders requested: `{}`  \nOpen orders after insert/cancel: `{}`  \nSequence: `{}`  \nTotal operations: `{}`  \nBest ops/sec: `{:.0}`\n\n", report.version, report.orders_requested, report.open_orders, report.sequence, report.total_operations, report.best_ops_per_sec);
    out.push_str("| Phase | Operations | Ops/sec | Target ops/sec | Verdict | Elapsed ms | p50 us | p95 us | p99 us | Max us | p50 ns | p99 ns |\n");
    out.push_str("|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|\n");
    for p in &report.phases {
        let target = target_ops(p.name);
        out.push_str(&format!("| {} | {} | {:.0} | {} | {} | {:.3} | {} | {} | {} | {} | {} | {} |\n", p.name, p.operations, p.ops_per_sec, target, verdict(p.ops_per_sec, target), p.elapsed_ms, p.p50_us, p.p95_us, p.p99_us, p.max_us, p.p50_ns, p.p99_ns));
    }
    out.push_str("\n## Verdict guide\n\n- `pass`: phase met or exceeded its current release target.\n- `watch`: phase reached at least 50% of target and should be tuned.\n- `fail`: phase is a bottleneck for the following performance pass.\n");
    out
}

fn parse_arg(name: &str, default: u64) -> u64 {
    let prefix = format!("{name}=");
    std::env::args().find_map(|arg| arg.strip_prefix(&prefix).and_then(|value| value.parse().ok())).unwrap_or(default)
}

fn parse_string_arg(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    std::env::args().find_map(|arg| arg.strip_prefix(&prefix).map(str::to_owned))
}
