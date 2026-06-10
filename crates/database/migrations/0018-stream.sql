-- Gravity v2.8.0 production streaming metadata shell
CREATE TABLE IF NOT EXISTS stream_topics (
  topic TEXT PRIMARY KEY,
  enabled BOOLEAN NOT NULL DEFAULT TRUE,
  default_encoding TEXT NOT NULL DEFAULT 'json',
  recent_limit BIGINT NOT NULL DEFAULT 100000,
  created_at_ms BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS stream_events (
  sequence BIGINT NOT NULL,
  topic TEXT NOT NULL,
  stream_key TEXT NOT NULL,
  encoding TEXT NOT NULL,
  payload_len BIGINT NOT NULL,
  timestamp_ms BIGINT NOT NULL,
  PRIMARY KEY (topic, sequence)
);

CREATE INDEX IF NOT EXISTS idx_stream_events_topic_ts ON stream_events(topic, timestamp_ms DESC);
