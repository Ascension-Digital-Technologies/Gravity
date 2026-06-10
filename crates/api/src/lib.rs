use axum::{
    body::Bytes,
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use gravity_book::{AmendRequest, MarketStatus, OrderRequest};
use gravity_amm::{PoolConfig, PoolKind, SwapSide};
use gravity_risk::{AccountRiskInput, CollateralInput, PositionInput};
use gravity_liquidator::LiquidationMode;
use gravity_perps::{FundingUpdateRequest, PerpMarketConfig, PerpPositionRequest, PerpSide};
use gravity_index::{IndexAsset, IndexProductConfig};
use gravity_database::GravityStore;
use gravity_types::{error_json, Fixed, GravityError, HealthStatus, OrderKind, Price, Quantity, Side, Symbol, TimeInForce};
use gravity_wire::{decode_order_batch, decode_order_message, OrderWire};
use serde::Deserialize;
use serde_json::Value;
use std::net::SocketAddr;
use std::str::FromStr;
use std::time::Duration;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

#[derive(Clone)]
pub struct ApiState {
    store: GravityStore,
    version: &'static str,
    started_ms: u64,
}

impl ApiState {
    pub fn new(store: GravityStore) -> Self { Self { store, version: env!("CARGO_PKG_VERSION"), started_ms: gravity_types::now_ms() } }
}

pub fn router(store: GravityStore) -> Router {
    Router::new()
        .route("/", get(health))
        .route("/api", get(api_contract))
        .route("/api/version", get(api_version))
        .route("/api/errors", get(api_errors))
        .route("/api/routes", get(api_routes))
        .route("/api/openapi", get(api_openapi))
        .route("/health", get(health))
        .route("/live", get(live))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
        .route("/ops", get(ops_overview))
        .route("/ops/health", get(ops_health))
        .route("/ops/startup", get(ops_startup))
        .route("/ops/queues", get(ops_queues))
        .route("/ops/latency", get(ops_latency))
        .route("/ops/services", get(ops_services))
        .route("/ops/performance", get(ops_performance))
        .route("/streams", get(streams))
        .route("/streams/recent", get(streams_recent))
        .route("/streams/{topic}/recent", get(stream_topic_recent))
        .route("/workers", get(workers))
        .route("/tiles", get(tiles))
        .route("/tiles/health", get(tiles_health))
        .route("/tiles/ping", post(tiles_ping))
        .route("/tiles/restart", post(tiles_restart))
        .route("/hardware", get(hardware_profile))
        .route("/hardware/plan", get(hardware_plan))
        .route("/hardware/simulate", get(hardware_simulate))
        .route("/persistence", get(persistence))
        .route("/persistence/recent", get(persistence_recent))
        .route("/database", get(database_report))
        .route("/database/migrations", get(database_migrations))
        .route("/database/backpressure", get(database_backpressure))
        .route("/wal", get(wal_stats))
        .route("/wal/recent", get(wal_recent))
        .route("/wal/checkpoint", post(wal_checkpoint))
        .route("/wal/replay-plan", get(wal_replay_plan))
        .route("/wal/checkpoints", get(wal_checkpoints))
        .route("/wal/recovery-report", get(wal_recovery_report))
        .route("/wal/replay-run", post(wal_replay_run))
        .route("/settlement", get(settlement))
        .route("/settlement/recent", get(settlement_recent))
        .route("/settlement/dead-letter", get(settlement_dead_letter))
        .route("/settlement/dead-letter/retry", post(settlement_retry_dead_letter))
        .route("/audit", get(audit))
        .route("/oracle", get(all_oracles))
        .route("/oracle/stats", get(oracle_stats))
        .route("/oracle/sources", get(oracle_sources))
        .route("/oracle/{symbol}", get(oracle_by_symbol))
        .route("/book/{symbol}", get(book_by_symbol))
        .route("/book/{symbol}/events", get(book_events))
        .route("/events/book", get(all_book_events))
        .route("/stream/book/{symbol}", get(book_events))
        .route("/ws/book/{symbol}", get(ws_book))
        .route("/ws/oracle", get(ws_oracle))
        .route("/stream/oracle", get(all_oracles))
        .route("/orders", post(submit_order))
        .route("/orders/batch", post(submit_orders))
        .route("/binary/orders", post(submit_binary_order))
        .route("/binary/orders/batch", post(submit_binary_orders))
        .route("/orders/{symbol}/{id}/cancel", post(cancel_order))
        .route("/orders/{symbol}/{id}/amend", post(amend_order))
        .route("/orders/{symbol}/{id}/replace", post(replace_order))
        .route("/markets/{symbol}/status", post(set_market_status))
        .route("/amm/pools", get(amm_pools).post(create_amm_pool))
        .route("/amm/pools/{symbol}", get(amm_pool))
        .route("/amm/pools/{symbol}/quote", post(amm_quote))
        .route("/amm/pools/{symbol}/swap", post(amm_swap))
        .route("/amm/pools/{symbol}/liquidity", post(amm_add_liquidity))
        .route("/amm/pools/{symbol}/liquidity/remove", post(amm_remove_liquidity))
        .route("/amm/pools/{symbol}/oracle-guard", post(amm_oracle_guard))
        .route("/amm/events", get(amm_events))
        .route("/risk/accounts/{account}", get(risk_account))
        .route("/risk/check", post(risk_check))
        .route("/risk/events", get(risk_events))
        .route("/risk/stats", get(risk_stats))
        .route("/liquidations/scan", post(liquidation_scan))
        .route("/liquidations/candidates", get(liquidation_candidates))
        .route("/liquidations/accounts/{account}/plan", post(liquidation_plan))
        .route("/liquidations/events", get(liquidation_events))
        .route("/liquidations/stats", get(liquidation_stats))
        .route("/perps/markets", get(perp_markets).post(create_perp_market))
        .route("/perps/markets/{symbol}", get(perp_market))
        .route("/perps/positions", get(perp_positions))
        .route("/perps/positions/open", post(open_perp_position))
        .route("/perps/accounts/{account}/positions", get(perp_account_positions))
        .route("/perps/funding", post(update_perp_funding))
        .route("/perps/events", get(perp_events))
        .route("/perps/stats", get(perp_stats))
        .route("/index/products", get(index_products).post(create_index_product))
        .route("/index/products/{id}", get(index_product))
        .route("/index/products/{id}/nav", post(index_nav))
        .route("/index/products/{id}/rebalance", post(index_rebalance))
        .route("/index/products/{id}/mint", post(index_mint))
        .route("/index/products/{id}/redeem", post(index_redeem))
        .route("/index/events", get(index_events))
        .route("/index/stats", get(index_stats))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(ApiState::new(store))
}

pub async fn serve(bind: String, store: GravityStore, shutdown: impl std::future::Future<Output = ()> + Send + 'static) -> Result<(), GravityError> {
    let addr: SocketAddr = bind.parse().map_err(|err| GravityError::InvalidConfig(format!("invalid bind address {bind}: {err}")))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router(store))
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|err| GravityError::Network(err.to_string()))
}

