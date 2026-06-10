use gravity_book::{Fill, OrderResult};
use gravity_types::{now_ms, stable_hash_hex, GravityError, OracleReport, SettlementPayload, SettlementReceipt, Symbol};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

const RECENT_LIMIT: usize = 100_000;
const DEAD_LIMIT: usize = 10_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementBatch {
    pub id: String,
    pub payloads: Vec<SettlementPayload>,
    pub created_ms: u64,
    pub idempotency_key: String,
    pub payload_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementBatchReceipt {
    pub batch_id: String,
    pub accepted: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub receipts: Vec<SettlementReceipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetSettlementDelta {
    pub account: String,
    pub symbol: Symbol,
    pub fills: u64,
    pub bought_raw: i128,
    pub sold_raw: i128,
    pub quote_notional_raw: i128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementCompressionReport {
    pub input_fills: usize,
    pub output_deltas: usize,
    pub compression_ratio_bps: u64,
    pub fills_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompressedSettlementBatch {
    pub id: String,
    pub symbol: Symbol,
    pub fills_root: String,
    pub audit_root: String,
    pub sequence_window: SettlementSequenceWindow,
    pub deltas: Vec<NetSettlementDelta>,
    pub fill_ids: Vec<String>,
    pub report: SettlementCompressionReport,
    pub created_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementSequenceWindow {
    pub first_timestamp_ms: u64,
    pub last_timestamp_ms: u64,
    pub fill_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StargateInstruction {
    pub kind: String,
    pub symbol: Symbol,
    pub batch_id: String,
    pub idempotency_key: String,
    pub payload_root: String,
    pub audit_root: String,
    pub body: String,
    pub created_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SettlementStatus {
    Submitted,
    Duplicate,
    Finalized,
    Failed,
    DeadLetter,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementFinalizationRecord {
    pub batch_id: String,
    pub kind: String,
    pub symbol: Symbol,
    pub idempotency_key: String,
    pub payload_root: String,
    pub status: SettlementStatus,
    pub reference: String,
    pub attempts: u32,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SettlementStats {
    pub submitted: u64,
    pub duplicates: u64,
    pub finalized: u64,
    pub failed: u64,
    pub dead_letters: u64,
    pub recent: usize,
    pub dead_queue: usize,
    pub accepted_payloads: usize,
}

impl SettlementBatch {
    pub fn new(payloads: Vec<SettlementPayload>) -> Self {
        let created_ms = now_ms();
        let seed = payloads.iter().map(|p| p.idempotency.as_str()).collect::<Vec<_>>().join("|");
        let payload_root = stable_hash_hex(&seed);
        let idempotency_key = format!("batch:{payload_root}");
        Self { id: stable_hash_hex(&format!("{created_ms}:{payload_root}")), payloads, created_ms, idempotency_key, payload_root }
    }
}

impl CompressedSettlementBatch {
    pub fn from_fills(symbol: Symbol, fills: &[Fill]) -> Self {
        let created_ms = now_ms();
        let fills_root = fast_fill_root(fills);
        let audit_root = audit_root(fills);
        let fill_ids = compact_fill_ids(fills);
        let sequence_window = sequence_window(fills);
        let mut deltas: HashMap<(String, String), NetSettlementDelta> = HashMap::with_capacity(fills.len().saturating_mul(2).min(65_536));

        for fill in fills {
            let qty = fill.quantity.0.as_raw();
            let notional = qty.saturating_mul(fill.price.0.as_raw());
            apply_delta(&mut deltas, &fill.maker_account, &fill.symbol, qty, notional, fill.taker_side, true);
            apply_delta(&mut deltas, &fill.taker_account, &fill.symbol, qty, notional, fill.taker_side, false);
        }

        let mut deltas = deltas.into_values().collect::<Vec<_>>();
        deltas.sort_unstable_by(|a, b| a.account.cmp(&b.account).then_with(|| a.symbol.0.cmp(&b.symbol.0)));
        let output_deltas = deltas.len();
        let compression_ratio_bps = if fills.is_empty() { 0 } else { ((output_deltas as u64) * 10_000) / (fills.len() as u64) };
        let report = SettlementCompressionReport { input_fills: fills.len(), output_deltas, compression_ratio_bps, fills_root: fills_root.clone() };
        let id = stable_hash_hex(&format!("compressed:{}:{}:{}", symbol, sequence_window.fill_count, fills_root));
        Self { id, symbol, fills_root, audit_root, sequence_window, deltas, fill_ids, report, created_ms }
    }

    pub fn idempotency_key(&self) -> String { format!("compressed:{}:{}", self.symbol, self.fills_root) }

    pub fn to_instruction(&self) -> Result<StargateInstruction, GravityError> {
        let body = serde_json::to_string(self)?;
        Ok(StargateInstruction {
            kind: "SettleCompressedTrades".into(),
            symbol: self.symbol.clone(),
            batch_id: self.id.clone(),
            idempotency_key: self.idempotency_key(),
            payload_root: self.fills_root.clone(),
            audit_root: self.audit_root.clone(),
            body,
            created_ms: now_ms(),
        })
    }

    pub fn to_payload(self) -> Result<SettlementPayload, GravityError> {
        let instruction = self.to_instruction()?;
        let body = serde_json::to_string(&instruction)?;
        Ok(SettlementPayload {
            kind: instruction.kind,
            symbol: instruction.symbol,
            idempotency: instruction.idempotency_key,
            timestamp_ms: instruction.created_ms,
            body,
        })
    }
}

fn apply_delta(
    deltas: &mut HashMap<(String, String), NetSettlementDelta>,
    account: &str,
    symbol: &Symbol,
    qty: i128,
    notional: i128,
    taker_side: gravity_types::Side,
    is_maker: bool,
) {
    let delta = deltas.entry((account.to_owned(), symbol.0.clone())).or_insert_with(|| NetSettlementDelta {
        account: account.to_owned(),
        symbol: symbol.clone(),
        fills: 0,
        bought_raw: 0,
        sold_raw: 0,
        quote_notional_raw: 0,
    });
    delta.fills = delta.fills.saturating_add(1);
    match (is_maker, taker_side) {
        (true, gravity_types::Side::Buy) | (false, gravity_types::Side::Sell) => {
            delta.sold_raw = delta.sold_raw.saturating_add(qty);
            delta.quote_notional_raw = delta.quote_notional_raw.saturating_add(notional);
        }
        (true, gravity_types::Side::Sell) | (false, gravity_types::Side::Buy) => {
            delta.bought_raw = delta.bought_raw.saturating_add(qty);
            delta.quote_notional_raw = delta.quote_notional_raw.saturating_sub(notional);
        }
    }
}

fn fast_fill_root(fills: &[Fill]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for fill in fills {
        for byte in fill.id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn audit_root(fills: &[Fill]) -> String {
    let mut seed = String::with_capacity(fills.len().saturating_mul(24).min(1_000_000));
    for fill in fills.iter().take(16_384) {
        seed.push_str(&fill.id);
        seed.push(':');
        seed.push_str(&fill.maker_account);
        seed.push(':');
        seed.push_str(&fill.taker_account);
        seed.push('|');
    }
    if fills.len() > 16_384 { seed.push_str(&format!("omitted:{}", fills.len() - 16_384)); }
    stable_hash_hex(&seed)
}

fn sequence_window(fills: &[Fill]) -> SettlementSequenceWindow {
    let mut first = u64::MAX;
    let mut last = 0_u64;
    for fill in fills {
        first = first.min(fill.timestamp_ms);
        last = last.max(fill.timestamp_ms);
    }
    if fills.is_empty() { first = 0; }
    SettlementSequenceWindow { first_timestamp_ms: first, last_timestamp_ms: last, fill_count: fills.len() }
}

fn compact_fill_ids(fills: &[Fill]) -> Vec<String> {
    const FULL_LIMIT: usize = 4096;
    if fills.len() <= FULL_LIMIT { return fills.iter().map(|f| f.id.clone()).collect(); }
    let mut ids = Vec::with_capacity(3);
    if let Some(first) = fills.first() { ids.push(first.id.clone()); }
    if fills.len() > 2 { ids.push(format!("omitted:{}", fills.len().saturating_sub(2))); }
    if let Some(last) = fills.last() { ids.push(last.id.clone()); }
    ids
}

#[derive(Clone, Debug)]
pub struct SettlementClient {
    endpoint: String,
    accepted: Arc<Mutex<Vec<SettlementPayload>>>,
    seen: Arc<Mutex<BTreeSet<String>>>,
    recent: Arc<Mutex<VecDeque<SettlementFinalizationRecord>>>,
    dead: Arc<Mutex<VecDeque<SettlementFinalizationRecord>>>,
    stats: Arc<Mutex<SettlementStats>>,
}

impl SettlementClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            accepted: Arc::new(Mutex::new(Vec::new())),
            seen: Arc::new(Mutex::new(BTreeSet::new())),
            recent: Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_LIMIT))),
            dead: Arc::new(Mutex::new(VecDeque::with_capacity(DEAD_LIMIT))),
            stats: Arc::new(Mutex::new(SettlementStats::default())),
        }
    }

    pub fn endpoint(&self) -> &str { &self.endpoint }

    pub async fn submit(&self, payload: SettlementPayload) -> Result<SettlementReceipt, GravityError> {
        let mut seen = self.seen.lock().await;
        let duplicate = !seen.insert(payload.idempotency.clone());
        drop(seen);

        if duplicate {
            let receipt = SettlementReceipt { accepted: true, reference: payload.idempotency.clone(), message: "duplicate ignored by local idempotency guard".into() };
            self.record(payload, SettlementStatus::Duplicate, receipt.reference.clone(), receipt.message.clone(), 1).await;
            return Ok(receipt);
        }

        let reference = format!("gravity-{}-{}", payload.kind, payload.timestamp_ms);
        self.accepted.lock().await.push(payload.clone());
        let receipt = SettlementReceipt { accepted: true, reference, message: "accepted by local v1.9 settlement finalizer".into() };
        self.record(payload, SettlementStatus::Finalized, receipt.reference.clone(), receipt.message.clone(), 1).await;
        Ok(receipt)
    }

    async fn record(&self, payload: SettlementPayload, status: SettlementStatus, reference: String, message: String, attempts: u32) {
        let updated_ms = now_ms();
        let record = SettlementFinalizationRecord {
            batch_id: stable_hash_hex(&format!("{}:{}", payload.kind, payload.idempotency)),
            kind: payload.kind,
            symbol: payload.symbol,
            idempotency_key: payload.idempotency,
            payload_root: stable_hash_hex(&payload.body),
            status: status.clone(),
            reference,
            attempts,
            created_ms: payload.timestamp_ms,
            updated_ms,
            message,
        };
        if let Ok(mut stats) = self.stats.try_lock() {
            match status {
                SettlementStatus::Submitted => stats.submitted = stats.submitted.saturating_add(1),
                SettlementStatus::Duplicate => stats.duplicates = stats.duplicates.saturating_add(1),
                SettlementStatus::Finalized => stats.finalized = stats.finalized.saturating_add(1),
                SettlementStatus::Failed => stats.failed = stats.failed.saturating_add(1),
                SettlementStatus::DeadLetter => stats.dead_letters = stats.dead_letters.saturating_add(1),
            }
        }
        push_limited(&self.recent, record.clone(), RECENT_LIMIT).await;
        if matches!(record.status, SettlementStatus::DeadLetter | SettlementStatus::Failed) {
            push_limited(&self.dead, record, DEAD_LIMIT).await;
        }
    }

    pub async fn submit_batch(&self, batch: SettlementBatch) -> Result<SettlementBatchReceipt, GravityError> {
        let mut receipts = Vec::with_capacity(batch.payloads.len());
        let mut accepted = 0_usize;
        let mut duplicates = 0_usize;
        let mut failed = 0_usize;
        for payload in batch.payloads {
            let receipt = self.submit(payload).await?;
            if receipt.message.contains("duplicate") { duplicates += 1; }
            else if receipt.accepted { accepted += 1; }
            else { failed += 1; }
            receipts.push(receipt);
        }
        Ok(SettlementBatchReceipt { batch_id: batch.id, accepted, duplicates, failed, receipts })
    }

    pub async fn submit_compressed_fills(&self, symbol: Symbol, fills: &[Fill]) -> Result<SettlementReceipt, GravityError> {
        if fills.is_empty() {
            return Ok(SettlementReceipt { accepted: true, reference: "empty-fill-batch".into(), message: "empty compressed fill batch ignored".into() });
        }
        let payload = CompressedSettlementBatch::from_fills(symbol, fills).to_payload()?;
        self.submit(payload).await
    }

    pub async fn submit_oracle(&self, report: &OracleReport) -> Result<SettlementReceipt, GravityError> {
        self.submit(SettlementPayload {
            kind: "UpdateOracle".into(),
            symbol: report.symbol.clone(),
            body: serde_json::to_string(report)?,
            idempotency: format!("oracle:{}:{}:{}", report.symbol, report.timestamp_ms, report.payload_hash),
            timestamp_ms: now_ms(),
        }).await
    }

    pub async fn submit_fill(&self, fill: &Fill) -> Result<SettlementReceipt, GravityError> {
        self.submit(SettlementPayload {
            kind: "SettleTrade".into(),
            symbol: fill.symbol.clone(),
            body: serde_json::to_string(fill)?,
            idempotency: format!("fill:{}:{}", fill.symbol, fill.id),
            timestamp_ms: now_ms(),
        }).await
    }

    pub async fn submit_order_result(&self, result: &OrderResult) -> Result<Vec<SettlementReceipt>, GravityError> {
        if result.fills.is_empty() { return Ok(Vec::new()); }
        let symbol = result.fills[0].symbol.clone();
        let receipt = self.submit_compressed_fills(symbol, &result.fills).await?;
        Ok(vec![receipt])
    }

    pub async fn accepted_count(&self) -> usize { self.accepted.lock().await.len() }

    pub async fn stats(&self) -> SettlementStats {
        let mut stats = self.stats.lock().await.clone();
        stats.recent = self.recent.lock().await.len();
        stats.dead_queue = self.dead.lock().await.len();
        stats.accepted_payloads = self.accepted.lock().await.len();
        stats
    }

    pub async fn recent(&self, limit: usize) -> Vec<SettlementFinalizationRecord> {
        recent_limited(&self.recent, limit.min(10_000)).await
    }

    pub async fn dead_letters(&self, limit: usize) -> Vec<SettlementFinalizationRecord> {
        recent_limited(&self.dead, limit.min(10_000)).await
    }

    pub async fn retry_dead_letters(&self, limit: usize) -> Result<SettlementBatchReceipt, GravityError> {
        let mut dead = self.dead.lock().await;
        let take = limit.min(dead.len()).min(10_000);
        let mut payloads = Vec::with_capacity(take);
        for _ in 0..take {
            if let Some(record) = dead.pop_front() {
                payloads.push(SettlementPayload {
                    kind: record.kind,
                    symbol: record.symbol,
                    body: record.message,
                    idempotency: format!("retry:{}:{}", record.idempotency_key, now_ms()),
                    timestamp_ms: now_ms(),
                });
            }
        }
        drop(dead);
        self.submit_batch(SettlementBatch::new(payloads)).await
    }
}

async fn push_limited(queue: &Arc<Mutex<VecDeque<SettlementFinalizationRecord>>>, record: SettlementFinalizationRecord, limit: usize) {
    let mut guard = queue.lock().await;
    if guard.len() >= limit { guard.pop_front(); }
    guard.push_back(record);
}

async fn recent_limited(queue: &Arc<Mutex<VecDeque<SettlementFinalizationRecord>>>, limit: usize) -> Vec<SettlementFinalizationRecord> {
    let guard = queue.lock().await;
    let mut records = guard.iter().rev().take(limit).cloned().collect::<Vec<_>>();
    records.reverse();
    records
}
