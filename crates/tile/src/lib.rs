//! Tile runtime scaffolding for Gravity.
//!
//! The tile crate is intentionally small and safe by default. It provides the
//! runtime controls needed to evolve Gravity into a hardware-aware, tile-based
//! DeFi engine without moving correctness-critical matching logic into unsafe
//! code paths. Cranelift support is feature-gated behind `cranelift-jit` so the
//! default build remains lightweight.

use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, TrySendError};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Logical tile role inside Gravity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TileKind {
    /// External/user/API ingress.
    Ingress,
    /// Binary/JSON order decoding and validation.
    Intake,
    /// Order routing into market-specific workers.
    OrderRoute,
    /// Per-market CLOB execution.
    Match,
    /// Oracle aggregation and report signing.
    Oracle,
    /// AMM quote/swap planning.
    Amm,
    /// Risk checks and circuit breakers.
    Risk,
    /// Liquidation candidate scanning.
    Liquidation,
    /// Settlement compression and submission.
    Settlement,
    /// Perpetual futures calculations.
    Perps,
    /// Index fund NAV/rebalance calculations.
    Index,
    /// Persistence queue flushing.
    Storage,
    /// WebSocket/API stream fanout.
    Stream,
    /// Audit and replay metadata.
    Audit,
    /// Metrics collection.
    Metrics,
    /// Benchmark/profiling worker.
    Bench,
}

impl Default for TileKind {
    fn default() -> Self { Self::Bench }
}

impl fmt::Display for TileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Ingress => "ingress",
            Self::Intake => "intake",
            Self::OrderRoute => "order-route",
            Self::Match => "match",
            Self::Oracle => "oracle",
            Self::Amm => "amm",
            Self::Risk => "risk",
            Self::Liquidation => "liquidation",
            Self::Settlement => "settlement",
            Self::Perps => "perps",
            Self::Index => "index",
            Self::Storage => "storage",
            Self::Stream => "stream",
            Self::Audit => "audit",
            Self::Metrics => "metrics",
            Self::Bench => "bench",
        };
        f.write_str(name)
    }
}

/// JIT execution mode for future hot math/risk kernels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JitMode {
    /// JIT disabled; pure Rust path only.
    Off,
    /// JIT objects may be prepared but not installed on the hot path.
    Warm,
    /// JIT kernels can be used for eligible deterministic kernels.
    Hot,
}

/// Tile auto-tuning mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TuneMode {
    /// Static config only.
    Fixed,
    /// Adjust batch sizes and spin/sleep policy based on queue pressure.
    Adaptive,
}

/// Health state for a tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TileHealth {
    /// Tile is accepting work and pressure is safe.
    Healthy,
    /// Queue pressure or rejections suggest the tile needs attention.
    Degraded,
    /// Tile is overloaded or not processing accepted commands.
    Unhealthy,
}

impl Default for TileHealth {
    fn default() -> Self { Self::Healthy }
}

/// Tile runtime configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TileConfig {
    /// Human-readable tile name.
    pub name: String,
    /// Tile role.
    pub kind: TileKind,
    /// Bounded queue capacity.
    pub capacity: usize,
    /// Maximum commands drained per loop.
    pub batch: usize,
    /// Optional CPU core id for affinity.
    pub core: Option<usize>,
    /// Whether the worker should busy-spin briefly before sleeping.
    pub spin: bool,
    /// JIT mode for future deterministic kernels.
    pub jit: JitMode,
    /// Auto-tuning policy.
    pub tune: TuneMode,
}

impl TileConfig {
    /// Build a sane config for a tile.
    pub fn new(name: impl Into<String>, kind: TileKind) -> Self {
        Self { name: name.into(), kind, capacity: 65_536, batch: 1024, core: None, spin: false, jit: JitMode::Off, tune: TuneMode::Adaptive }
    }

    /// Pin this tile to a logical core if supported by the OS/runtime.
    #[must_use]
    pub fn pinned(mut self, core: usize) -> Self {
        self.core = Some(core);
        self
    }

    /// Set queue capacity.
    #[must_use]
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity.max(1);
        self
    }

    /// Set max drain batch.
    #[must_use]
    pub fn batch(mut self, batch: usize) -> Self {
        self.batch = batch.max(1);
        self
    }
}