fn public_api_routes() -> Vec<&'static str> {
    vec![
        "GET /api",
        "GET /api/version",
        "GET /api/errors",
        "GET /api/routes",
        "GET /api/openapi",
        "GET /health",
        "GET /live",
        "GET /ready",
        "GET /metrics",
        "GET /ops",
        "GET /ops/health",
        "GET /ops/startup",
        "GET /ops/queues",
        "GET /ops/latency",
        "GET /ops/services",
        "GET /ops/performance",
        "GET /streams",
        "GET /streams/recent",
        "GET /streams/{topic}/recent",
        "GET /workers",
        "GET /tiles",
        "GET /tiles/health",
        "POST /tiles/ping",
        "POST /tiles/restart",
        "GET /hardware",
        "GET /hardware/plan",
        "GET /hardware/simulate",
        "GET /database",
        "GET /database/migrations",
        "GET /database/backpressure",
        "GET /wal",
        "GET /wal/recent",
        "POST /wal/checkpoint",
        "GET /wal/replay-plan",
        "GET /wal/checkpoints",
        "GET /wal/recovery-report",
        "POST /wal/replay-run",
        "GET /settlement",
        "GET /settlement/recent",
        "GET /settlement/dead-letter",
        "POST /settlement/dead-letter/retry",
        "GET /audit",
        "GET /oracle",
        "GET /oracle/stats",
        "GET /oracle/sources",
        "GET /oracle/{symbol}",
        "GET /book/{symbol}",
        "GET /book/{symbol}/events",
        "GET /events/book",
        "GET /ws/book/{symbol}",
        "GET /ws/oracle",
        "POST /orders",
        "POST /orders/batch",
        "POST /binary/orders",
        "POST /binary/orders/batch",
        "POST /orders/{symbol}/{id}/cancel",
        "POST /orders/{symbol}/{id}/amend",
        "POST /orders/{symbol}/{id}/replace",
        "POST /markets/{symbol}/status",
        "GET /amm/pools",
        "POST /amm/pools",
        "GET /amm/pools/{symbol}",
        "POST /amm/pools/{symbol}/quote",
        "POST /amm/pools/{symbol}/swap",
        "POST /amm/pools/{symbol}/liquidity",
        "POST /amm/pools/{symbol}/liquidity/remove",
        "POST /amm/pools/{symbol}/oracle-guard",
        "GET /amm/events",
        "GET /risk/accounts/{account}",
        "POST /risk/check",
        "GET /risk/events",
        "GET /risk/stats",
        "POST /liquidations/scan",
        "GET /liquidations/candidates",
        "POST /liquidations/accounts/{account}/plan",
        "GET /liquidations/events",
        "GET /liquidations/stats",
        "GET /perps/markets",
        "POST /perps/markets",
        "GET /perps/markets/{symbol}",
        "GET /perps/positions",
        "POST /perps/positions/open",
        "GET /perps/accounts/{account}/positions",
        "POST /perps/funding",
        "GET /perps/events",
        "GET /perps/stats",
        "GET /index/products",
        "POST /index/products",
        "GET /index/products/{id}",
        "POST /index/products/{id}/nav",
        "POST /index/products/{id}/rebalance",
        "POST /index/products/{id}/mint",
        "POST /index/products/{id}/redeem",
        "GET /index/events",
        "GET /index/stats",
    ]
}

fn error_catalog() -> Value {
    serde_json::json!({
        "error_codes": [
            {"code": "invalid_request", "status": 400, "meaning": "Malformed input, unsupported parameter, or failed validation."},
            {"code": "not_found", "status": 404, "meaning": "Requested market, account, pool, product, or resource does not exist."},
            {"code": "conflict", "status": 409, "meaning": "Idempotency conflict, duplicate request, or state transition conflict."},
            {"code": "rate_limited", "status": 429, "meaning": "Request was throttled by future rate-limit policy."},
            {"code": "internal", "status": 500, "meaning": "Unexpected Gravity service failure."},
            {"code": "unavailable", "status": 503, "meaning": "Dependency, storage, stream, tile, or worker is unhealthy."}
        ],
        "envelope": {
            "ok": false,
            "request_id": "string",
            "data": null,
            "error": {"code": "invalid_request", "message": "human-readable failure", "details": {}}
        }
    })
}

async fn api_contract(State(state): State<ApiState>) -> Json<Value> {
    Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "contract_version": "v3.6.0",
        "api_version": "v1",
        "status": "contract-ready",
        "response_envelope": {
            "ok": true,
            "request_id": "string",
            "data": {},
            "error": null
        },
        "headers": {
            "request_id": "x-request-id",
            "idempotency_key": "idempotency-key"
        },
        "pagination": {
            "limit": "optional integer, capped by endpoint",
            "cursor": "reserved for cursor based pagination"
        },
        "contracts": {
            "rest": "/api/routes",
            "errors": "/api/errors",
            "openapi": "/api/openapi",
            "binary": "docs/WIRE.md and docs/BINARY-PROTOCOL-CONTRACT.md",
            "sdk": "docs/SDK-CONTRACT.md"
        }
    }))
}

async fn api_version(State(state): State<ApiState>) -> Json<Value> {
    Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "api_version": "v1",
        "contract_version": "v3.6.0",
        "started_ms": state.started_ms,
        "uptime_ms": gravity_types::now_ms().saturating_sub(state.started_ms)
    }))
}

async fn api_errors() -> Json<Value> {
    Json(error_catalog())
}

async fn api_routes() -> Json<Value> {
    Json(serde_json::json!({
        "api_version": "v1",
        "route_count": public_api_routes().len(),
        "routes": public_api_routes(),
        "admin_prefixes": ["/tiles", "/hardware", "/database", "/wal", "/settlement/dead-letter"],
        "public_prefixes": ["/oracle", "/book", "/amm", "/risk", "/liquidations", "/perps", "/index", "/streams"],
        "binary_prefixes": ["/binary"]
    }))
}

async fn api_openapi(State(state): State<ApiState>) -> Json<Value> {
    let mut paths = serde_json::Map::new();
    for route in public_api_routes() {
        let mut parts = route.splitn(2, ' ');
        let method = parts.next().unwrap_or("GET").to_lowercase();
        let path = parts.next().unwrap_or("/");
        let entry = paths.entry(path.to_string()).or_insert_with(|| serde_json::json!({}));
        if let Some(map) = entry.as_object_mut() {
            map.insert(method, serde_json::json!({
                "summary": format!("Gravity {route}"),
                "responses": {
                    "200": {"description": "Successful Gravity response"},
                    "400": {"description": "Invalid request"},
                    "500": {"description": "Internal error"}
                }
            }));
        }
    }
    Json(serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Gravity API",
            "version": state.version,
            "description": "Production API and SDK contract for Ascension Gravity."
        },
        "servers": [{"url": "http://127.0.0.1:8080", "description": "local Gravity runtime"}],
        "paths": paths,
        "components": {
            "schemas": {
                "GravityEnvelope": {
                    "type": "object",
                    "properties": {
                        "ok": {"type": "boolean"},
                        "request_id": {"type": "string"},
                        "data": {"type": "object"},
                        "error": {"type": ["object", "null"]}
                    }
                }
            }
        }
    }))
}

