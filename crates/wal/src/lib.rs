//! Durable write-ahead log foundation for Gravity.
//!
//! The WAL is intentionally simple and append-only. Hot services can record accepted
//! commands/events before slower Postgres/Redis persistence catches up. Replay logic
//! can then rebuild volatile state after a restart and safely reconcile settlement
//! receipts through existing idempotency keys.

use gravity_types::{now_ms, GravityError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A logical WAL stream.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WalStream {
    /// Accepted order/orderbook events.
    Orders,
    /// Fill and trade output events.
    Fills,
    /// Settlement batches and receipt reconciliation records.
    Settlement,
    /// Oracle source/report events.
    Oracle,
    /// AMM pool/swap/liquidity events.
    Amm,
    /// Risk snapshots and risk events.
    Risk,
    /// Liquidation candidates/plans/events.
    Liquidation,
    /// Generic persistence/audit records.
    Generic,
}

impl WalStream {
    /// Return the stable file name for the stream.
    pub fn file_name(&self) -> &'static str {
        match self {
            Self::Orders => "orders.wal",
            Self::Fills => "fills.wal",
            Self::Settlement => "settlement.wal",
            Self::Oracle => "oracle.wal",
            Self::Amm => "amm.wal",
            Self::Risk => "risk.wal",
            Self::Liquidation => "liquidation.wal",
            Self::Generic => "generic.wal",
        }
    }

    /// Map an existing Gravity persistence kind to a WAL stream.
    pub fn from_kind(kind: &str) -> Self {
        let lower = kind.to_ascii_lowercase();
        if lower.contains("order") || lower.contains("book") || lower.contains("cancel") || lower.contains("amend") || lower.contains("replace") {
            Self::Orders
        } else if lower.contains("fill") || lower.contains("trade") {
            Self::Fills
        } else if lower.contains("settlement") {
            Self::Settlement
        } else if lower.contains("oracle") {
            Self::Oracle
        } else if lower.contains("amm") || lower.contains("pool") || lower.contains("swap") || lower.contains("liquidity") {
            Self::Amm
        } else if lower.contains("risk") || lower.contains("margin") || lower.contains("collateral") {
            Self::Risk
        } else if lower.contains("liquidation") {
            Self::Liquidation
        } else {
            Self::Generic
        }
    }
}

/// A single append-only WAL record.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalRecord {
    /// Monotonic local WAL sequence.
    pub wal_sequence: u64,
    /// Logical stream.
    pub stream: WalStream,
    /// Original event kind.
    pub kind: String,
    /// Primary target such as symbol/account/batch id.
    pub target: String,
    /// Source sequence from the producing subsystem.
    pub source_sequence: u64,
    /// Wall-clock timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// JSON payload body.
    pub body: Value,
}

/// Lightweight WAL runtime statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalStats {
    /// Whether append-to-file is enabled.
    pub enabled: bool,
    /// WAL root directory.
    pub root: String,
    /// Total records accepted by the WAL manager.
    pub records: u64,
    /// Records kept in the in-memory recent ring.
    pub recent: usize,
    /// Recent ring capacity.
    pub recent_capacity: usize,
    /// Last local WAL sequence.
    pub last_sequence: u64,
    /// Last successful checkpoint timestamp.
    pub last_checkpoint_ms: u64,
    /// Append/write failures observed.
    pub write_errors: u64,
}

/// A checkpoint marker used by release/recovery tooling.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRecord {
    /// Checkpoint id.
    pub id: String,
    /// Timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Last WAL sequence covered by this checkpoint.
    pub last_sequence: u64,
    /// Human-readable note.
    pub note: String,
}

/// Per-stream replay scan statistics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamReplayStats {
    /// Stream file name.
    pub stream: String,
    /// Whether the stream file exists.
    pub exists: bool,
    /// Valid records decoded from the stream.
    pub records: u64,
    /// Malformed JSON lines or invalid records.
    pub malformed: u64,
    /// First WAL sequence seen in this stream.
    pub first_sequence: u64,
    /// Last WAL sequence seen in this stream.
    pub last_sequence: u64,
    /// First timestamp seen in this stream.
    pub first_timestamp_ms: u64,
    /// Last timestamp seen in this stream.
    pub last_timestamp_ms: u64,
    /// Number of local sequence regressions inside the stream file.
    pub sequence_regressions: u64,
}

