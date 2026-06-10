use async_trait::async_trait;
use gravity_market::{AdapterFrame, FrameNormalizer, MarketAdapter, MarketSender};
use gravity_types::{now_ms, Fixed, GravityError, MarketEvent, Price, Quantity, Side, Symbol, Ticker, Trade};
use serde_json::Value;
use std::str::FromStr;
use std::time::Duration;
use tokio::time;

#[derive(Clone, Debug)]
pub struct ExchangeSpec {
    pub venue: String,
    pub websocket_url: String,
    pub native_symbols: Vec<String>,
    pub normalized_symbols: Vec<Symbol>,
}

impl ExchangeSpec {
    pub fn binance() -> Result<Self, GravityError> {
        Ok(Self {
            venue: "binance".into(),
            websocket_url: "wss://stream.binance.com:9443/stream".into(),
            native_symbols: vec!["BTCUSDT".into(), "ETHUSDT".into()],
            normalized_symbols: vec![Symbol::new("BTC-USDx")?, Symbol::new("ETH-USDx")?],
        })
    }

    pub fn coinbase() -> Result<Self, GravityError> {
        Ok(Self {
            venue: "coinbase".into(),
            websocket_url: "wss://ws-feed.exchange.coinbase.com".into(),
            native_symbols: vec!["BTC-USD".into(), "ETH-USD".into()],
            normalized_symbols: vec![Symbol::new("BTC-USDx")?, Symbol::new("ETH-USDx")?],
        })
    }