async fn health(State(state): State<ApiState>) -> Result<Json<HealthStatus>, ApiError> {
    let storage = state.store.health().await?;
    Ok(Json(HealthStatus { status: "ok".into(), service: "gravity".into(), version: state.version.into(), storage }))
}

async fn live(State(state): State<ApiState>) -> Json<Value> {
    Json(serde_json::json!({
        "status": "live",
        "service": "gravity",
        "version": state.version,
        "uptime_ms": gravity_types::now_ms().saturating_sub(state.started_ms)
    }))
}

async fn ready(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let storage = state.store.health().await?;
    Ok(Json(serde_json::json!({
        "status": "ready",
        "service": "gravity",
        "version": state.version,
        "storage": storage,
        "checks": { "api": "ok", "storage": "ok" }
    })))
}

async fn metrics(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let counters = state.store.counters().await?;
    let workers = state.store.worker_stats().await?;
    Ok(Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "uptime_ms": gravity_types::now_ms().saturating_sub(state.started_ms),
        "counters": counters,
        "workers": workers,
        "risk": state.store.risk_stats().await?,
        "liquidations": state.store.liquidation_stats().await?,
        "perps": state.store.perp_stats().await?,
        "index": state.store.index_stats().await?,
        "streams": state.store.stream_stats().await?,
        "tiles": state.store.tile_snapshot().await?,
        "hardware": state.store.hardware_plan(None).await?,
        "recovery": state.store.wal_recovery_report().await?,
        "database": state.store.database_report().await?
    })))
}

async fn ops_overview(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let now = gravity_types::now_ms();
    Ok(Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "ops_version": "v3.6.0",
        "started_ms": state.started_ms,
        "uptime_ms": now.saturating_sub(state.started_ms),
        "health": {
            "live": true,
            "ready": true,
            "mode": "observability-foundation"
        },
        "services": {
            "workers": state.store.worker_stats().await?,
            "tiles": state.store.tile_snapshot().await?,
            "streams": state.store.stream_stats().await?,
            "database": state.store.database_report().await?,
            "wal_recovery": state.store.wal_recovery_report().await?,
            "settlement": state.store.settlement_stats().await?,
            "risk": state.store.risk_stats().await?,
            "liquidations": state.store.liquidation_stats().await?,
            "oracle": state.store.oracle_stats().await?
        }
    })))
}

async fn ops_health(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let database = state.store.database_report().await?;
    let recovery = state.store.wal_recovery_report().await?;
    let tiles = state.store.tile_snapshot().await?;
    let streams = state.store.stream_stats().await?;
    Ok(Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "live": true,
        "ready": true,
        "verdict": "operational",
        "database": database,
        "recovery": recovery,
        "tiles": tiles,
        "streams": streams
    })))
}

async fn ops_startup(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "started_ms": state.started_ms,
        "uptime_ms": gravity_types::now_ms().saturating_sub(state.started_ms),
        "hardware": state.store.hardware_profile().await?,
        "placement": state.store.hardware_plan(None).await?,
        "database": state.store.database_report().await?,
        "recovery": state.store.wal_recovery_report().await?,
        "streams": state.store.stream_stats().await?,
        "tiles": state.store.tile_snapshot().await?
    })))
}

async fn ops_queues(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let workers = state.store.worker_stats().await?;
    let persistence = state.store.persistence_stats().await?;
    let backpressure = state.store.storage_backpressure().await?;
    let tiles = state.store.tile_snapshot().await?;
    Ok(Json(serde_json::json!({
        "workers": workers,
        "persistence": persistence,
        "storage_backpressure": backpressure,
        "tiles": tiles
    })))
}

async fn ops_latency(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let workers = state.store.worker_stats().await?;
    let worker_count = workers.len();
    let max_worker_latency_us = workers.iter().map(|w| w.max_latency_us).max().unwrap_or(0);
    let avg_worker_latency_us = if worker_count == 0 { 0 } else { workers.iter().map(|w| w.avg_latency_us).sum::<u64>() / worker_count as u64 };
    Ok(Json(serde_json::json!({
        "workers": workers,
        "worker_count": worker_count,
        "max_worker_latency_us": max_worker_latency_us,
        "avg_worker_latency_us": avg_worker_latency_us,
        "tiles": state.store.tile_snapshot().await?,
        "streams": state.store.stream_stats().await?
    })))
}

async fn ops_services(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "database": state.store.database_report().await?,
        "migrations": state.store.migration_status().await?,
        "wal": state.store.wal_stats().await?,
        "recovery": state.store.wal_recovery_report().await?,
        "settlement": state.store.settlement_stats().await?,
        "streams": state.store.stream_stats().await?,
        "risk": state.store.risk_stats().await?,
        "liquidations": state.store.liquidation_stats().await?,
        "perps": state.store.perp_stats().await?,
        "index": state.store.index_stats().await?,
        "oracle": state.store.oracle_stats().await?
    })))
}

async fn ops_performance(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({
        "service": "gravity",
        "version": state.version,
        "hardware": state.store.hardware_plan(Some("high-throughput")).await?,
        "queues": {
            "workers": state.store.worker_stats().await?,
            "persistence": state.store.persistence_stats().await?,
            "storage": state.store.storage_backpressure().await?
        },
        "runtime": {
            "tiles": state.store.tile_snapshot().await?,
            "streams": state.store.stream_stats().await?,
            "counters": state.store.counters().await?
        }
    })))
}

async fn workers(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let workers = state.store.worker_stats().await?;
    Ok(Json(serde_json::json!({ "workers": workers })))
}


async fn tiles(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.tile_snapshot().await?;
    Ok(Json(serde_json::json!({ "tiles": snapshot })))
}

async fn tiles_health(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.tile_snapshot().await?;
    Ok(Json(serde_json::json!({ "health": snapshot.stats, "tiles": snapshot.tiles })))
}

async fn tiles_ping(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.tile_ping().await?;
    Ok(Json(serde_json::json!({ "tiles": snapshot, "action": "ping" })))
}

async fn tiles_restart(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.tile_restart_all().await?;
    Ok(Json(serde_json::json!({ "tiles": snapshot, "action": "restart-all" })))
}



#[derive(Deserialize)]
struct HardwareQuery { profile: Option<String> }

async fn hardware_profile(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let profile = state.store.hardware_profile().await?;
    Ok(Json(serde_json::json!({ "hardware": profile })))
}

async fn hardware_plan(State(state): State<ApiState>, Query(query): Query<HardwareQuery>) -> Result<Json<Value>, ApiError> {
    let plan = state.store.hardware_plan(query.profile.as_deref()).await?;
    Ok(Json(serde_json::json!({ "plan": plan })))
}

async fn hardware_simulate(State(state): State<ApiState>, Query(query): Query<HardwareQuery>) -> Result<Json<Value>, ApiError> {
    let simulation = state.store.hardware_simulate(query.profile.as_deref()).await?;
    Ok(Json(serde_json::json!({ "simulation": simulation })))
}