/// WAL recovery verdict.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryVerdict {
    /// All scanned streams decoded cleanly.
    Healthy,
    /// Replay is possible, but some expected files are missing or empty.
    Degraded,
    /// One or more WAL records are malformed or sequence order regressed.
    Corrupt,
}

/// Full recovery report produced by scanning persisted WAL files.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// WAL root directory.
    pub root: String,
    /// Report timestamp.
    pub timestamp_ms: u64,
    /// Overall verdict.
    pub verdict: RecoveryVerdict,
    /// Total decoded records.
    pub total_records: u64,
    /// Total malformed lines.
    pub malformed_records: u64,
    /// Missing stream files.
    pub missing_streams: u64,
    /// Empty stream files.
    pub empty_streams: u64,
    /// Highest WAL sequence observed while scanning.
    pub last_sequence: u64,
    /// Last checkpoint if one exists.
    pub latest_checkpoint: Option<CheckpointRecord>,
    /// Per-stream replay statistics.
    pub streams: Vec<StreamReplayStats>,
    /// Human-readable recovery actions.
    pub actions: Vec<String>,
}

/// Replay planning output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReplayPlan {
    /// WAL root directory.
    pub root: String,
    /// Last known checkpoint timestamp.
    pub last_checkpoint_ms: u64,
    /// Last known WAL sequence.
    pub last_sequence: u64,
    /// Streams that should be scanned during startup replay.
    pub streams: Vec<String>,
    /// Human-readable replay mode.
    pub mode: String,
}

#[derive(Clone, Debug)]
struct WalInner {
    enabled: bool,
    root: PathBuf,
    records: u64,
    sequence: u64,
    write_errors: u64,
    last_checkpoint_ms: u64,
    recent_limit: usize,
    recent: VecDeque<WalRecord>,
}

/// Cloneable WAL manager.
#[derive(Clone, Debug)]
pub struct WalManager {
    inner: Arc<Mutex<WalInner>>,
}

impl Default for WalManager {
    fn default() -> Self {
        Self::new("runtime/wal", true, 100_000)
    }
}