/// Runtime command passed through tile channels.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TileCommand {
    /// No-op marker used by benchmarks and health tests.
    Ping(u64),
    /// Binary frame for future internal hot-path routing.
    Frame(Vec<u8>),
    /// Ask a tile to stop cleanly.
    Stop,
}

/// Lightweight tile response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TileReply {
    /// No-op acknowledgement.
    Pong(u64),
    /// Command was accepted and processed.
    Ack,
    /// Tile stopped.
    Stopped,
}

/// Snapshot of tile counters.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TileStats {
    /// Tile name.
    pub name: String,
    /// Tile role.
    pub kind: TileKind,
    /// Commands accepted by senders.
    pub accepted: u64,
    /// Commands rejected due to full queues.
    pub rejected: u64,
    /// Commands processed by worker loop.
    pub processed: u64,
    /// Drain loop iterations.
    pub loops: u64,
    /// Queue capacity.
    pub capacity: usize,
    /// Last observed queue depth.
    pub depth: usize,
    /// Current max batch size.
    pub batch: usize,
    /// Queue pressure in basis points.
    pub pressure_bps: u64,
    /// Tile health verdict.
    pub health: TileHealth,
    /// Average command latency in nanoseconds.
    pub avg_ns: u64,
    /// Max observed command latency in nanoseconds.
    pub max_ns: u64,
    /// Whether CPU affinity was successfully applied.
    pub pinned: bool,
}

#[derive(Default)]
struct TileCounters {
    accepted: AtomicU64,
    rejected: AtomicU64,
    processed: AtomicU64,
    loops: AtomicU64,
    total_ns: AtomicU64,
    max_ns: AtomicU64,
    pinned: AtomicBool,
}

/// Sender side for tile commands.
#[derive(Clone)]
pub struct TileHandle {
    config: TileConfig,
    tx: Sender<TilePacket>,
    counters: Arc<TileCounters>,
}

impl TileHandle {
    /// Try to enqueue a command without blocking.
    pub fn try_send(&self, command: TileCommand) -> Result<(), TileSendError> {
        let packet = TilePacket { command, created: Instant::now() };
        match self.tx.try_send(packet) {
            Ok(()) => {
                self.counters.accepted.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                Err(TileSendError::Full)
            }
            Err(TrySendError::Disconnected(_)) => Err(TileSendError::Closed),
        }
    }

    /// Current stats for this tile.
    #[must_use]
    pub fn stats(&self) -> TileStats {
        let processed = self.counters.processed.load(Ordering::Relaxed);
        let total_ns = self.counters.total_ns.load(Ordering::Relaxed);
        let depth = self.tx.len();
        let pressure_bps = if self.config.capacity > 0 { (depth as u64).saturating_mul(10_000) / self.config.capacity as u64 } else { 0 };
        let rejected = self.counters.rejected.load(Ordering::Relaxed);
        let accepted = self.counters.accepted.load(Ordering::Relaxed);
        let health = if pressure_bps >= 9_000 || (accepted > 0 && processed == 0) { TileHealth::Unhealthy }
            else if pressure_bps >= 7_500 || rejected > 0 { TileHealth::Degraded }
            else { TileHealth::Healthy };
        TileStats {
            name: self.config.name.clone(),
            kind: self.config.kind,
            accepted,
            rejected,
            processed,
            loops: self.counters.loops.load(Ordering::Relaxed),
            capacity: self.config.capacity,
            depth,
            batch: self.config.batch,
            pressure_bps,
            health,
            avg_ns: if processed > 0 { total_ns / processed } else { 0 },
            max_ns: self.counters.max_ns.load(Ordering::Relaxed),
            pinned: self.counters.pinned.load(Ordering::Relaxed),
        }
    }

    /// Tile name.
    #[must_use]
    pub fn name(&self) -> &str { &self.config.name }
}

/// Tile enqueue error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TileSendError {
    /// Queue is full.
    Full,
    /// Worker has stopped.
    Closed,
}

struct TilePacket {
    command: TileCommand,
    created: Instant,
}

/// Running tile worker.
pub struct TileWorker {
    /// Sender handle.
    pub handle: TileHandle,
    join: Option<JoinHandle<()>>,
}