    pub fn kraken() -> Result<Self, GravityError> {
        Ok(Self {
            venue: "kraken".into(),
            websocket_url: "wss://ws.kraken.com/v2".into(),
            native_symbols: vec!["BTC/USD".into(), "ETH/USD".into()],
            normalized_symbols: vec![Symbol::new("BTC-USDx")?, Symbol::new("ETH-USDx")?],
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VenueKind { Binance, Coinbase, Kraken }

impl VenueKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "binance" => Some(Self::Binance),
            "coinbase" | "coinbase-exchange" => Some(Self::Coinbase),
            "kraken" => Some(Self::Kraken),
            _ => None,
        }
    }

    pub fn spec(self) -> Result<ExchangeSpec, GravityError> {
        match self {
            Self::Binance => ExchangeSpec::binance(),
            Self::Coinbase => ExchangeSpec::coinbase(),
            Self::Kraken => ExchangeSpec::kraken(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExchangeReplayAdapter {
    spec: ExchangeSpec,
    kind: VenueKind,
    interval_ms: u64,
    sequence: u64,
}

impl ExchangeReplayAdapter {
    pub fn new(kind: VenueKind, interval_ms: u64) -> Result<Self, GravityError> {
        Ok(Self { spec: kind.spec()?, kind, interval_ms: interval_ms.max(50), sequence: 0 })
    }

    fn frame(&mut self, symbol_index: usize) -> AdapterFrame {
        self.sequence = self.sequence.saturating_add(1);
        let normalized = self.spec.normalized_symbols[symbol_index].clone();
        AdapterFrame {
            venue: self.spec.venue.clone(),
            channel: "ticker".into(),
            symbol: normalized,
            sequence: self.sequence,
            timestamp_ms: now_ms(),
            payload: sample_payload(self.kind, &self.spec.native_symbols[symbol_index], self.sequence),
        }
    }
}

#[async_trait]
impl MarketAdapter for ExchangeReplayAdapter {
    fn name(&self) -> &str { &self.spec.venue }

    async fn run(&mut self, sender: MarketSender) -> Result<(), GravityError> {
        let normalizer = ExchangeNormalizer::new(self.kind);
        let mut tick = time::interval(Duration::from_millis(self.interval_ms));
        tracing::info!(venue=%self.spec.venue, url=%self.spec.websocket_url, "exchange adapter started in replay mode");
        loop {
            tick.tick().await;
            for index in 0..self.spec.normalized_symbols.len() {
                for event in normalizer.normalize(self.frame(index)).await? {
                    sender.publish(event).await?;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExchangeNormalizer { kind: VenueKind }

impl ExchangeNormalizer {
    pub fn new(kind: VenueKind) -> Self { Self { kind } }
}

#[async_trait]
impl FrameNormalizer for ExchangeNormalizer {
    async fn normalize(&self, frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError> {
        match self.kind {
            VenueKind::Binance => normalize_binance(frame),
            VenueKind::Coinbase => normalize_coinbase(frame),
            VenueKind::Kraken => normalize_kraken(frame),
        }
    }
}

pub fn adapters_for_venues(venues: &[String], interval_ms: u64) -> Result<Vec<ExchangeReplayAdapter>, GravityError> {
    let mut out = Vec::new();
    for venue in venues {
        let Some(kind) = VenueKind::parse(venue) else {
            tracing::warn!(venue=%venue, "unsupported exchange adapter skipped");
            continue;
        };
        out.push(ExchangeReplayAdapter::new(kind, interval_ms)?);
    }
    if out.is_empty() {
        return Err(GravityError::InvalidConfig("no supported exchange adapters enabled".into()));
    }
    Ok(out)
}

fn normalize_binance(frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError> {
    let value: Value = serde_json::from_str(&frame.payload)?;
    let price = parse_price(value.get("p").or_else(|| value.get("c")))?;
    let qty = parse_quantity(value.get("q").or_else(|| value.get("Q")))?;
    let side = if value.get("m").and_then(Value::as_bool).unwrap_or(false) { Side::Sell } else { Side::Buy };
    Ok(vec![MarketEvent::Trade(Trade {
        symbol: frame.symbol,
        venue: frame.venue,
        price,
        quantity: qty,
        side,
        sequence: frame.sequence,
        timestamp_ms: value.get("T").and_then(Value::as_u64).unwrap_or(frame.timestamp_ms),
    })])
}

fn normalize_coinbase(frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError> {
    let value: Value = serde_json::from_str(&frame.payload)?;
    let price = parse_price(value.get("price"))?;
    let qty = parse_quantity(value.get("last_size").or_else(|| value.get("size")))?;
    Ok(vec![MarketEvent::Trade(Trade {
        symbol: frame.symbol,
        venue: frame.venue,
        price,
        quantity: qty,
        side: side_from_str(value.get("side").and_then(Value::as_str)),
        sequence: value.get("sequence").and_then(Value::as_u64).unwrap_or(frame.sequence),
        timestamp_ms: frame.timestamp_ms,
    })])
}

fn normalize_kraken(frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError> {
    let value: Value = serde_json::from_str(&frame.payload)?;
    let price = parse_price(value.get("price"))?;
    let qty = parse_quantity(value.get("qty").or_else(|| value.get("volume")))?;
    let side = side_from_str(value.get("side").and_then(Value::as_str));
    if value.get("kind").and_then(Value::as_str) == Some("ticker") {
        let bid = parse_price(value.get("bid")).unwrap_or(price);
        let ask = parse_price(value.get("ask")).unwrap_or(price);
        return Ok(vec![MarketEvent::Ticker(Ticker { symbol: frame.symbol, venue: frame.venue, bid, ask, last: price, sequence: frame.sequence, timestamp_ms: frame.timestamp_ms })]);
    }
    Ok(vec![MarketEvent::Trade(Trade { symbol: frame.symbol, venue: frame.venue, price, quantity: qty, side, sequence: frame.sequence, timestamp_ms: frame.timestamp_ms })])
}

fn parse_price(value: Option<&Value>) -> Result<Price, GravityError> { Price::new(parse_fixed(value)?) }
fn parse_quantity(value: Option<&Value>) -> Result<Quantity, GravityError> { Quantity::new(parse_fixed(value)?) }

fn parse_fixed(value: Option<&Value>) -> Result<Fixed, GravityError> {
    match value {
        Some(Value::String(v)) => Fixed::from_str(v),
        Some(Value::Number(v)) => Fixed::from_str(&v.to_string()),
        _ => Err(GravityError::Parse("missing fixed-point numeric field".into())),
    }
}

fn side_from_str(value: Option<&str>) -> Side {
    match value.unwrap_or("buy").to_ascii_lowercase().as_str() {
        "sell" | "s" => Side::Sell,
        _ => Side::Buy,
    }
}

fn sample_payload(kind: VenueKind, native: &str, sequence: u64) -> String {
    let base = if native.contains("ETH") { 3_400 } else { 67_000 };
    let price = base + (sequence % 17) as i64 - 8;
    let qty = if native.contains("ETH") { "1.250000" } else { "0.125000" };
    match kind {
        VenueKind::Binance => format!(r#"{{"e":"trade","s":"{native}","p":"{price}.000000","q":"{qty}","T":{},"t":{},"m":false}}"#, now_ms(), sequence),
        VenueKind::Coinbase => format!(r#"{{"type":"match","product_id":"{native}","price":"{price}.000000","last_size":"{qty}","sequence":{},"side":"buy"}}"#, sequence),
        VenueKind::Kraken => format!(r#"{{"kind":"trade","symbol":"{native}","price":"{price}.000000","qty":"{qty}","side":"buy"}}"#),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binance_normalizer_emits_trade() {
        let normalizer = ExchangeNormalizer::new(VenueKind::Binance);
        let frame = AdapterFrame::new("binance", "trade", Symbol::new("BTC-USDx").unwrap(), 1, r#"{"p":"67000.00","q":"0.10","T":1,"m":false}"#);
        let events = normalizer.normalize(frame).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind(), "trade");
    }
}
