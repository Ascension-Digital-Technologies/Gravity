use async_trait::async_trait;
use gravity_types::{now_ms, Fixed, FundingRate, GravityError, Liquidation, MarketEvent, OpenInterest, OrderBookDelta, OrderBookLevel, Price, Quantity, Side, Symbol, Ticker, Trade};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;

#[derive(Clone)]
pub struct MarketSender { tx: mpsc::Sender<MarketEvent> }

impl MarketSender {
    pub async fn publish(&self, event: MarketEvent) -> Result<(), GravityError> {
        self.tx.send(event).await.map_err(|_| GravityError::ChannelClosed)
    }

    pub fn try_publish(&self, event: MarketEvent) -> Result<(), GravityError> {
        self.tx.try_send(event).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => GravityError::InvalidConfig("market bus is full; apply backpressure".into()),
            mpsc::error::TrySendError::Closed(_) => GravityError::ChannelClosed,
        })
    }
}

pub type MarketReceiver = mpsc::Receiver<MarketEvent>;

pub fn market_bus(capacity: usize) -> (MarketSender, MarketReceiver) {
    let (tx, rx) = mpsc::channel(capacity);
    (MarketSender { tx }, rx)
}

#[derive(Clone, Debug, Default)]
pub struct SequenceTracker { last: BTreeMap<String, u64> }

impl SequenceTracker {
    pub fn accept(&mut self, venue: &str, symbol: &str, sequence: u64) -> SequenceVerdict {
        let key = format!("{venue}:{symbol}");
        let Some(last) = self.last.get_mut(&key) else {
            self.last.insert(key, sequence);
            return SequenceVerdict::Accepted;
        };
        if sequence <= *last { return SequenceVerdict::DuplicateOrOld; }
        let gap = sequence.saturating_sub(*last).saturating_sub(1);
        *last = sequence;
        if gap > 0 { SequenceVerdict::Gap(gap) } else { SequenceVerdict::Accepted }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceVerdict { Accepted, DuplicateOrOld, Gap(u64) }

#[derive(Clone, Debug)]
pub struct FeedMonitor {
    tracker: SequenceTracker,
    max_gap: u64,
    accepted: u64,
    rejected: u64,
    gaps: u64,
}

impl Default for FeedMonitor {
    fn default() -> Self { Self { tracker: SequenceTracker::default(), max_gap: 100, accepted: 0, rejected: 0, gaps: 0 } }
}

impl FeedMonitor {
    pub fn with_max_gap(max_gap: u64) -> Self { Self { max_gap, ..Self::default() } }

    pub fn accept(&mut self, event: &MarketEvent) -> bool {
        match self.tracker.accept(event.venue(), &event.symbol().0, event.sequence()) {
            SequenceVerdict::Accepted => { self.accepted += 1; true }
            SequenceVerdict::Gap(gap) if gap <= self.max_gap => { self.accepted += 1; self.gaps += 1; true }
            SequenceVerdict::Gap(_) | SequenceVerdict::DuplicateOrOld => { self.rejected += 1; false }
        }
    }

    pub fn stats(&self) -> FeedStats { FeedStats { accepted: self.accepted, rejected: self.rejected, gaps: self.gaps } }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FeedStats { pub accepted: u64, pub rejected: u64, pub gaps: u64 }

#[derive(Clone, Debug)]
pub struct AdapterFrame {
    pub venue: String,
    pub channel: String,
    pub symbol: Symbol,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub payload: String,
}

impl AdapterFrame {
    pub fn new(venue: impl Into<String>, channel: impl Into<String>, symbol: Symbol, sequence: u64, payload: impl Into<String>) -> Self {
        Self { venue: venue.into(), channel: channel.into(), symbol, sequence, timestamp_ms: now_ms(), payload: payload.into() }
    }
}

#[derive(Clone, Debug)]
pub enum AdapterState { Starting, Ready, Reconnecting, Stale, Stopped }

#[async_trait]
pub trait MarketAdapter: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&mut self, sender: MarketSender) -> Result<(), GravityError>;
}

#[async_trait]
pub trait FrameNormalizer: Send + Sync {
    async fn normalize(&self, frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError>;
}

#[derive(Clone, Debug, Default)]
pub struct JsonFrameNormalizer;

#[async_trait]
impl FrameNormalizer for JsonFrameNormalizer {
    async fn normalize(&self, frame: AdapterFrame) -> Result<Vec<MarketEvent>, GravityError> {
        let price = frame.payload.parse::<Fixed>().unwrap_or_else(|_| Fixed::from_units(1));
        let price = Price::new(price)?;
        let quantity = Quantity::new(Fixed::from_units(1))?;
        Ok(vec![MarketEvent::Trade(Trade {
            symbol: frame.symbol,
            venue: frame.venue,
            price,
            quantity,
            side: Side::Buy,
            sequence: frame.sequence,
            timestamp_ms: frame.timestamp_ms,
        })])
    }
}

#[derive(Clone, Debug)]
pub struct MockFeed {
    symbol: Symbol,
    venue: String,
    price: Fixed,
    sequence: u64,
}

impl MockFeed {
    pub fn new(symbol: Symbol, venue: impl Into<String>, price: Fixed) -> Self {
        Self { symbol, venue: venue.into(), price, sequence: 0 }
    }