async fn streams(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let stats = state.store.stream_stats().await?;
    Ok(Json(serde_json::json!({ "streams": stats })))
}

async fn streams_recent(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_stream_records(None, limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

async fn stream_topic_recent(State(state): State<ApiState>, Path(topic): Path<String>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_stream_records(Some(&topic), limit).await?;
    Ok(Json(serde_json::json!({ "topic": topic, "records": records, "limit": limit })))
}

async fn persistence(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let stats = state.store.persistence_stats().await?;
    Ok(Json(serde_json::json!({ "persistence": stats, "mode": "hot-cold-queue" })))
}

async fn persistence_recent(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_persistence(limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

async fn database_report(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({ "database": state.store.database_report().await? })))
}

async fn database_migrations(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({ "migrations": state.store.migration_status().await? })))
}

async fn database_backpressure(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!({ "backpressure": state.store.storage_backpressure().await? })))
}

async fn wal_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let stats = state.store.wal_stats().await?;
    Ok(Json(serde_json::json!({ "wal": stats, "mode": "append-only-foundation" })))
}

async fn wal_recent(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_wal(limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

#[derive(Clone, Debug, Deserialize)]
struct WalCheckpointDto { note: Option<String> }

async fn wal_checkpoint(State(state): State<ApiState>, body: Option<Json<WalCheckpointDto>>) -> Result<Json<Value>, ApiError> {
    let note = body.and_then(|Json(v)| v.note).unwrap_or_else(|| "manual checkpoint".to_string());
    let checkpoint = state.store.wal_checkpoint(note).await?;
    Ok(Json(serde_json::json!({ "checkpoint": checkpoint })))
}

async fn wal_replay_plan(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let plan = state.store.wal_replay_plan().await?;
    Ok(Json(serde_json::json!({ "replay": plan })))
}

async fn wal_checkpoints(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let checkpoints = state.store.wal_checkpoints(limit).await?;
    Ok(Json(serde_json::json!({ "checkpoints": checkpoints, "limit": limit })))
}

async fn wal_recovery_report(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let report = state.store.wal_recovery_report().await?;
    Ok(Json(serde_json::json!({ "recovery": report })))
}

async fn wal_replay_run(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let report = state.store.wal_replay_dry_run().await?;
    Ok(Json(serde_json::json!({ "mode": "dry-run", "recovery": report })))
}


async fn settlement(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let stats = state.store.settlement_stats().await?;
    Ok(Json(serde_json::json!({ "settlement": stats, "mode": "local-finalizer" })))
}

async fn settlement_recent(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_settlements(limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

async fn settlement_dead_letter(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.dead_letter_settlements(limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

async fn settlement_retry_dead_letter(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let receipt = state.store.retry_dead_letter_settlements(limit).await?;
    Ok(Json(serde_json::json!({ "receipt": receipt, "limit": limit })))
}

async fn audit(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let records = state.store.recent_audit(limit).await?;
    Ok(Json(serde_json::json!({ "records": records, "limit": limit })))
}

async fn all_oracles(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let reports = state.store.all_oracles().await?;
    Ok(Json(serde_json::json!(reports)))
}

async fn oracle_sources(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let sources = state.store.oracle_sources().await?;
    Ok(Json(serde_json::json!({ "sources": sources })))
}

async fn oracle_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let stats = state.store.oracle_stats().await?;
    Ok(Json(serde_json::json!({ "oracle": stats })))
}

async fn oracle_by_symbol(State(state): State<ApiState>, Path(symbol): Path<String>) -> Result<Response, ApiError> {
    match state.store.get_oracle(&symbol).await? {
        Some(report) => Ok(Json(report).into_response()),
        None => Ok((StatusCode::NOT_FOUND, Json(error_json(format!("oracle report not found for {symbol}")))).into_response()),
    }
}

#[derive(Clone, Debug, Deserialize)]
struct DepthQuery { depth: Option<usize>, limit: Option<usize> }

async fn book_by_symbol(State(state): State<ApiState>, Path(symbol): Path<String>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol)?;
    let depth = query.depth.unwrap_or(25).min(500);
    let snapshot = state.store.book_snapshot(symbol, depth).await?;
    Ok(Json(serde_json::json!(snapshot)))
}


async fn all_book_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(1000);
    let events = state.store.recent_book_events(None, limit).await?;
    Ok(Json(serde_json::json!({ "events": events, "stream": "book", "mode": "snapshot" })))
}

async fn book_events(State(state): State<ApiState>, Path(symbol): Path<String>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol)?;
    let limit = query.limit.unwrap_or(100).min(1000);
    let events = state.store.recent_book_events(Some(symbol), limit).await?;
    Ok(Json(serde_json::json!({ "events": events, "stream": "book", "mode": "snapshot" })))
}

async fn ws_book(State(state): State<ApiState>, Path(symbol): Path<String>, ws: WebSocketUpgrade) -> Result<Response, ApiError> {
    let symbol = Symbol::new(symbol)?;
    Ok(ws.on_upgrade(move |socket| async move { stream_book(socket, state, symbol).await }).into_response())
}

async fn ws_oracle(State(state): State<ApiState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| async move { stream_oracle(socket, state).await }).into_response()
}

async fn stream_book(mut socket: WebSocket, state: ApiState, symbol: Symbol) {
    if let Ok(snapshot) = state.store.book_snapshot(symbol.clone(), 50).await {
        let payload = serde_json::json!({ "type": "book_snapshot", "symbol": symbol.to_string(), "snapshot": snapshot });
        if send_json(&mut socket, payload).await.is_err() { return; }
    }

    let mut rx = state.store.subscribe_book_events();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(event) if event.symbol == symbol => {
                        let payload = serde_json::json!({ "type": "book_event", "symbol": symbol.to_string(), "event": event });
                        if send_json(&mut socket, payload).await.is_err() { break; }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let payload = serde_json::json!({ "type": "lagged", "skipped": skipped });
                        if send_json(&mut socket, payload).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = heartbeat.tick() => {
                let payload = serde_json::json!({ "type": "heartbeat", "stream": "book", "symbol": symbol.to_string(), "ts": gravity_types::now_ms() });
                if send_json(&mut socket, payload).await.is_err() { break; }
            }
        }
    }
}

