use gravity_types::{Fixed, GravityError, OrderKind, Price, Quantity, Side, Symbol, TimeInForce, now_ms};

const MAGIC: &[u8; 4] = b"GVW1";
const VERSION: u8 = 1;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    Order = 1,
    Fill = 2,
    Oracle = 3,
    Settlement = 4,
    Heartbeat = 5,
    OrderBatch = 6,
}

impl FrameKind {
    fn from_u8(value: u8) -> Result<Self, GravityError> {
        match value {
            1 => Ok(Self::Order),
            2 => Ok(Self::Fill),
            3 => Ok(Self::Oracle),
            4 => Ok(Self::Settlement),
            5 => Ok(Self::Heartbeat),
            6 => Ok(Self::OrderBatch),
            other => Err(GravityError::InvalidConfig(format!("unknown wire frame kind: {other}"))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireFrame {
    pub kind: FrameKind,
    pub flags: u8,
    pub sequence: u64,
    pub timestamp_ms: u64,
    pub payload: Vec<u8>,
}

impl WireFrame {
    pub fn new(kind: FrameKind, sequence: u64, payload: Vec<u8>) -> Self {
        Self { kind, flags: 0, sequence, timestamp_ms: now_ms(), payload }
    }

    pub fn encode(&self) -> Result<Vec<u8>, GravityError> {
        if self.payload.len() > u32::MAX as usize {
            return Err(GravityError::InvalidConfig("wire payload too large".into()));
        }
        let mut out = Vec::with_capacity(27 + self.payload.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.push(self.kind as u8);
        out.push(self.flags);
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    pub fn decode(input: &[u8]) -> Result<Self, GravityError> {
        let mut cur = Cursor::new(input);
        let magic = cur.take_exact(4)?;
        if magic != &MAGIC[..] { return Err(GravityError::InvalidConfig("invalid wire magic".into())); }
        let version = cur.u8()?;
        if version != VERSION { return Err(GravityError::InvalidConfig(format!("unsupported wire version: {version}"))); }
        let kind = FrameKind::from_u8(cur.u8()?)?;
        let flags = cur.u8()?;
        let sequence = cur.u64()?;
        let timestamp_ms = cur.u64()?;
        let payload_len = cur.u32()? as usize;
        let payload = cur.take_exact(payload_len)?.to_vec();
        if !cur.is_empty() { return Err(GravityError::InvalidConfig("trailing bytes in wire frame".into())); }
        Ok(Self { kind, flags, sequence, timestamp_ms, payload })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderWire {
    pub symbol: Symbol,
    pub account: String,
    pub side: Side,
    pub kind: OrderKind,
    pub tif: TimeInForce,
    pub price_raw: i128,
    pub quantity_raw: i128,
    pub client_id: Option<String>,
    pub sequence: u64,
}

impl OrderWire {
    pub fn price(&self) -> Result<Option<Price>, GravityError> {
        if self.kind == OrderKind::Market { Ok(None) } else { Ok(Some(Price::new(Fixed::raw(self.price_raw))?)) }
    }

    pub fn quantity(&self) -> Result<Quantity, GravityError> { Quantity::new(Fixed::raw(self.quantity_raw)) }
}

pub fn encode_order_message(symbol: &Symbol, account: &str, side: Side, price: Price, quantity: Quantity, sequence: u64) -> Result<Vec<u8>, GravityError> {
    encode_order_request(symbol, account, side, OrderKind::Limit, TimeInForce::Gtc, Some(price), quantity, None, sequence)
}

pub fn encode_order_request(
    symbol: &Symbol,
    account: &str,
    side: Side,
    kind: OrderKind,
    tif: TimeInForce,
    price: Option<Price>,
    quantity: Quantity,
    client_id: Option<&str>,
    sequence: u64,
) -> Result<Vec<u8>, GravityError> {
    let payload = encode_order_payload(symbol, account, side, kind, tif, price, quantity, client_id)?;
    WireFrame::new(FrameKind::Order, sequence, payload).encode()
}

pub fn decode_order_message(input: &[u8]) -> Result<OrderWire, GravityError> {
    let frame = WireFrame::decode(input)?;
    if frame.kind != FrameKind::Order { return Err(GravityError::InvalidConfig("wire frame is not an order".into())); }
    decode_order_payload(&frame.payload, frame.sequence)
}

pub fn encode_order_batch(orders: &[OrderWire], sequence: u64) -> Result<Vec<u8>, GravityError> {
    if orders.len() > u32::MAX as usize { return Err(GravityError::InvalidConfig("wire order batch too large".into())); }
    let mut payload = Vec::with_capacity(4 + orders.len() * 80);
    payload.extend_from_slice(&(orders.len() as u32).to_le_bytes());
    for order in orders {
        let price = if order.kind == OrderKind::Market { None } else { Some(Price::new(Fixed::raw(order.price_raw))?) };
        let quantity = Quantity::new(Fixed::raw(order.quantity_raw))?;
        let item = encode_order_payload(&order.symbol, &order.account, order.side, order.kind, order.tif, price, quantity, order.client_id.as_deref())?;
        if item.len() > u32::MAX as usize { return Err(GravityError::InvalidConfig("wire order item too large".into())); }
        payload.extend_from_slice(&(item.len() as u32).to_le_bytes());
        payload.extend_from_slice(&item);
    }
    WireFrame::new(FrameKind::OrderBatch, sequence, payload).encode()
}

pub fn decode_order_batch(input: &[u8]) -> Result<Vec<OrderWire>, GravityError> {
    let frame = WireFrame::decode(input)?;
    if frame.kind != FrameKind::OrderBatch { return Err(GravityError::InvalidConfig("wire frame is not an order batch".into())); }
    let mut cur = Cursor::new(&frame.payload);
    let count = cur.u32()? as usize;
    if count > 1_000_000 { return Err(GravityError::InvalidConfig("wire order batch count too large".into())); }
    let mut orders = Vec::with_capacity(count);
    for index in 0..count {
        let len = cur.u32()? as usize;
        let payload = cur.take_exact(len)?;
        orders.push(decode_order_payload(payload, frame.sequence.saturating_add(index as u64))?);
    }
    if !cur.is_empty() { return Err(GravityError::InvalidConfig("trailing bytes in order batch payload".into())); }
    Ok(orders)
}

fn encode_order_payload(
    symbol: &Symbol,
    account: &str,
    side: Side,
    kind: OrderKind,
    tif: TimeInForce,
    price: Option<Price>,
    quantity: Quantity,
    client_id: Option<&str>,
) -> Result<Vec<u8>, GravityError> {
    let mut payload = Vec::with_capacity(112);
    write_string(&mut payload, &symbol.0)?;
    write_string(&mut payload, account)?;
    payload.push(match side { Side::Buy => 0, Side::Sell => 1 });
    payload.push(match kind { OrderKind::Limit => 0, OrderKind::Market => 1 });
    payload.push(match tif { TimeInForce::Gtc => 0, TimeInForce::Ioc => 1, TimeInForce::Fok => 2, TimeInForce::PostOnly => 3 });
    payload.extend_from_slice(&price.map_or(0_i128, |p| p.0.as_raw()).to_le_bytes());
    payload.extend_from_slice(&quantity.0.as_raw().to_le_bytes());
    match client_id {
        Some(value) => { payload.push(1); write_string(&mut payload, value)?; }
        None => payload.push(0),
    }
    Ok(payload)
}

fn decode_order_payload(payload: &[u8], sequence: u64) -> Result<OrderWire, GravityError> {
    let mut cur = Cursor::new(payload);
    let symbol = Symbol::new(cur.string()?)?;
    let account = cur.string()?;
    let side = match cur.u8()? {
        0 => Side::Buy,
        1 => Side::Sell,
        other => return Err(GravityError::InvalidConfig(format!("invalid order side byte: {other}"))),
    };
    let kind = match cur.u8()? {
        0 => OrderKind::Limit,
        1 => OrderKind::Market,
        other => return Err(GravityError::InvalidConfig(format!("invalid order kind byte: {other}"))),
    };
    let tif = match cur.u8()? {
        0 => TimeInForce::Gtc,
        1 => TimeInForce::Ioc,
        2 => TimeInForce::Fok,
        3 => TimeInForce::PostOnly,
        other => return Err(GravityError::InvalidConfig(format!("invalid order tif byte: {other}"))),
    };
    let price_raw = cur.i128()?;
    let quantity_raw = cur.i128()?;
    let has_client = cur.u8()?;
    let client_id = match has_client {
        0 => None,
        1 => Some(cur.string()?),
        other => return Err(GravityError::InvalidConfig(format!("invalid client id flag: {other}"))),
    };
    if !cur.is_empty() { return Err(GravityError::InvalidConfig("trailing bytes in order wire payload".into())); }
    Ok(OrderWire { symbol, account, side, kind, tif, price_raw, quantity_raw, client_id, sequence })
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), GravityError> {
    let bytes = value.as_bytes();
    if bytes.len() > u16::MAX as usize { return Err(GravityError::InvalidConfig("wire string too long".into())); }
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

struct Cursor<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a [u8]) -> Self { Self { input, pos: 0 } }
    fn is_empty(&self) -> bool { self.pos == self.input.len() }

    fn take_exact(&mut self, len: usize) -> Result<&'a [u8], GravityError> {
        let end = self.pos.checked_add(len).ok_or(GravityError::Overflow)?;
        if end > self.input.len() { return Err(GravityError::InvalidConfig("truncated wire frame".into())); }
        let slice = &self.input[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, GravityError> { Ok(*self.take_exact(1)?.first().ok_or_else(|| GravityError::InvalidConfig("missing byte".into()))?) }
    fn u16(&mut self) -> Result<u16, GravityError> { let mut b = [0_u8; 2]; b.copy_from_slice(self.take_exact(2)?); Ok(u16::from_le_bytes(b)) }
    fn u32(&mut self) -> Result<u32, GravityError> { let mut b = [0_u8; 4]; b.copy_from_slice(self.take_exact(4)?); Ok(u32::from_le_bytes(b)) }
    fn u64(&mut self) -> Result<u64, GravityError> { let mut b = [0_u8; 8]; b.copy_from_slice(self.take_exact(8)?); Ok(u64::from_le_bytes(b)) }
    fn i128(&mut self) -> Result<i128, GravityError> { let mut b = [0_u8; 16]; b.copy_from_slice(self.take_exact(16)?); Ok(i128::from_le_bytes(b)) }

    fn string(&mut self) -> Result<String, GravityError> {
        let len = self.u16()? as usize;
        let bytes = self.take_exact(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|err| GravityError::InvalidConfig(format!("invalid utf8 wire string: {err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_wire_round_trip() {
        let symbol = Symbol::new("BTC-USDx").unwrap();
        let price = Price::new(Fixed::from_units(100_000)).unwrap();
        let qty = Quantity::new("0.01".parse::<Fixed>().unwrap()).unwrap();
        let bytes = encode_order_message(&symbol, "acct", Side::Buy, price, qty, 42).unwrap();
        let decoded = decode_order_message(&bytes).unwrap();
        assert_eq!(decoded.symbol, symbol);
        assert_eq!(decoded.account, "acct");
        assert_eq!(decoded.side, Side::Buy);
        assert_eq!(decoded.kind, OrderKind::Limit);
        assert_eq!(decoded.tif, TimeInForce::Gtc);
        assert_eq!(decoded.price_raw, price.0.as_raw());
        assert_eq!(decoded.quantity_raw, qty.0.as_raw());
        assert_eq!(decoded.sequence, 42);
    }

    #[test]
    fn order_batch_round_trip() {
        let symbol = Symbol::new("ETH-USDx").unwrap();
        let orders = vec![OrderWire { symbol, account: "acct".into(), side: Side::Sell, kind: OrderKind::Limit, tif: TimeInForce::Ioc, price_raw: Fixed::from_units(2500).as_raw(), quantity_raw: Fixed::from_units(1).as_raw(), client_id: Some("c1".into()), sequence: 9 }];
        let bytes = encode_order_batch(&orders, 9).unwrap();
        let decoded = decode_order_batch(&bytes).unwrap();
        assert_eq!(decoded, orders);
    }
}