impl TileWorker {
    /// Start a tile with a simple command handler.
    pub fn spawn(config: TileConfig) -> Self {
        let (tx, rx) = bounded::<TilePacket>(config.capacity);
        let counters = Arc::new(TileCounters::default());
        let handle = TileHandle { config: config.clone(), tx, counters: Arc::clone(&counters) };
        let worker_config = config.clone();
        let join = thread::Builder::new()
            .name(format!("gravity-tile-{}", config.name))
            .spawn(move || run_tile(worker_config, rx, counters))
            .expect("tile worker thread should start");
        Self { handle, join: Some(join) }
    }

    /// Stop and join the worker.
    pub fn stop(mut self) {
        let _ = self.handle.tx.send(TilePacket { command: TileCommand::Stop, created: Instant::now() });
        if let Some(join) = self.join.take() { let _ = join.join(); }
    }
}

fn run_tile(mut config: TileConfig, rx: Receiver<TilePacket>, counters: Arc<TileCounters>) {
    if let Some(core) = config.core {
        if pin_to_core(core) { counters.pinned.store(true, Ordering::Relaxed); }
    }
    let tuner = AutoTuner::new(config.batch);
    let mut buffer = Vec::with_capacity(config.batch);
    loop {
        counters.loops.fetch_add(1, Ordering::Relaxed);
        match rx.recv_timeout(Duration::from_millis(if config.spin { 0 } else { 1 })) {
            Ok(packet) => buffer.push(packet),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
        drain_available(&rx, &mut buffer, config.batch.saturating_sub(1));
        let stop = process_batch(&buffer, &counters);
        buffer.clear();
        if config.tune == TuneMode::Adaptive {
            config.batch = tuner.tune(config.batch, rx.len(), config.capacity);
            if buffer.capacity() < config.batch { buffer.reserve(config.batch - buffer.capacity()); }
        }
        if stop { break; }
    }
}

fn drain_available(rx: &Receiver<TilePacket>, buffer: &mut Vec<TilePacket>, limit: usize) {
    for _ in 0..limit {
        match rx.try_recv() {
            Ok(packet) => buffer.push(packet),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }
}

fn process_batch(buffer: &[TilePacket], counters: &TileCounters) -> bool {
    let mut stop = false;
    for packet in buffer {
        let ns = packet.created.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        counters.total_ns.fetch_add(ns, Ordering::Relaxed);
        max_atomic(&counters.max_ns, ns);
        counters.processed.fetch_add(1, Ordering::Relaxed);
        match &packet.command {
            TileCommand::Stop => {
                stop = true;
            }
            TileCommand::Ping(v) => {
                std::hint::black_box(v);
            }
            TileCommand::Frame(bytes) => {
                std::hint::black_box(bytes.len());
            }
        }
    }
    stop
}

fn max_atomic(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => current = next,
        }
    }
}

fn pin_to_core(core: usize) -> bool {
    let Some(ids) = core_affinity::get_core_ids() else { return false; };
    let Some(id) = ids.into_iter().find(|id| id.id == core) else { return false; };
    core_affinity::set_for_current(id);
    true
}

/// Simple adaptive tile tuner.
pub struct AutoTuner {
    min_batch: usize,
    max_batch: usize,
}

impl AutoTuner {
    /// Create a tuner around an initial batch size.
    #[must_use]
    pub fn new(initial_batch: usize) -> Self {
        Self { min_batch: 32.min(initial_batch.max(1)), max_batch: initial_batch.saturating_mul(8).clamp(64, 65_536) }
    }

    /// Tune batch size using queue pressure.
    #[must_use]
    pub fn tune(&self, current: usize, depth: usize, capacity: usize) -> usize {
        if capacity == 0 { return current.max(1); }
        let pressure_bps = depth.saturating_mul(10_000) / capacity;
        if pressure_bps > 7_500 { current.saturating_mul(2).min(self.max_batch) }
        else if pressure_bps < 1_000 { current.saturating_div(2).max(self.min_batch) }
        else { current }
    }
}


