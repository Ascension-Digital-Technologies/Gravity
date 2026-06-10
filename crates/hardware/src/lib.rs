use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeProfile {
    Balanced,
    LowLatency,
    HighThroughput,
    MarketMaker,
    OracleHeavy,
    StreamHeavy,
    StorageHeavy,
}

impl RuntimeProfile {
    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "low-latency" | "low_latency" => Self::LowLatency,
            "high-throughput" | "high_throughput" => Self::HighThroughput,
            "market-maker" | "market_maker" => Self::MarketMaker,
            "oracle-heavy" | "oracle_heavy" => Self::OracleHeavy,
            "stream-heavy" | "stream_heavy" => Self::StreamHeavy,
            "storage-heavy" | "storage_heavy" => Self::StorageHeavy,
            _ => Self::Balanced,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub os: String,
    pub architecture: String,
    pub logical_cores: usize,
    pub pinning_available: bool,
    pub detected_core_ids: Vec<usize>,
    pub supports_processor_groups: bool,
    pub supports_numa_hinting: bool,
    pub timer_resolution_hint: String,
    pub notes: Vec<String>,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let logical_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
        let detected = core_affinity::get_core_ids().unwrap_or_default();
        let detected_core_ids = detected.iter().map(|id| id.id).collect::<Vec<_>>();
        let os = std::env::consts::OS.to_string();
        let mut notes = Vec::new();
        if cfg!(target_os = "windows") {
            notes.push("Windows profile: keep matching tiles pinned away from storage/stream tasks; processor group handling can be added for >64 logical CPUs.".into());
        } else if cfg!(target_os = "linux") {
            notes.push("Linux profile: NUMA-aware placement and io_uring networking/storage can be enabled in later production builds.".into());
        } else {
            notes.push("Generic profile: use portable core placement hints only.".into());
        }
        if detected_core_ids.is_empty() {
            notes.push("CPU pinning unavailable; Gravity will run without failing.".into());
        }
        Self {
            os,
            architecture: std::env::consts::ARCH.to_string(),
            logical_cores,
            pinning_available: !detected_core_ids.is_empty(),
            detected_core_ids,
            supports_processor_groups: cfg!(target_os = "windows"),
            supports_numa_hinting: cfg!(target_os = "linux"),
            timer_resolution_hint: if cfg!(target_os = "windows") { "consider high-resolution timer setup for low-latency profiles".into() } else { "use monotonic clock and optional busy-poll only in explicit low-latency profile".into() },
            notes,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TilePlacement {
    pub tile: String,
    pub role: String,
    pub core: Option<usize>,
    pub priority: String,
    pub batch_hint: usize,
    pub queue_hint: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HardwarePlan {
    pub profile: RuntimeProfile,
    pub logical_cores: usize,
    pub hot_market_cores: Vec<usize>,
    pub storage_cores: Vec<usize>,
    pub stream_cores: Vec<usize>,
    pub oracle_risk_cores: Vec<usize>,
    pub placements: Vec<TilePlacement>,
    pub settings: BTreeMap<String, String>,
}

pub fn build_plan(profile: RuntimeProfile, hw: &HardwareProfile) -> HardwarePlan {
    let cores = if hw.detected_core_ids.is_empty() { (0..hw.logical_cores).collect::<Vec<_>>() } else { hw.detected_core_ids.clone() };
    let core = |idx: usize| -> Option<usize> { cores.get(idx % cores.len().max(1)).copied() };
    let mut settings = BTreeMap::new();
    settings.insert("auto_tune".into(), "enabled-with-guardrails".into());
    settings.insert("never_tune_order_priority".into(), "true".into());
    settings.insert("never_tune_fixed_point_math".into(), "true".into());
    settings.insert("never_tune_replay_order".into(), "true".into());
    let (match_batch, queue, priority) = match profile {
        RuntimeProfile::LowLatency | RuntimeProfile::MarketMaker => (512, 65_536, "latency"),
        RuntimeProfile::HighThroughput => (4096, 262_144, "throughput"),
        RuntimeProfile::OracleHeavy => (2048, 131_072, "oracle"),
        RuntimeProfile::StreamHeavy => (2048, 131_072, "stream"),
        RuntimeProfile::StorageHeavy => (1024, 65_536, "storage"),
        RuntimeProfile::Balanced => (2048, 131_072, "balanced"),
    };
    let placements = vec![
        TilePlacement { tile: "ingress".into(), role: "IngressTile".into(), core: core(0), priority: priority.into(), batch_hint: match_batch, queue_hint: queue },
        TilePlacement { tile: "decode".into(), role: "BinaryDecodeTile".into(), core: core(1), priority: priority.into(), batch_hint: match_batch, queue_hint: queue },
        TilePlacement { tile: "match-hot-0".into(), role: "MarketMatchTile".into(), core: core(2), priority: "hot-market".into(), batch_hint: match_batch, queue_hint: queue },
        TilePlacement { tile: "match-hot-1".into(), role: "MarketMatchTile".into(), core: core(3), priority: "hot-market".into(), batch_hint: match_batch, queue_hint: queue },
        TilePlacement { tile: "oracle-risk".into(), role: "OracleRiskTile".into(), core: core(4), priority: "safety".into(), batch_hint: 1024, queue_hint: queue / 2 },
        TilePlacement { tile: "settlement".into(), role: "SettlementTile".into(), core: core(5), priority: "finalization".into(), batch_hint: 2048, queue_hint: queue / 2 },
        TilePlacement { tile: "stream".into(), role: "StreamTile".into(), core: core(6), priority: "fanout".into(), batch_hint: 2048, queue_hint: queue / 2 },
        TilePlacement { tile: "storage".into(), role: "StorageTile".into(), core: core(7), priority: "durable".into(), batch_hint: 512, queue_hint: queue / 4 },
    ];
    HardwarePlan {
        profile,
        logical_cores: hw.logical_cores,
        hot_market_cores: placements.iter().filter(|p| p.priority == "hot-market").filter_map(|p| p.core).collect(),
        storage_cores: placements.iter().filter(|p| p.role == "StorageTile").filter_map(|p| p.core).collect(),
        stream_cores: placements.iter().filter(|p| p.role == "StreamTile").filter_map(|p| p.core).collect(),
        oracle_risk_cores: placements.iter().filter(|p| p.role == "OracleRiskTile").filter_map(|p| p.core).collect(),
        placements,
        settings,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlacementSimulation {
    pub profile: RuntimeProfile,
    pub planned_tiles: usize,
    pub pinned_tiles: usize,
    pub hot_market_cores: usize,
    pub storage_isolated: bool,
    pub stream_isolated: bool,
    pub recommendation: String,
}

pub fn simulate(profile: RuntimeProfile) -> PlacementSimulation {
    let hw = HardwareProfile::detect();
    let plan = build_plan(profile.clone(), &hw);
    let pinned_tiles = plan.placements.iter().filter(|p| p.core.is_some()).count();
    let storage_isolated = !plan.storage_cores.iter().any(|c| plan.hot_market_cores.contains(c));
    let stream_isolated = !plan.stream_cores.iter().any(|c| plan.hot_market_cores.contains(c));
    let recommendation = if hw.logical_cores >= 8 {
        "hardware has enough logical cores for isolated match/storage/stream placement".into()
    } else {
        "hardware has limited cores; use balanced profile and smaller batches".into()
    };
    PlacementSimulation { profile, planned_tiles: plan.placements.len(), pinned_tiles, hot_market_cores: plan.hot_market_cores.len(), storage_isolated, stream_isolated, recommendation }
}