async fn stream_oracle(mut socket: WebSocket, state: ApiState) {
    if let Ok(reports) = state.store.all_oracles().await {
        let payload = serde_json::json!({ "type": "oracle_snapshot", "reports": reports });
        if send_json(&mut socket, payload).await.is_err() { return; }
    }

    let mut rx = state.store.subscribe_oracles();
    let mut heartbeat = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {
            report = rx.recv() => {
                match report {
                    Ok(report) => {
                        let payload = serde_json::json!({ "type": "oracle_report", "report": report });
                        if send_json(&mut socket, payload).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        let payload = serde_json::json!({ "type": "lagged", "skipped": skipped });
                        if send_json(&mut socket, payload).await.is_err() { break; }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = heartbeat.tick() => {
                let payload = serde_json::json!({ "type": "heartbeat", "stream": "oracle", "ts": gravity_types::now_ms() });
                if send_json(&mut socket, payload).await.is_err() { break; }
            }
        }
    }
}

async fn send_json(socket: &mut WebSocket, payload: Value) -> Result<(), ()> {
    let body = serde_json::to_string(&payload).map_err(|_| ())?;
    socket.send(Message::Text(body.into())).await.map_err(|_| ())
}

#[derive(Clone, Debug, Deserialize)]
pub struct OrderDto {
    pub account: String,
    pub symbol: String,
    pub side: String,
    pub kind: Option<String>,
    pub tif: Option<String>,
    pub price: Option<String>,
    pub quantity: String,
    pub client_id: Option<String>,
}

impl TryFrom<OrderDto> for OrderRequest {
    type Error = GravityError;

    fn try_from(value: OrderDto) -> Result<Self, Self::Error> {
        validate_account(&value.account)?;
        if let Some(client_id) = &value.client_id { validate_client_id(client_id)?; }
        let side = parse_side(&value.side)?;
        let kind = parse_kind(value.kind.as_deref().unwrap_or(if value.price.is_some() { "limit" } else { "market" }))?;
        let tif = parse_tif(value.tif.as_deref().unwrap_or("gtc"))?;
        let price = match value.price {
            Some(price) => Some(Price::new(Fixed::from_str(&price)?)?),
            None => None,
        };
        let quantity = Quantity::new(Fixed::from_str(&value.quantity)?)?;
        Ok(Self {
            account: value.account,
            symbol: Symbol::new(value.symbol)?,
            side,
            kind,
            tif,
            price,
            quantity,
            client_id: value.client_id,
        })
    }
}

async fn submit_order(State(state): State<ApiState>, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let req = decode_order(body)?;
    let result = state.store.submit_order(req).await?;
    Ok(Json(serde_json::json!(result)))
}

async fn submit_orders(State(state): State<ApiState>, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let requests = decode_orders(body)?;
    let count = requests.len();
    let results = state.store.submit_orders(requests).await?;
    Ok(Json(serde_json::json!({ "count": count, "results": results })))
}


async fn submit_binary_order(State(state): State<ApiState>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let wire = decode_order_message(&body)?;
    let req = order_from_wire(wire)?;
    let result = state.store.submit_order(req).await?;
    Ok(Json(serde_json::json!({ "mode": "binary", "count": 1, "results": [result] })))
}

async fn submit_binary_orders(State(state): State<ApiState>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let wires = decode_order_batch(&body)?;
    let mut requests = Vec::with_capacity(wires.len());
    for wire in wires { requests.push(order_from_wire(wire)?); }
    let count = requests.len();
    let results = state.store.submit_orders(requests).await?;
    Ok(Json(serde_json::json!({ "mode": "binary", "count": count, "results": results })))
}

fn order_from_wire(value: OrderWire) -> Result<OrderRequest, GravityError> {
    validate_account(&value.account)?;
    if let Some(client_id) = &value.client_id { validate_client_id(client_id)?; }

    // Parse derived fixed-point values before moving owned fields out of the wire DTO.
    // This keeps the binary intake path allocation-light while avoiding partial-move borrows.
    let price = value.price()?;
    let quantity = value.quantity()?;

    Ok(OrderRequest {
        account: value.account,
        symbol: value.symbol,
        side: value.side,
        kind: value.kind,
        tif: value.tif,
        price,
        quantity,
        client_id: value.client_id,
    })
}

async fn cancel_order(State(state): State<ApiState>, Path((symbol, id)): Path<(String, String)>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol)?;
    let result = state.store.cancel_order(symbol, &id).await?;
    Ok(Json(serde_json::json!(result)))
}

#[derive(Clone, Debug, Deserialize)]
pub struct AmendDto {
    pub price: Option<String>,
    pub quantity: Option<String>,
    pub tif: Option<String>,
    pub client_id: Option<String>,
}

impl TryFrom<AmendDto> for AmendRequest {
    type Error = GravityError;

    fn try_from(value: AmendDto) -> Result<Self, Self::Error> {
        let price = match value.price { Some(price) => Some(Price::new(Fixed::from_str(&price)?)?), None => None };
        let quantity = match value.quantity { Some(quantity) => Some(Quantity::new(Fixed::from_str(&quantity)?)?), None => None };
        let tif = match value.tif { Some(tif) => Some(parse_tif(&tif)?), None => None };
        if let Some(client_id) = &value.client_id { validate_client_id(client_id)?; }
        Ok(Self { price, quantity, tif, client_id: value.client_id })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct StatusDto { pub status: String }

async fn amend_order(State(state): State<ApiState>, Path((symbol, id)): Path<(String, String)>, Json(body): Json<AmendDto>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol)?;
    let amend = AmendRequest::try_from(body)?;
    let result = state.store.amend_order(symbol, &id, amend).await?;
    Ok(Json(serde_json::json!(result)))
}

async fn replace_order(State(state): State<ApiState>, Path((symbol, id)): Path<(String, String)>, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol.clone())?;
    let mut req = decode_order(body)?;
    if req.symbol != symbol { return Err(GravityError::InvalidConfig("replacement symbol must match route symbol".into()).into()); }
    req.symbol = symbol.clone();
    let result = state.store.replace_order(symbol, &id, req).await?;
    Ok(Json(serde_json::json!(result)))
}

async fn set_market_status(State(state): State<ApiState>, Path(symbol): Path<String>, Json(body): Json<StatusDto>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(symbol)?;
    let status = parse_market_status(&body.status)?;
    let applied = state.store.set_market_status(symbol.clone(), status).await?;
    Ok(Json(serde_json::json!({ "symbol": symbol, "status": format!("{:?}", applied) })))
}

fn decode_order(body: Value) -> Result<OrderRequest, GravityError> {
    if let Ok(dto) = serde_json::from_value::<OrderDto>(body.clone()) {
        return dto.try_into();
    }
    serde_json::from_value::<OrderRequest>(body).map_err(GravityError::from)
}

fn decode_orders(body: Value) -> Result<Vec<OrderRequest>, GravityError> {
    if let Ok(dtos) = serde_json::from_value::<Vec<OrderDto>>(body.clone()) {
        return dtos.into_iter().map(TryInto::try_into).collect();
    }
    if let Some(orders) = body.get("orders") {
        if let Ok(dtos) = serde_json::from_value::<Vec<OrderDto>>(orders.clone()) {
            return dtos.into_iter().map(TryInto::try_into).collect();
        }
        return serde_json::from_value::<Vec<OrderRequest>>(orders.clone()).map_err(GravityError::from);
    }
    serde_json::from_value::<Vec<OrderRequest>>(body).map_err(GravityError::from)
}


fn validate_account(value: &str) -> Result<(), GravityError> {
    let trimmed = value.trim();
    if trimmed.len() < 3 || trimmed.len() > 96 {
        return Err(GravityError::InvalidConfig("account must be 3-96 characters".into()));
    }
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')) {
        return Err(GravityError::InvalidConfig("account contains unsupported characters".into()));
    }
    Ok(())
}

