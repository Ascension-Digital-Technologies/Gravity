use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum StreamEncoding {
    Json,
    Binary,
}

#[derive(Clone, Debug)]
pub struct StreamFrame {
    pub sequence: u64,
    pub topic: String,
    pub key: String,
    pub encoding: StreamEncoding,
    pub timestamp_ms: u64,
    pub payload: Arc<[u8]>,
}

impl StreamFrame {
    pub fn record(&self) -> StreamRecord {
        StreamRecord {
            sequence: self.sequence,
            topic: self.topic.clone(),
            key: self.key.clone(),
            encoding: self.encoding.clone(),
            timestamp_ms: self.timestamp_ms,
            payload_len: self.payload.len(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamRecord {
    pub sequence: u64,
    pub topic: String,
    pub key: String,
    pub encoding: StreamEncoding,
    pub timestamp_ms: u64,
    pub payload_len: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopicStats {
    pub topic: String,
    pub published: u64,
    pub dropped: u64,
    pub subscribers: usize,
    pub recent: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamStats {
    pub topics: usize,
    pub published: u64,
    pub dropped: u64,
    pub topic_stats: Vec<TopicStats>,
}

struct TopicState {
    tx: broadcast::Sender<StreamFrame>,
    recent: VecDeque<StreamRecord>,
    published: u64,
    dropped: u64,
}

#[derive(Clone)]
pub struct StreamHub {
    topics: Arc<Mutex<BTreeMap<String, TopicState>>>,
    topic_capacity: usize,
    recent_limit: usize,
}

impl Default for StreamHub {
    fn default() -> Self { Self::new(65_536, 100_000) }
}

impl StreamHub {
    pub fn new(topic_capacity: usize, recent_limit: usize) -> Self {
        Self { topics: Arc::new(Mutex::new(BTreeMap::new())), topic_capacity, recent_limit }
    }

    pub fn publish_json(&self, topic: impl Into<String>, key: impl Into<String>, sequence: u64, timestamp_ms: u64, value: &Value) -> Result<StreamRecord, serde_json::Error> {
        let bytes = serde_json::to_vec(value)?;
        Ok(self.publish_bytes(topic, key, sequence, timestamp_ms, StreamEncoding::Json, bytes))
    }

    pub fn publish_binary(&self, topic: impl Into<String>, key: impl Into<String>, sequence: u64, timestamp_ms: u64, bytes: Vec<u8>) -> StreamRecord {
        self.publish_bytes(topic, key, sequence, timestamp_ms, StreamEncoding::Binary, bytes)
    }

    pub fn publish_bytes(&self, topic: impl Into<String>, key: impl Into<String>, sequence: u64, timestamp_ms: u64, encoding: StreamEncoding, bytes: Vec<u8>) -> StreamRecord {
        let topic = topic.into();
        let frame = StreamFrame { sequence, topic: topic.clone(), key: key.into(), encoding, timestamp_ms, payload: Arc::<[u8]>::from(bytes) };
        let record = frame.record();
        let mut guard = self.topics.lock().expect("stream hub lock poisoned");
        let state = guard.entry(topic.clone()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(self.topic_capacity);
            TopicState { tx, recent: VecDeque::with_capacity(self.recent_limit.min(100_000)), published: 0, dropped: 0 }
        });
        state.published = state.published.saturating_add(1);
        if state.recent.len() >= self.recent_limit {
            state.recent.pop_front();
            state.dropped = state.dropped.saturating_add(1);
        }
        state.recent.push_back(record.clone());
        if state.tx.send(frame).is_err() {
            // No active subscribers is not an error; recent replay still receives the event.
        }
        record
    }

    pub fn subscribe(&self, topic: &str) -> broadcast::Receiver<StreamFrame> {
        let mut guard = self.topics.lock().expect("stream hub lock poisoned");
        let state = guard.entry(topic.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(self.topic_capacity);
            TopicState { tx, recent: VecDeque::with_capacity(self.recent_limit.min(100_000)), published: 0, dropped: 0 }
        });
        state.tx.subscribe()
    }

    pub fn recent(&self, topic: Option<&str>, limit: usize) -> Vec<StreamRecord> {
        let limit = limit.min(10_000);
        let guard = self.topics.lock().expect("stream hub lock poisoned");
        let mut records = Vec::new();
        for (name, state) in guard.iter() {
            if topic.map_or(true, |wanted| wanted == name) {
                records.extend(state.recent.iter().rev().take(limit).cloned());
            }
        }
        records.sort_by_key(|record| record.sequence);
        if records.len() > limit {
            records = records.split_off(records.len() - limit);
        }
        records
    }

    pub fn stats(&self) -> StreamStats {
        let guard = self.topics.lock().expect("stream hub lock poisoned");
        let mut published = 0u64;
        let mut dropped = 0u64;
        let mut topic_stats = Vec::with_capacity(guard.len());
        for (topic, state) in guard.iter() {
            published = published.saturating_add(state.published);
            dropped = dropped.saturating_add(state.dropped);
            topic_stats.push(TopicStats {
                topic: topic.clone(),
                published: state.published,
                dropped: state.dropped,
                subscribers: state.tx.receiver_count(),
                recent: state.recent.len(),
            });
        }
        StreamStats { topics: topic_stats.len(), published, dropped, topic_stats }
    }
}