impl WalManager {
    /// Create a WAL manager.
    pub fn new(root: impl Into<PathBuf>, enabled: bool, recent_limit: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(WalInner {
                enabled,
                root: root.into(),
                records: 0,
                sequence: 0,
                write_errors: 0,
                last_checkpoint_ms: 0,
                recent_limit: recent_limit.max(1),
                recent: VecDeque::with_capacity(recent_limit.max(1).min(1_000_000)),
            })),
        }
    }

    /// Append a record to the WAL. File-write errors are returned to the caller so
    /// production code can fail closed when desired.
    pub fn append(&self, kind: impl Into<String>, target: impl Into<String>, source_sequence: u64, body: Value) -> Result<WalRecord, GravityError> {
        let kind = kind.into();
        let target = target.into();
        let mut inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
        inner.sequence = inner.sequence.saturating_add(1);
        inner.records = inner.records.saturating_add(1);
        let record = WalRecord {
            wal_sequence: inner.sequence,
            stream: WalStream::from_kind(&kind),
            kind,
            target,
            source_sequence,
            timestamp_ms: now_ms(),
            body,
        };
        if inner.enabled {
            if let Err(err) = append_record(&inner.root, &record) {
                inner.write_errors = inner.write_errors.saturating_add(1);
                return Err(GravityError::Database(format!("wal append failed: {err}")));
            }
        }
        if inner.recent.len() >= inner.recent_limit { inner.recent.pop_front(); }
        inner.recent.push_back(record.clone());
        Ok(record)
    }

    /// Return runtime stats.
    pub fn stats(&self) -> Result<WalStats, GravityError> {
        let inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
        Ok(WalStats {
            enabled: inner.enabled,
            root: inner.root.display().to_string(),
            records: inner.records,
            recent: inner.recent.len(),
            recent_capacity: inner.recent_limit,
            last_sequence: inner.sequence,
            last_checkpoint_ms: inner.last_checkpoint_ms,
            write_errors: inner.write_errors,
        })
    }

    /// Return recent WAL records.
    pub fn recent(&self, limit: usize) -> Result<Vec<WalRecord>, GravityError> {
        let limit = limit.min(10_000);
        let inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
        let mut out = inner.recent.iter().rev().take(limit).cloned().collect::<Vec<_>>();
        out.reverse();
        Ok(out)
    }

    /// Create a checkpoint marker.
    pub fn checkpoint(&self, note: impl Into<String>) -> Result<CheckpointRecord, GravityError> {
        let mut inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
        let now = now_ms();
        let checkpoint = CheckpointRecord {
            id: format!("ckpt-{}-{}", now, inner.sequence),
            timestamp_ms: now,
            last_sequence: inner.sequence,
            note: note.into(),
        };
        inner.last_checkpoint_ms = now;
        if inner.enabled {
            if let Err(err) = write_checkpoint(&inner.root, &checkpoint) {
                inner.write_errors = inner.write_errors.saturating_add(1);
                return Err(GravityError::Database(format!("wal checkpoint failed: {err}")));
            }
        }
        Ok(checkpoint)
    }

    /// Produce a replay plan for startup recovery tooling.
    pub fn replay_plan(&self) -> Result<ReplayPlan, GravityError> {
        let inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
        Ok(ReplayPlan {
            root: inner.root.display().to_string(),
            last_checkpoint_ms: inner.last_checkpoint_ms,
            last_sequence: inner.sequence,
            streams: [
                WalStream::Orders,
                WalStream::Fills,
                WalStream::Settlement,
                WalStream::Oracle,
                WalStream::Amm,
                WalStream::Risk,
                WalStream::Liquidation,
                WalStream::Generic,
            ].iter().map(|s| s.file_name().to_string()).collect(),
            mode: "scan-checkpoint-replay".into(),
        })
    }

    /// Read checkpoint markers from disk, newest last.
    pub fn checkpoints(&self, limit: usize) -> Result<Vec<CheckpointRecord>, GravityError> {
        let root = {
            let inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
            inner.root.clone()
        };
        read_checkpoints(&root, limit.min(10_000))
    }

    /// Scan WAL files and produce a production recovery report.
    pub fn recovery_report(&self) -> Result<RecoveryReport, GravityError> {
        let root = {
            let inner = self.inner.lock().map_err(|_| GravityError::Database("wal lock poisoned".into()))?;
            inner.root.clone()
        };
        build_recovery_report(&root)
    }

    /// Dry-run startup replay. This does not mutate runtime state yet; it verifies
    /// readable WAL files, checkpoints, and the action plan needed by production startup.
    pub fn replay_dry_run(&self) -> Result<RecoveryReport, GravityError> {
        self.recovery_report()
    }

}

fn append_record(root: &Path, record: &WalRecord) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let path = root.join(record.stream.file_name());
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, record)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn write_checkpoint(root: &Path, checkpoint: &CheckpointRecord) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let path = root.join("checkpoints.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, checkpoint)?;
    file.write_all(b"\n")?;
    Ok(())
}


fn all_streams() -> [WalStream; 8] {
    [
        WalStream::Orders,
        WalStream::Fills,
        WalStream::Settlement,
        WalStream::Oracle,
        WalStream::Amm,
        WalStream::Risk,
        WalStream::Liquidation,
        WalStream::Generic,
    ]
}

fn read_checkpoints(root: &Path, limit: usize) -> Result<Vec<CheckpointRecord>, GravityError> {
    let path = root.join("checkpoints.jsonl");
    if !path.exists() { return Ok(Vec::new()); }
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut checkpoints = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        checkpoints.push(serde_json::from_str::<CheckpointRecord>(&line)?);
    }
    if checkpoints.len() > limit {
        Ok(checkpoints[checkpoints.len().saturating_sub(limit)..].to_vec())
    } else {
        Ok(checkpoints)
    }
}