fn validate_client_id(value: &str) -> Result<(), GravityError> {
    if value.len() > 96 { return Err(GravityError::InvalidConfig("client_id must be <= 96 characters".into())); }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')) {
        return Err(GravityError::InvalidConfig("client_id contains unsupported characters".into()));
    }
    Ok(())
}

fn parse_side(value: &str) -> Result<Side, GravityError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "buy" | "bid" => Ok(Side::Buy),
        "sell" | "ask" => Ok(Side::Sell),
        other => Err(GravityError::InvalidConfig(format!("invalid side: {other}"))),
    }
}

fn parse_kind(value: &str) -> Result<OrderKind, GravityError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "limit" => Ok(OrderKind::Limit),
        "market" => Ok(OrderKind::Market),
        other => Err(GravityError::InvalidConfig(format!("invalid order kind: {other}"))),
    }
}

fn parse_tif(value: &str) -> Result<TimeInForce, GravityError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "gtc" => Ok(TimeInForce::Gtc),
        "ioc" => Ok(TimeInForce::Ioc),
        "fok" => Ok(TimeInForce::Fok),
        "post_only" | "postonly" => Ok(TimeInForce::PostOnly),
        other => Err(GravityError::InvalidConfig(format!("invalid time-in-force: {other}"))),
    }
}

fn parse_market_status(value: &str) -> Result<MarketStatus, GravityError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "open" => Ok(MarketStatus::Open),
        "cancel_only" | "cancelonly" => Ok(MarketStatus::CancelOnly),
        "halted" | "halt" => Ok(MarketStatus::Halted),
        other => Err(GravityError::InvalidConfig(format!("invalid market status: {other}"))),
    }
}


#[derive(Deserialize)]
struct CreatePoolDto {
    symbol: String,
    kind: Option<String>,
    fee_bps: Option<u32>,
    base_reserve: String,
    quote_reserve: String,
    min_liquidity: Option<String>,
    base_weight_bps: Option<u32>,
    quote_weight_bps: Option<u32>,
    amplification_bps: Option<u32>,
    max_price_impact_bps: Option<u32>,
}

#[derive(Deserialize)]
struct AmmAmountDto {
    side: String,
    amount_in: String,
    min_out: Option<String>,
}

#[derive(Deserialize)]
struct AddLiquidityDto {
    base: String,
    quote: String,
}

#[derive(Deserialize)]
struct RemoveLiquidityDto {
    lp: String,
    min_base: Option<String>,
    min_quote: Option<String>,
}

#[derive(Deserialize)]
struct AmmOracleGuardDto {
    oracle_price: String,
    max_deviation_bps: Option<u32>,
}

async fn amm_pools(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let pools = state.store.amm_pools().await?;
    Ok(Json(serde_json::json!({ "pools": pools })))
}

async fn amm_pool(State(state): State<ApiState>, Path(symbol): Path<String>) -> Result<Json<Value>, ApiError> {
    let pool = state.store.amm_pool(&symbol).await?;
    Ok(Json(serde_json::json!({ "pool": pool })))
}

async fn create_amm_pool(State(state): State<ApiState>, Json(dto): Json<CreatePoolDto>) -> Result<Json<Value>, ApiError> {
    let symbol = Symbol::new(dto.symbol)?;
    let kind = parse_pool_kind(dto.kind.as_deref().unwrap_or("constant-product"))?;
    let min_liquidity = parse_quantity(dto.min_liquidity.as_deref().unwrap_or("1"))?;
    let mut config = PoolConfig::normalized(symbol, kind, dto.fee_bps.unwrap_or(30), min_liquidity);
    if let Some(value) = dto.base_weight_bps { config.base_weight_bps = value; }
    if let Some(value) = dto.quote_weight_bps { config.quote_weight_bps = value; }
    if let Some(value) = dto.amplification_bps { config.amplification_bps = value; }
    if let Some(value) = dto.max_price_impact_bps { config.max_price_impact_bps = value; }
    let snapshot = state.store.create_amm_pool(config, parse_quantity(&dto.base_reserve)?, parse_quantity(&dto.quote_reserve)?).await?;
    Ok(Json(serde_json::json!({ "pool": snapshot })))
}

async fn amm_quote(State(state): State<ApiState>, Path(symbol): Path<String>, Json(dto): Json<AmmAmountDto>) -> Result<Json<Value>, ApiError> {
    let quote = state.store.amm_quote(&symbol, parse_swap_side(&dto.side)?, parse_quantity(&dto.amount_in)?).await?;
    Ok(Json(serde_json::json!({ "quote": quote })))
}

async fn amm_swap(State(state): State<ApiState>, Path(symbol): Path<String>, Json(dto): Json<AmmAmountDto>) -> Result<Json<Value>, ApiError> {
    let min_out = match dto.min_out.as_deref() { Some(value) => Some(parse_quantity(value)?), None => None };
    let result = state.store.amm_swap(&symbol, parse_swap_side(&dto.side)?, parse_quantity(&dto.amount_in)?, min_out).await?;
    Ok(Json(serde_json::json!({ "swap": result })))
}

async fn amm_add_liquidity(State(state): State<ApiState>, Path(symbol): Path<String>, Json(dto): Json<AddLiquidityDto>) -> Result<Json<Value>, ApiError> {
    let result = state.store.amm_add_liquidity(&symbol, parse_quantity(&dto.base)?, parse_quantity(&dto.quote)?).await?;
    Ok(Json(serde_json::json!({ "liquidity": result })))
}

async fn amm_remove_liquidity(State(state): State<ApiState>, Path(symbol): Path<String>, Json(dto): Json<RemoveLiquidityDto>) -> Result<Json<Value>, ApiError> {
    let min_base = match dto.min_base.as_deref() { Some(value) => Some(parse_quantity(value)?), None => None };
    let min_quote = match dto.min_quote.as_deref() { Some(value) => Some(parse_quantity(value)?), None => None };
    let result = state.store.amm_remove_liquidity(&symbol, parse_quantity(&dto.lp)?, min_base, min_quote).await?;
    Ok(Json(serde_json::json!({ "liquidity": result })))
}

async fn amm_oracle_guard(State(state): State<ApiState>, Path(symbol): Path<String>, Json(dto): Json<AmmOracleGuardDto>) -> Result<Json<Value>, ApiError> {
    let price = Price::new(Fixed::from_str(&dto.oracle_price)?)?;
    let result = state.store.amm_oracle_guard(&symbol, price, dto.max_deviation_bps.unwrap_or(250)).await?;
    Ok(Json(serde_json::json!({ "guard": result })))
}

async fn amm_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let events = state.store.recent_amm_events(query.limit.unwrap_or(100).min(1000)).await?;
    Ok(Json(serde_json::json!({ "events": events })))
}

fn parse_quantity(value: &str) -> Result<Quantity, GravityError> { Quantity::new(Fixed::from_str(value)?) }

fn parse_pool_kind(value: &str) -> Result<PoolKind, GravityError> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "constant-product" | "cp" => Ok(PoolKind::ConstantProduct),
        "stable" => Ok(PoolKind::Stable),
        "weighted" => Ok(PoolKind::Weighted),
        other => Err(GravityError::InvalidConfig(format!("unsupported AMM pool kind: {other}"))),
    }
}