/// Aggregate runtime stats for the tile supervisor.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TileRuntimeStats {
    /// Number of configured tiles.
    pub tiles: usize,
    /// Healthy tile count.
    pub healthy: usize,
    /// Degraded tile count.
    pub degraded: usize,
    /// Unhealthy tile count.
    pub unhealthy: usize,
    /// Total accepted commands across tiles.
    pub accepted: u64,
    /// Total rejected commands across tiles.
    pub rejected: u64,
    /// Total processed commands across tiles.
    pub processed: u64,
    /// Average queue pressure in basis points.
    pub avg_pressure_bps: u64,
    /// Worst queue pressure in basis points.
    pub max_pressure_bps: u64,
    /// Worst observed latency in nanoseconds.
    pub max_ns: u64,
}

/// Snapshot returned by tile supervisor APIs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TileRuntimeSnapshot {
    /// Aggregate runtime stats.
    pub stats: TileRuntimeStats,
    /// Per-tile stats.
    pub tiles: Vec<TileStats>,
}

/// Lightweight tile supervisor that owns a group of worker tiles.
#[derive(Clone)]
pub struct TileSupervisor {
    configs: Arc<Vec<TileConfig>>,
    workers: Arc<std::sync::Mutex<Vec<TileWorker>>>,
}

impl TileSupervisor {
    /// Build a supervisor from tile configs and start all workers immediately.
    #[must_use]
    pub fn start(configs: Vec<TileConfig>) -> Self {
        let workers = configs.iter().cloned().map(TileWorker::spawn).collect::<Vec<_>>();
        Self { configs: Arc::new(configs), workers: Arc::new(std::sync::Mutex::new(workers)) }
    }

    /// Build the default Gravity tile layout.
    #[must_use]
    pub fn default_runtime() -> Self {
        let kinds = [
            ("ingress", TileKind::Ingress),
            ("decode", TileKind::Intake),
            ("route", TileKind::OrderRoute),
            ("match", TileKind::Match),
            ("oracle", TileKind::Oracle),
            ("amm", TileKind::Amm),
            ("risk", TileKind::Risk),
            ("liquidation", TileKind::Liquidation),
            ("settlement", TileKind::Settlement),
            ("storage", TileKind::Storage),
            ("stream", TileKind::Stream),
            ("metrics", TileKind::Metrics),
        ];
        let configs = kinds.into_iter().enumerate().map(|(i, (name, kind))| {
            TileConfig::new(name, kind).capacity(65_536).batch(1024).pinned(i)
        }).collect();
        Self::start(configs)
    }

    /// Return a complete snapshot of supervisor and tile state.
    #[must_use]
    pub fn snapshot(&self) -> TileRuntimeSnapshot {
        let tiles = self.workers.lock().map(|workers| workers.iter().map(|worker| worker.handle.stats()).collect::<Vec<_>>()).unwrap_or_default();
        let mut stats = TileRuntimeStats { tiles: tiles.len(), ..TileRuntimeStats::default() };
        let mut pressure_sum = 0_u64;
        for tile in &tiles {
            stats.accepted = stats.accepted.saturating_add(tile.accepted);
            stats.rejected = stats.rejected.saturating_add(tile.rejected);
            stats.processed = stats.processed.saturating_add(tile.processed);
            stats.max_pressure_bps = stats.max_pressure_bps.max(tile.pressure_bps);
            stats.max_ns = stats.max_ns.max(tile.max_ns);
            pressure_sum = pressure_sum.saturating_add(tile.pressure_bps);
            match tile.health {
                TileHealth::Healthy => stats.healthy += 1,
                TileHealth::Degraded => stats.degraded += 1,
                TileHealth::Unhealthy => stats.unhealthy += 1,
            }
        }
        stats.avg_pressure_bps = if stats.tiles > 0 { pressure_sum / stats.tiles as u64 } else { 0 };
        TileRuntimeSnapshot { stats, tiles }
    }

    /// Try to send synthetic health pings to every tile.
    pub fn ping_all(&self, sequence: u64) -> usize {
        self.workers.lock().map(|workers| {
            workers.iter().enumerate().filter(|(i, worker)| worker.handle.try_send(TileCommand::Ping(sequence.saturating_add(*i as u64))).is_ok()).count()
        }).unwrap_or(0)
    }

    /// Stop all tiles.
    pub fn stop_all(&self) -> usize {
        let mut stopped = 0;
        if let Ok(mut workers) = self.workers.lock() {
            for worker in workers.drain(..) {
                worker.stop();
                stopped += 1;
            }
        }
        stopped
    }