fn scan_stream_file(root: &Path, stream: WalStream) -> Result<StreamReplayStats, GravityError> {
    let file_name = stream.file_name().to_string();
    let path = root.join(&file_name);
    if !path.exists() {
        return Ok(StreamReplayStats {
            stream: file_name,
            exists: false,
            records: 0,
            malformed: 0,
            first_sequence: 0,
            last_sequence: 0,
            first_timestamp_ms: 0,
            last_timestamp_ms: 0,
            sequence_regressions: 0,
        });
    }
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut stats = StreamReplayStats {
        stream: file_name,
        exists: true,
        records: 0,
        malformed: 0,
        first_sequence: 0,
        last_sequence: 0,
        first_timestamp_ms: 0,
        last_timestamp_ms: 0,
        sequence_regressions: 0,
    };
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<WalRecord>(&line) {
            Ok(record) => {
                if stats.records == 0 {
                    stats.first_sequence = record.wal_sequence;
                    stats.first_timestamp_ms = record.timestamp_ms;
                } else if record.wal_sequence <= stats.last_sequence {
                    stats.sequence_regressions = stats.sequence_regressions.saturating_add(1);
                }
                stats.records = stats.records.saturating_add(1);
                stats.last_sequence = record.wal_sequence;
                stats.last_timestamp_ms = record.timestamp_ms;
            }
            Err(_) => {
                stats.malformed = stats.malformed.saturating_add(1);
            }
        }
    }
    Ok(stats)
}

fn build_recovery_report(root: &Path) -> Result<RecoveryReport, GravityError> {
    let checkpoints = read_checkpoints(root, 10_000)?;
    let latest_checkpoint = checkpoints.last().cloned();
    let mut streams = Vec::new();
    let mut total_records = 0_u64;
    let mut malformed_records = 0_u64;
    let mut missing_streams = 0_u64;
    let mut empty_streams = 0_u64;
    let mut last_sequence = latest_checkpoint.as_ref().map(|v| v.last_sequence).unwrap_or(0);
    let mut corrupt = false;

    for stream in all_streams() {
        let stats = scan_stream_file(root, stream)?;
        if !stats.exists { missing_streams = missing_streams.saturating_add(1); }
        if stats.exists && stats.records == 0 { empty_streams = empty_streams.saturating_add(1); }
        if stats.malformed > 0 || stats.sequence_regressions > 0 { corrupt = true; }
        total_records = total_records.saturating_add(stats.records);
        malformed_records = malformed_records.saturating_add(stats.malformed);
        last_sequence = last_sequence.max(stats.last_sequence);
        streams.push(stats);
    }

    let verdict = if corrupt {
        RecoveryVerdict::Corrupt
    } else if missing_streams > 0 || empty_streams > 0 || total_records == 0 {
        RecoveryVerdict::Degraded
    } else {
        RecoveryVerdict::Healthy
    };

    let mut actions = Vec::new();
    actions.push("load latest checkpoint if present".to_string());
    actions.push("scan each WAL stream after the checkpoint sequence".to_string());
    actions.push("rebuild volatile orderbook/oracle/AMM/risk/liquidation/perps/index state".to_string());
    actions.push("reconcile settlement batches by idempotency key".to_string());
    actions.push("requeue dead-letter settlement records for operator-approved retry".to_string());
    if corrupt { actions.push("stop startup and require recovery operator review because corruption was detected".to_string()); }
    if missing_streams > 0 { actions.push("continue in degraded mode only for missing streams that are optional in this environment".to_string()); }

    Ok(RecoveryReport {
        root: root.display().to_string(),
        timestamp_ms: now_ms(),
        verdict,
        total_records,
        malformed_records,
        missing_streams,
        empty_streams,
        last_sequence,
        latest_checkpoint,
        streams,
        actions,
    })
}