fn parse_swap_side(value: &str) -> Result<SwapSide, GravityError> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "base-in" | "base" => Ok(SwapSide::BaseIn),
        "quote-in" | "quote" => Ok(SwapSide::QuoteIn),
        other => Err(GravityError::InvalidConfig(format!("unsupported AMM swap side: {other}"))),
    }
}

#[derive(Debug)]
pub struct ApiError(GravityError);

impl From<GravityError> for ApiError {
    fn from(value: GravityError) -> Self { Self(value) }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            GravityError::NotFound(_) => StatusCode::NOT_FOUND,
            GravityError::InvalidConfig(_) | GravityError::InvalidSymbol(_) | GravityError::InvalidNumber(_) | GravityError::InvalidPrice | GravityError::InvalidQuantity => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(error_json(&self.0))).into_response()
    }
}


#[derive(Clone, Debug, Deserialize)]
pub struct RiskCollateralDto {
    pub asset: String,
    pub quantity: String,
    pub price: String,
    pub collateral_factor_bps: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RiskPositionDto {
    pub symbol: String,
    pub quantity: String,
    pub mark_price: String,
    pub side: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RiskCheckDto {
    pub account: String,
    pub collaterals: Vec<RiskCollateralDto>,
    pub positions: Vec<RiskPositionDto>,
    pub debt_value: Option<String>,
    pub maintenance_margin_bps: Option<u32>,
    pub initial_margin_bps: Option<u32>,
}

impl TryFrom<RiskCheckDto> for AccountRiskInput {
    type Error = GravityError;

    fn try_from(value: RiskCheckDto) -> Result<Self, Self::Error> {
        validate_account(&value.account)?;
        let mut collaterals = Vec::with_capacity(value.collaterals.len());
        for item in value.collaterals {
            collaterals.push(CollateralInput {
                asset: item.asset,
                quantity: Quantity::new(Fixed::from_str(&item.quantity)?)?,
                price: Price::new(Fixed::from_str(&item.price)?)?,
                collateral_factor_bps: item.collateral_factor_bps.unwrap_or(8_500),
            });
        }
        let mut positions = Vec::with_capacity(value.positions.len());
        for item in value.positions {
            positions.push(PositionInput {
                symbol: Symbol::new(item.symbol)?,
                quantity: Quantity::new(Fixed::from_str(&item.quantity)?)?,
                mark_price: Price::new(Fixed::from_str(&item.mark_price)?)?,
                side: item.side.unwrap_or_else(|| "long".into()),
            });
        }
        let debt_value = match value.debt_value { Some(v) => Fixed::from_str(&v)?, None => Fixed::ZERO };
        Ok(AccountRiskInput {
            account: value.account,
            collaterals,
            positions,
            debt_value,
            maintenance_margin_bps: value.maintenance_margin_bps.unwrap_or(1_000),
            initial_margin_bps: value.initial_margin_bps.unwrap_or(2_000),
            timestamp_ms: None,
        })
    }
}

async fn risk_check(State(state): State<ApiState>, Json(body): Json<RiskCheckDto>) -> Result<Json<Value>, ApiError> {
    let input = AccountRiskInput::try_from(body)?;
    let snapshot = state.store.risk_check(input).await?;
    Ok(Json(serde_json::json!(snapshot)))
}

async fn risk_account(State(state): State<ApiState>, Path(account): Path<String>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.risk_account(&account).await?;
    Ok(Json(serde_json::json!({ "account": account, "snapshot": snapshot })))
}

async fn risk_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let events = state.store.risk_events(limit).await?;
    Ok(Json(serde_json::json!({ "events": events })))
}

async fn risk_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!(state.store.risk_stats().await?)))
}



#[derive(Clone, Debug, Deserialize)]
pub struct LiquidationScanDto { pub limit: Option<usize> }

#[derive(Clone, Debug, Deserialize)]
pub struct LiquidationPlanDto { pub mode: Option<String> }

fn parse_liquidation_mode(value: Option<&str>) -> Result<LiquidationMode, GravityError> {
    match value.unwrap_or("partial").to_ascii_lowercase().as_str() {
        "partial" => Ok(LiquidationMode::Partial),
        "full" => Ok(LiquidationMode::Full),
        other => Err(GravityError::InvalidConfig(format!("unsupported liquidation mode: {other}"))),
    }
}

async fn liquidation_scan(State(state): State<ApiState>, body: Option<Json<LiquidationScanDto>>) -> Result<Json<Value>, ApiError> {
    let limit = body.map(|Json(v)| v.limit.unwrap_or(100)).unwrap_or(100).min(10_000);
    let candidates = state.store.liquidation_scan(limit).await?;
    Ok(Json(serde_json::json!({ "candidates": candidates, "limit": limit })))
}

async fn liquidation_candidates(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let candidates = state.store.liquidation_candidates(limit).await?;
    Ok(Json(serde_json::json!({ "candidates": candidates, "limit": limit })))
}

async fn liquidation_plan(State(state): State<ApiState>, Path(account): Path<String>, body: Option<Json<LiquidationPlanDto>>) -> Result<Json<Value>, ApiError> {
    let mode = parse_liquidation_mode(body.as_ref().and_then(|Json(v)| v.mode.as_deref()))?;
    let plan = state.store.liquidation_plan(&account, mode).await?;
    Ok(Json(serde_json::json!({ "account": account, "plan": plan })))
}

async fn liquidation_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let events = state.store.liquidation_events(limit).await?;
    Ok(Json(serde_json::json!({ "events": events, "limit": limit })))
}

async fn liquidation_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!(state.store.liquidation_stats().await?)))
}


