use core::cmp::Ordering;
use core::fmt;
use core::ops::{Add, Div, Mul, Sub};
use core::str::FromStr;
use serde::{Deserialize, Serialize};

pub const SCALE: i128 = 1_000_000;
pub const BPS: i128 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset(pub String);

impl Asset {
    pub fn new(value: impl Into<String>) -> Result<Self, GravityError> {
        let value = value.into();
        validate_symbol_part(&value)?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(pub String);

impl Symbol {
    pub fn new(value: impl Into<String>) -> Result<Self, GravityError> {
        let value = value.into();
        if value.len() < 3 || value.len() > 32 {
            return Err(GravityError::InvalidSymbol(value));
        }
        if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(GravityError::InvalidSymbol(value));
        }
        Ok(Self(value))
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Market {
    pub symbol: Symbol,
    pub base: Asset,
    pub quote: Asset,
}

impl Market {
    pub fn new(symbol: impl Into<String>, base: impl Into<String>, quote: impl Into<String>) -> Result<Self, GravityError> {
        Ok(Self { symbol: Symbol::new(symbol)?, base: Asset::new(base)?, quote: Asset::new(quote)? })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side { Buy, Sell }

impl Side {
    pub fn as_str(self) -> &'static str { match self { Self::Buy => "buy", Self::Sell => "sell" } }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderKind { Limit, Market }

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce { Gtc, Ioc, Fok, PostOnly }

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Fixed(pub i128);

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(SCALE);

    pub fn raw(value: i128) -> Self { Self(value) }
    pub fn from_units(value: i128) -> Self { Self(value.saturating_mul(SCALE)) }
    pub fn as_raw(self) -> i128 { self.0 }
    pub fn abs(self) -> Self { Self(self.0.abs()) }

    pub fn checked_add(self, other: Self) -> Result<Self, GravityError> {
        self.0.checked_add(other.0).map(Self).ok_or(GravityError::Overflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, GravityError> {
        self.0.checked_sub(other.0).map(Self).ok_or(GravityError::Overflow)
    }

    pub fn checked_mul(self, other: Self) -> Result<Self, GravityError> {
        self.0.checked_mul(other.0).and_then(|v| v.checked_div(SCALE)).map(Self).ok_or(GravityError::Overflow)
    }

    pub fn checked_div(self, other: Self) -> Result<Self, GravityError> {
        if other.0 == 0 { return Err(GravityError::DivisionByZero); }
        self.0.checked_mul(SCALE).and_then(|v| v.checked_div(other.0)).map(Self).ok_or(GravityError::Overflow)
    }
}

impl FromStr for Fixed {
    type Err = GravityError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim();
        if s.is_empty() { return Err(GravityError::InvalidNumber(input.into())); }
        let neg = s.starts_with('-');
        let body = if neg { &s[1..] } else { s };
        let mut parts = body.split('.');
        let whole = parts.next().ok_or_else(|| GravityError::InvalidNumber(input.into()))?;
        let frac = parts.next().unwrap_or("0");
        if parts.next().is_some() || whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
            return Err(GravityError::InvalidNumber(input.into()));
        }
        let whole_value = whole.parse::<i128>().map_err(|_| GravityError::InvalidNumber(input.into()))?;
        let mut frac_string = frac.to_string();
        if frac_string.len() > 6 { frac_string.truncate(6); }
        while frac_string.len() < 6 { frac_string.push('0'); }
        let frac_value = frac_string.parse::<i128>().map_err(|_| GravityError::InvalidNumber(input.into()))?;
        let raw = whole_value.checked_mul(SCALE).and_then(|v| v.checked_add(frac_value)).ok_or(GravityError::Overflow)?;
        Ok(Self(if neg { -raw } else { raw }))
    }
}

impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let neg = self.0 < 0;
        let abs = self.0.abs();
        let whole = abs / SCALE;
        let frac = abs % SCALE;
        if frac == 0 {
            if neg { write!(f, "-{whole}") } else { write!(f, "{whole}") }
        } else {
            let mut frac_string = format!("{frac:06}");
            while frac_string.ends_with('0') { frac_string.pop(); }
            if neg { write!(f, "-{whole}.{frac_string}") } else { write!(f, "{whole}.{frac_string}") }
        }
    }
}

impl Add for Fixed { type Output = Self; fn add(self, rhs: Self) -> Self::Output { Self(self.0.saturating_add(rhs.0)) } }
impl Sub for Fixed { type Output = Self; fn sub(self, rhs: Self) -> Self::Output { Self(self.0.saturating_sub(rhs.0)) } }
impl Mul for Fixed { type Output = Self; fn mul(self, rhs: Self) -> Self::Output { Self(self.0.saturating_mul(rhs.0) / SCALE) } }
impl Div for Fixed { type Output = Self; fn div(self, rhs: Self) -> Self::Output { if rhs.0 == 0 { Self::ZERO } else { Self(self.0.saturating_mul(SCALE) / rhs.0) } } }

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Price(pub Fixed);

impl Price {
    pub fn new(value: Fixed) -> Result<Self, GravityError> {
        if value.0 <= 0 { return Err(GravityError::InvalidPrice); }
        Ok(Self(value))
    }
}

impl fmt::Display for Price { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) } }

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Quantity(pub Fixed);

impl Quantity {
    pub fn new(value: Fixed) -> Result<Self, GravityError> {
        if value.0 <= 0 { return Err(GravityError::InvalidQuantity); }
        Ok(Self(value))
    }
}

impl fmt::Display for Quantity { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.0) } }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub symbol: Symbol,
    pub venue: String,
    pub price: Price,
    pub quantity: Quantity,
    pub side: Side,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: Symbol,
    pub venue: String,
    pub bid: Price,
    pub ask: Price,
    pub last: Price,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: Price,
    pub quantity: Quantity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBookDelta {
    pub symbol: Symbol,
    pub venue: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: Symbol,
    pub venue: String,
    pub rate_bps: i64,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenInterest {
    pub symbol: Symbol,
    pub venue: String,
    pub quantity: Quantity,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Liquidation {
    pub symbol: Symbol,
    pub venue: String,
    pub price: Price,
    pub quantity: Quantity,
    pub side: Side,
    pub sequence: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleReport {
    pub symbol: Symbol,
    pub price: Price,
    pub confidence_bps: u32,
    pub sources: u32,
    pub method: String,
    pub timestamp_ms: u64,
    pub key_id: Option<String>,
    pub payload_hash: String,
    pub signature: Option<String>,
}

impl OracleReport {
    pub fn signing_payload(&self) -> String {
        format!(
            "symbol={};price={};confidence_bps={};sources={};method={};timestamp_ms={};payload_hash={}",
            self.symbol, self.price, self.confidence_bps, self.sources, self.method, self.timestamp_ms, self.payload_hash
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum MarketEvent {
    Trade(Trade),
    Ticker(Ticker),
    OrderBookDelta(OrderBookDelta),
    FundingRate(FundingRate),
    OpenInterest(OpenInterest),
    Liquidation(Liquidation),
}

impl MarketEvent {
    pub fn symbol(&self) -> &Symbol {
        match self {
            Self::Trade(v) => &v.symbol,
            Self::Ticker(v) => &v.symbol,
            Self::OrderBookDelta(v) => &v.symbol,
            Self::FundingRate(v) => &v.symbol,
            Self::OpenInterest(v) => &v.symbol,
            Self::Liquidation(v) => &v.symbol,
        }
    }

    pub fn venue(&self) -> &str {
        match self {
            Self::Trade(v) => &v.venue,
            Self::Ticker(v) => &v.venue,
            Self::OrderBookDelta(v) => &v.venue,
            Self::FundingRate(v) => &v.venue,
            Self::OpenInterest(v) => &v.venue,
            Self::Liquidation(v) => &v.venue,
        }
    }

    pub fn sequence(&self) -> u64 {
        match self {
            Self::Trade(v) => v.sequence,
            Self::Ticker(v) => v.sequence,
            Self::OrderBookDelta(v) => v.sequence,
            Self::FundingRate(v) => v.sequence,
            Self::OpenInterest(v) => v.sequence,
            Self::Liquidation(v) => v.sequence,
        }
    }

    pub fn timestamp_ms(&self) -> u64 {
        match self {
            Self::Trade(v) => v.timestamp_ms,
            Self::Ticker(v) => v.timestamp_ms,
            Self::OrderBookDelta(v) => v.timestamp_ms,
            Self::FundingRate(v) => v.timestamp_ms,
            Self::OpenInterest(v) => v.timestamp_ms,
            Self::Liquidation(v) => v.timestamp_ms,
        }
    }

    pub fn price(&self) -> Option<Price> {
        match self {
            Self::Trade(v) => Some(v.price),
            Self::Ticker(v) => Some(v.last),
            Self::OrderBookDelta(v) => mid_price(&v.bids, &v.asks),
            Self::Liquidation(v) => Some(v.price),
            Self::FundingRate(_) | Self::OpenInterest(_) => None,
        }
    }

    pub fn quantity(&self) -> Option<Quantity> {
        match self {
            Self::Trade(v) => Some(v.quantity),
            Self::OpenInterest(v) => Some(v.quantity),
            Self::Liquidation(v) => Some(v.quantity),
            Self::Ticker(_) | Self::OrderBookDelta(_) | Self::FundingRate(_) => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Trade(_) => "trade",
            Self::Ticker(_) => "ticker",
            Self::OrderBookDelta(_) => "book_delta",
            Self::FundingRate(_) => "funding_rate",
            Self::OpenInterest(_) => "open_interest",
            Self::Liquidation(_) => "liquidation",
        }
    }
}

fn mid_price(bids: &[OrderBookLevel], asks: &[OrderBookLevel]) -> Option<Price> {
    let bid = bids.iter().map(|v| v.price).max();
    let ask = asks.iter().map(|v| v.price).min();
    match (bid, ask) {
        (Some(bid), Some(ask)) => Price::new((bid.0 + ask.0) / Fixed::from_units(2)).ok(),
        (Some(price), None) | (None, Some(price)) => Some(price),
        (None, None) => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementPayload {
    pub kind: String,
    pub symbol: Symbol,
    pub body: String,
    pub idempotency: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettlementReceipt {
    pub accepted: bool,
    pub reference: String,
    pub message: String,
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: String,
    pub kind: String,
    pub target: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub payload_hash: String,
    pub message: String,
}

impl AuditRecord {
    pub fn new(kind: impl Into<String>, target: impl Into<String>, sequence: u64, message: impl Into<String>) -> Self {
        let kind = kind.into();
        let target = target.into();
        let timestamp_ms = now_ms();
        let message = message.into();
        let seed = format!("{kind}:{target}:{sequence}:{timestamp_ms}:{message}");
        Self {
            id: stable_hash_hex(&seed),
            kind,
            target,
            sequence,
            timestamp_ms,
            payload_hash: stable_hash_hex(&seed),
            message,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub service: String,
    pub version: String,
    pub storage: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GravityError {
    InvalidSymbol(String),
    InvalidNumber(String),
    InvalidPrice,
    InvalidQuantity,
    InvalidConfig(String),
    ChannelClosed,
    Overflow,
    DivisionByZero,
    Io(String),
    NotFound(String),
    Parse(String),
    Database(String),
    Cache(String),
    Network(String),
}

impl fmt::Display for GravityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSymbol(v) => write!(f, "invalid symbol: {v}"),
            Self::InvalidNumber(v) => write!(f, "invalid fixed-point number: {v}"),
            Self::InvalidPrice => write!(f, "price must be positive"),
            Self::InvalidQuantity => write!(f, "quantity must be positive"),
            Self::InvalidConfig(v) => write!(f, "invalid config: {v}"),
            Self::ChannelClosed => write!(f, "channel closed"),
            Self::Overflow => write!(f, "arithmetic overflow"),
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::Io(v) => write!(f, "io error: {v}"),
            Self::NotFound(v) => write!(f, "not found: {v}"),
            Self::Parse(v) => write!(f, "parse error: {v}"),
            Self::Database(v) => write!(f, "database error: {v}"),
            Self::Cache(v) => write!(f, "cache error: {v}"),
            Self::Network(v) => write!(f, "network error: {v}"),
        }
    }
}

impl std::error::Error for GravityError {}

impl From<std::io::Error> for GravityError {
    fn from(value: std::io::Error) -> Self { Self::Io(value.to_string()) }
}

impl From<serde_json::Error> for GravityError {
    fn from(value: serde_json::Error) -> Self { Self::Parse(value.to_string()) }
}

fn validate_symbol_part(value: &str) -> Result<(), GravityError> {
    if value.len() < 2 || value.len() > 16 || !value.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(GravityError::InvalidSymbol(value.into()));
    }
    Ok(())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .unwrap_or(0)
}

pub fn median_price(mut values: Vec<Price>) -> Option<Price> {
    if values.is_empty() { return None; }
    values.sort_by(|a, b| a.cmp(b));
    Some(values[values.len() / 2])
}

pub fn weighted_price(values: &[(Price, Quantity)]) -> Option<Price> {
    if values.is_empty() { return None; }
    let mut notional = Fixed::ZERO;
    let mut quantity = Fixed::ZERO;
    for (price, qty) in values {
        notional = notional + (price.0 * qty.0);
        quantity = quantity + qty.0;
    }
    if quantity.0 == 0 { return None; }
    Price::new(notional / quantity).ok()
}

pub fn deviation_bps(reference: Price, value: Price) -> u32 {
    if reference.0.0 == 0 { return u32::MAX; }
    let delta = match value.cmp(&reference) {
        Ordering::Greater => value.0 - reference.0,
        Ordering::Less => reference.0 - value.0,
        Ordering::Equal => Fixed::ZERO,
    };
    ((delta.0.saturating_mul(BPS) / reference.0.0) as u32).min(u32::MAX)
}

pub fn stable_hash_hex(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub fn error_json(err: impl fmt::Display) -> serde_json::Value {
    serde_json::json!({ "error": err.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_parse_roundtrip() {
        let v: Fixed = "123.456789".parse().unwrap();
        assert_eq!(v.to_string(), "123.456789");
    }

    #[test]
    fn price_validation() {
        assert!(Price::new(Fixed::from_units(1)).is_ok());
        assert!(Price::new(Fixed::ZERO).is_err());
    }

    #[test]
    fn weighted_price_uses_quantity() {
        let a = Price::new(Fixed::from_units(100)).unwrap();
        let b = Price::new(Fixed::from_units(200)).unwrap();
        let q1 = Quantity::new(Fixed::from_units(1)).unwrap();
        let q3 = Quantity::new(Fixed::from_units(3)).unwrap();
        assert_eq!(weighted_price(&[(a, q1), (b, q3)]).unwrap().to_string(), "175");
    }
}