    /// Restart all tiles from their original configs. This is a controlled supervisor-level reset.
    pub fn restart_all(&self) -> usize {
        let mut restarted = 0;
        if let Ok(mut workers) = self.workers.lock() {
            for worker in workers.drain(..) { worker.stop(); }
            for config in self.configs.iter().cloned() {
                workers.push(TileWorker::spawn(config));
                restarted += 1;
            }
        }
        restarted
    }
}

impl Default for TileSupervisor {
    fn default() -> Self { Self::default_runtime() }
}

/// Safe deterministic kernel families eligible for JIT acceleration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum KernelKind {
    /// Quote/maker/taker fee calculation in basis points.
    FeeBps,
    /// Initial/maintenance margin requirement calculation.
    MarginRequirement,
    /// Health factor calculation in basis points.
    HealthBps,
    /// Settlement net delta accumulation helper.
    NetDelta,
    /// Constant-product AMM quote helper.
    AmmQuote,
    /// Index fund NAV weighted-sum helper.
    IndexNav,
}

impl fmt::Display for KernelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::FeeBps => "fee-bps",
            Self::MarginRequirement => "margin-requirement",
            Self::HealthBps => "health-bps",
            Self::NetDelta => "net-delta",
            Self::AmmQuote => "amm-quote",
            Self::IndexNav => "index-nav",
        };
        f.write_str(name)
    }
}

/// Fixed-width input used by safe deterministic JIT/native kernels.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct KernelInput {
    /// First numeric operand.
    pub a: i128,
    /// Second numeric operand.
    pub b: i128,
    /// Third numeric operand.
    pub c: i128,
    /// Basis-point operand where applicable.
    pub bps: i128,
}

impl KernelInput {
    /// Build a new input.
    #[must_use]
    pub fn new(a: i128, b: i128, c: i128, bps: i128) -> Self { Self { a, b, c, bps } }
}

/// Kernel execution result.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct KernelOutput {
    /// Deterministic integer result.
    pub value: i128,
}

/// Result of checking a native fallback against the active JIT mode.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KernelCheck {
    /// Kernel family.
    pub kind: KernelKind,
    /// Native/fallback output.
    pub native: KernelOutput,
    /// JIT output, or deterministic placeholder when JIT is disabled.
    pub accelerated: KernelOutput,
    /// Whether outputs matched exactly.
    pub equivalent: bool,
    /// Active mode.
    pub mode: JitMode,
}

/// JIT kernel descriptor used before native code is enabled on hot paths.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JitKernel {
    /// Kernel name.
    pub name: String,
    /// What this kernel accelerates.
    pub purpose: String,
    /// Kernel family.
    pub kind: KernelKind,
    /// Whether it is deterministic and safe for execution use.
    pub deterministic: bool,
    /// Whether native fallback must be retained.
    pub fallback_required: bool,
    /// Current execution mode.
    pub mode: JitMode,
}

impl JitKernel {
    /// Create a safe deterministic kernel descriptor.
    #[must_use]
    pub fn new(kind: KernelKind, purpose: impl Into<String>) -> Self {
        Self {
            name: kind.to_string(),
            purpose: purpose.into(),
            kind,
            deterministic: true,
            fallback_required: true,
            mode: JitMode::Warm,
        }
    }
}

/// Aggregate JIT registry stats.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct JitRegistryStats {
    /// Number of registered kernels.
    pub kernels: usize,
    /// Kernels that are deterministic.
    pub deterministic: usize,
    /// Kernels requiring native fallback.
    pub fallback_required: usize,
    /// Whether Cranelift support is compiled in.
    pub cranelift_available: bool,
}

/// Cranelift JIT scaffold. Default builds keep this as a safe registry stub.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct JitRegistry {
    kernels: Vec<JitKernel>,
}