#[derive(Clone, Debug, Deserialize)]
pub struct PerpMarketDto {
    pub symbol: String,
    pub index_symbol: Option<String>,
    pub initial_margin_bps: Option<u32>,
    pub maintenance_margin_bps: Option<u32>,
    pub max_leverage_bps: Option<u32>,
    pub funding_interval_ms: Option<u64>,
    pub maker_fee_bps: Option<i64>,
    pub taker_fee_bps: Option<i64>,
    pub insurance_fund_bps: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PerpOpenDto {
    pub account: String,
    pub symbol: String,
    pub side: String,
    pub quantity: String,
    pub entry_price: String,
    pub collateral: String,
    pub leverage_bps: Option<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PerpFundingDto {
    pub symbol: String,
    pub index_price: String,
    pub mark_price: String,
    pub funding_rate_bps: Option<i64>,
}

fn config_from_perp_dto(dto: PerpMarketDto) -> Result<PerpMarketConfig, GravityError> {
    let symbol = Symbol::new(dto.symbol)?;
    let index_symbol = Symbol::new(dto.index_symbol.unwrap_or_else(|| symbol.0.clone()))?;
    let mut config = PerpMarketConfig::new(symbol, index_symbol);
    if let Some(value) = dto.initial_margin_bps { config.initial_margin_bps = value; }
    if let Some(value) = dto.maintenance_margin_bps { config.maintenance_margin_bps = value; }
    if let Some(value) = dto.max_leverage_bps { config.max_leverage_bps = value; }
    if let Some(value) = dto.funding_interval_ms { config.funding_interval_ms = value; }
    if let Some(value) = dto.maker_fee_bps { config.maker_fee_bps = value; }
    if let Some(value) = dto.taker_fee_bps { config.taker_fee_bps = value; }
    if let Some(value) = dto.insurance_fund_bps { config.insurance_fund_bps = value; }
    Ok(config)
}

async fn create_perp_market(State(state): State<ApiState>, Json(dto): Json<PerpMarketDto>) -> Result<Json<Value>, ApiError> {
    let snapshot = state.store.create_perp_market(config_from_perp_dto(dto)?).await?;
    Ok(Json(serde_json::json!({ "market": snapshot })))
}

async fn perp_markets(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let markets = state.store.perp_markets().await?;
    Ok(Json(serde_json::json!({ "markets": markets })))
}

async fn perp_market(State(state): State<ApiState>, Path(symbol): Path<String>) -> Result<Json<Value>, ApiError> {
    let market = state.store.perp_market(&symbol).await?;
    Ok(Json(serde_json::json!({ "market": market })))
}

async fn open_perp_position(State(state): State<ApiState>, Json(dto): Json<PerpOpenDto>) -> Result<Json<Value>, ApiError> {
    validate_account(&dto.account)?;
    let request = PerpPositionRequest {
        account: dto.account,
        symbol: Symbol::new(dto.symbol)?,
        side: PerpSide::parse(&dto.side)?,
        quantity: parse_quantity(&dto.quantity)?,
        entry_price: Price::new(Fixed::from_str(&dto.entry_price)?)?,
        collateral: Fixed::from_str(&dto.collateral)?,
        leverage_bps: dto.leverage_bps.unwrap_or(10_000),
    };
    let position = state.store.open_perp_position(request).await?;
    Ok(Json(serde_json::json!({ "position": position })))
}

async fn update_perp_funding(State(state): State<ApiState>, Json(dto): Json<PerpFundingDto>) -> Result<Json<Value>, ApiError> {
    let request = FundingUpdateRequest {
        symbol: Symbol::new(dto.symbol)?,
        index_price: Price::new(Fixed::from_str(&dto.index_price)?)?,
        mark_price: Price::new(Fixed::from_str(&dto.mark_price)?)?,
        funding_rate_bps: dto.funding_rate_bps.unwrap_or(0),
    };
    let market = state.store.update_perp_funding(request).await?;
    Ok(Json(serde_json::json!({ "market": market })))
}

async fn perp_positions(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let positions = state.store.perp_positions(None, limit).await?;
    Ok(Json(serde_json::json!({ "positions": positions, "limit": limit })))
}

async fn perp_account_positions(State(state): State<ApiState>, Path(account): Path<String>) -> Result<Json<Value>, ApiError> {
    let positions = state.store.perp_positions(Some(&account), 10_000).await?;
    Ok(Json(serde_json::json!({ "account": account, "positions": positions })))
}

async fn perp_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let events = state.store.perp_events(limit).await?;
    Ok(Json(serde_json::json!({ "events": events, "limit": limit })))
}

async fn perp_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!(state.store.perp_stats().await?)))
}


#[derive(Clone, Debug, Deserialize)]
pub struct IndexAssetDto {
    pub symbol: String,
    pub target_weight_bps: u32,
    pub oracle_price: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IndexProductDto {
    pub id: String,
    pub name: String,
    pub quote_asset: Option<String>,
    pub management_fee_bps: Option<u32>,
    pub rebalance_threshold_bps: Option<u32>,
    pub min_mint_notional: Option<String>,
    pub seed_notional: Option<String>,
    pub assets: Vec<IndexAssetDto>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IndexAccountNotionalDto {
    pub account: String,
    pub notional: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct IndexRedeemDto {
    pub account: String,
    pub units: String,
}

fn config_from_index_dto(dto: IndexProductDto) -> Result<(IndexProductConfig, Fixed), GravityError> {
    let mut assets = Vec::with_capacity(dto.assets.len());
    for asset in dto.assets {
        assets.push(IndexAsset {
            symbol: Symbol::new(asset.symbol)?,
            target_weight_bps: asset.target_weight_bps,
            oracle_price: Price::new(Fixed::from_str(&asset.oracle_price)?)?,
        });
    }
    let config = IndexProductConfig {
        id: dto.id,
        name: dto.name,
        quote_asset: dto.quote_asset.unwrap_or_else(|| "USDx".into()),
        management_fee_bps: dto.management_fee_bps.unwrap_or(25),
        rebalance_threshold_bps: dto.rebalance_threshold_bps.unwrap_or(500),
        min_mint_notional: Fixed::from_str(dto.min_mint_notional.as_deref().unwrap_or("100"))?,
        assets,
    };
    let seed = Fixed::from_str(dto.seed_notional.as_deref().unwrap_or("1000000"))?;
    Ok((config, seed))
}

async fn create_index_product(State(state): State<ApiState>, Json(dto): Json<IndexProductDto>) -> Result<Json<Value>, ApiError> {
    let (config, seed) = config_from_index_dto(dto)?;
    let product = state.store.create_index_product(config, seed).await?;
    Ok(Json(serde_json::json!({ "product": product })))
}

async fn index_products(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    let products = state.store.index_products().await?;
    Ok(Json(serde_json::json!({ "products": products })))
}

async fn index_product(State(state): State<ApiState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let product = state.store.index_product(&id).await?;
    Ok(Json(serde_json::json!({ "product": product })))
}

async fn index_nav(State(state): State<ApiState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let report = state.store.index_nav(&id).await?;
    Ok(Json(serde_json::json!({ "nav": report })))
}

async fn index_rebalance(State(state): State<ApiState>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let plan = state.store.index_rebalance(&id).await?;
    Ok(Json(serde_json::json!({ "plan": plan })))
}

async fn index_mint(State(state): State<ApiState>, Path(id): Path<String>, Json(dto): Json<IndexAccountNotionalDto>) -> Result<Json<Value>, ApiError> {
    validate_account(&dto.account)?;
    let plan = state.store.index_mint_plan(&id, dto.account, Fixed::from_str(&dto.notional)?).await?;
    Ok(Json(serde_json::json!({ "plan": plan })))
}

async fn index_redeem(State(state): State<ApiState>, Path(id): Path<String>, Json(dto): Json<IndexRedeemDto>) -> Result<Json<Value>, ApiError> {
    validate_account(&dto.account)?;
    let plan = state.store.index_redeem_plan(&id, dto.account, parse_quantity(&dto.units)?).await?;
    Ok(Json(serde_json::json!({ "plan": plan })))
}

async fn index_events(State(state): State<ApiState>, Query(query): Query<DepthQuery>) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(100).min(10_000);
    let events = state.store.index_events(limit).await?;
    Ok(Json(serde_json::json!({ "events": events, "limit": limit })))
}

async fn index_stats(State(state): State<ApiState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::json!(state.store.index_stats().await?)))
}