    pub fn event(&mut self) -> Result<MarketEvent, GravityError> {
        self.sequence = self.sequence.saturating_add(1);
        let drift = Fixed::raw(((self.sequence % 11) as i128 - 5) * 1000);
        let price = Price::new(self.price + drift)?;
        let quantity = Quantity::new(Fixed::from_units(1))?;
        Ok(MarketEvent::Trade(Trade { symbol: self.symbol.clone(), venue: self.venue.clone(), price, quantity, side: Side::Buy, sequence: self.sequence, timestamp_ms: now_ms() }))
    }
}

#[derive(Clone, Debug)]
pub struct MockAdapter {
    name: String,
    interval_ms: u64,
    feeds: Vec<MockFeed>,
}

impl MockAdapter {
    pub fn new(name: impl Into<String>, interval_ms: u64, feeds: Vec<MockFeed>) -> Self { Self { name: name.into(), interval_ms, feeds } }

    pub fn default_set() -> Result<Vec<Self>, GravityError> {
        Ok(vec![Self::new("mock", 250, vec![
            MockFeed::new(Symbol::new("BTC-USDx")?, "binance", Fixed::from_units(67_000)),
            MockFeed::new(Symbol::new("BTC-USDx")?, "coinbase", Fixed::from_units(67_005)),
            MockFeed::new(Symbol::new("BTC-USDx")?, "kraken", Fixed::from_units(66_995)),
            MockFeed::new(Symbol::new("ETH-USDx")?, "binance", Fixed::from_units(3_400)),
            MockFeed::new(Symbol::new("ETH-USDx")?, "coinbase", Fixed::from_units(3_402)),
            MockFeed::new(Symbol::new("ETH-USDx")?, "kraken", Fixed::from_units(3_398)),
        ])])
    }
}

#[async_trait]
impl MarketAdapter for MockAdapter {
    fn name(&self) -> &str { &self.name }

    async fn run(&mut self, sender: MarketSender) -> Result<(), GravityError> {
        let mut tick = time::interval(Duration::from_millis(self.interval_ms.max(10)));
        loop {
            tick.tick().await;
            for feed in &mut self.feeds {
                sender.publish(feed.event()?).await?;
            }
        }
    }
}

pub fn sample_book_delta(symbol: Symbol, venue: impl Into<String>, sequence: u64) -> Result<MarketEvent, GravityError> {
    Ok(MarketEvent::OrderBookDelta(OrderBookDelta {
        symbol,
        venue: venue.into(),
        bids: vec![OrderBookLevel { price: Price::new(Fixed::from_units(100))?, quantity: Quantity::new(Fixed::from_units(1))? }],
        asks: vec![OrderBookLevel { price: Price::new(Fixed::from_units(101))?, quantity: Quantity::new(Fixed::from_units(1))? }],
        sequence,
        timestamp_ms: now_ms(),
    }))
}

pub fn sample_funding(symbol: Symbol, venue: impl Into<String>, sequence: u64) -> MarketEvent {
    MarketEvent::FundingRate(FundingRate { symbol, venue: venue.into(), rate_bps: 1, sequence, timestamp_ms: now_ms() })
}

pub fn sample_interest(symbol: Symbol, venue: impl Into<String>, sequence: u64) -> Result<MarketEvent, GravityError> {
    Ok(MarketEvent::OpenInterest(OpenInterest { symbol, venue: venue.into(), quantity: Quantity::new(Fixed::from_units(1000))?, sequence, timestamp_ms: now_ms() }))
}

pub fn sample_liquidation(symbol: Symbol, venue: impl Into<String>, sequence: u64) -> Result<MarketEvent, GravityError> {
    Ok(MarketEvent::Liquidation(Liquidation { symbol, venue: venue.into(), price: Price::new(Fixed::from_units(100))?, quantity: Quantity::new(Fixed::from_units(1))?, side: Side::Sell, sequence, timestamp_ms: now_ms() }))
}

pub fn sample_ticker(symbol: Symbol, venue: impl Into<String>, sequence: u64) -> Result<MarketEvent, GravityError> {
    Ok(MarketEvent::Ticker(Ticker { symbol, venue: venue.into(), bid: Price::new(Fixed::from_units(100))?, ask: Price::new(Fixed::from_units(101))?, last: Price::new(Fixed::from_units(100))?, sequence, timestamp_ms: now_ms() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_sequence() {
        let mut monitor = FeedMonitor::default();
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let trade = MarketEvent::Trade(Trade { symbol, venue: "test".into(), price: Price::new(Fixed::from_units(10)).unwrap(), quantity: Quantity::new(Fixed::from_units(1)).unwrap(), side: Side::Buy, sequence: 1, timestamp_ms: now_ms() });
        assert!(monitor.accept(&trade));
        assert!(!monitor.accept(&trade));
    }
}