impl JitRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self { Self { kernels: Vec::new() } }

    /// Create a registry containing Gravity's default deterministic hot kernels.
    #[must_use]
    pub fn with_default_kernels() -> Self {
        let mut registry = Self::new();
        registry.register(JitKernel::new(KernelKind::FeeBps, "maker/taker fee calculation"));
        registry.register(JitKernel::new(KernelKind::MarginRequirement, "margin requirement calculation"));
        registry.register(JitKernel::new(KernelKind::HealthBps, "account health basis-point calculation"));
        registry.register(JitKernel::new(KernelKind::NetDelta, "settlement net delta helper"));
        registry.register(JitKernel::new(KernelKind::AmmQuote, "constant-product AMM quote helper"));
        registry.register(JitKernel::new(KernelKind::IndexNav, "index fund NAV weighted-sum helper"));
        registry
    }

    /// Register a deterministic kernel descriptor.
    pub fn register(&mut self, kernel: JitKernel) { self.kernels.push(kernel); }

    /// Number of registered kernel descriptors.
    #[must_use]
    pub fn len(&self) -> usize { self.kernels.len() }

    /// Whether no kernels are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.kernels.is_empty() }

    /// Registered kernel descriptors.
    #[must_use]
    pub fn kernels(&self) -> &[JitKernel] { &self.kernels }

    /// Registry stats for APIs and benchmark reports.
    #[must_use]
    pub fn stats(&self) -> JitRegistryStats {
        JitRegistryStats {
            kernels: self.kernels.len(),
            deterministic: self.kernels.iter().filter(|k| k.deterministic).count(),
            fallback_required: self.kernels.iter().filter(|k| k.fallback_required).count(),
            cranelift_available: cranelift_available(),
        }
    }

    /// Execute the native fallback for a kernel.
    #[must_use]
    pub fn execute_native(kind: KernelKind, input: KernelInput) -> KernelOutput { KernelOutput { value: execute_kernel(kind, input) } }

    /// Execute a kernel through the safe checked path.
    ///
    /// For v3.0.0, Cranelift is intentionally not installed into the critical CLOB
    /// ordering path. The accelerated branch mirrors native output and is checked for
    /// exact equivalence. Future releases can replace the accelerated branch with
    /// compiled Cranelift functions one kernel at a time while preserving this check.
    #[must_use]
    pub fn execute_checked(&self, kind: KernelKind, input: KernelInput, mode: JitMode) -> KernelCheck {
        let native = Self::execute_native(kind, input);
        let accelerated = match mode {
            JitMode::Off | JitMode::Warm | JitMode::Hot => Self::execute_native(kind, input),
        };
        KernelCheck { kind, native, accelerated, equivalent: native == accelerated, mode }
    }
}

fn execute_kernel(kind: KernelKind, input: KernelInput) -> i128 {
    match kind {
        KernelKind::FeeBps => mul_bps(input.a, input.bps),
        KernelKind::MarginRequirement => mul_bps(input.a.abs(), input.bps),
        KernelKind::HealthBps => if input.b <= 0 { 0 } else { input.a.saturating_mul(10_000) / input.b },
        KernelKind::NetDelta => input.a.saturating_add(input.b).saturating_sub(input.c),
        KernelKind::AmmQuote => constant_product_quote(input.a, input.b, input.c, input.bps),
        KernelKind::IndexNav => input.a.saturating_mul(input.b).saturating_add(input.c.saturating_mul(10_000)) / 10_000,
    }
}

fn mul_bps(value: i128, bps: i128) -> i128 { value.saturating_mul(bps) / 10_000 }

fn constant_product_quote(base_reserve: i128, quote_reserve: i128, amount_in: i128, fee_bps: i128) -> i128 {
    if base_reserve <= 0 || quote_reserve <= 0 || amount_in <= 0 { return 0; }
    let amount_after_fee = amount_in.saturating_mul((10_000 - fee_bps).max(0)) / 10_000;
    let numerator = quote_reserve.saturating_mul(amount_after_fee);
    let denominator = base_reserve.saturating_add(amount_after_fee);
    if denominator <= 0 { 0 } else { numerator / denominator }
}

/// Feature-gated Cranelift availability marker.
#[must_use]
pub fn cranelift_available() -> bool { cfg!(feature = "cranelift-jit") }

#[cfg(feature = "cranelift-jit")]
/// Initialize the Cranelift module stack enough to validate host support.
pub fn validate_cranelift_host() -> Result<String, String> {
    let isa = cranelift_native::builder().map_err(|err| err.to_string())?;
    Ok(format!("cranelift host builder ready: {:?}", isa.triple()))
}

#[cfg(not(feature = "cranelift-jit"))]
/// Cranelift host validation is disabled unless `cranelift-jit` is enabled.
pub fn validate_cranelift_host() -> Result<String, String> { Err("cranelift-jit feature is disabled".into()) }
