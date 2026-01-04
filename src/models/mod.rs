use serde::{Deserialize, Serialize};

pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/hypermarket.clob.rs"));
}

pub type MarketId = u64;
pub type SubaccountId = u64;
pub type OrderId = u64;
pub type ShardId = usize;
pub type PriceTicks = u64;
pub type Quantity = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderType {
    Limit,
    Market,
    PostOnly,
    Ioc,
    Fok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeInForce {
    Gtc,
    Ioc,
    Fok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OrderStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOrder {
    pub request_id: String,
    pub market_id: MarketId,
    pub subaccount_id: SubaccountId,
    pub side: Side,
    pub order_type: OrderType,
    pub tif: TimeInForce,
    pub price_ticks: PriceTicks,
    pub qty: Quantity,
    pub reduce_only: bool,
    pub expiry_ts: u64,
    pub nonce: u64,
    pub client_ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrder {
    pub request_id: String,
    pub market_id: MarketId,
    pub subaccount_id: SubaccountId,
    pub order_id: Option<OrderId>,
    pub nonce_start: Option<u64>,
    pub nonce_end: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceUpdate {
    pub market_id: MarketId,
    pub mark_price: PriceTicks,
    pub index_price: PriceTicks,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingUpdate {
    pub market_id: MarketId,
    pub funding_index: i64,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAck {
    pub request_id: String,
    pub status: OrderStatus,
    pub reject_reason: Option<String>,
    pub assigned_order_id: Option<OrderId>,
    pub engine_seq: u64,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub market_id: MarketId,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub price_ticks: PriceTicks,
    pub qty: Quantity,
    pub maker_fee: i64,
    pub taker_fee: i64,
    pub engine_seq: u64,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookLevel {
    pub price_ticks: PriceTicks,
    pub qty: Quantity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDelta {
    pub market_id: MarketId,
    pub bids_levels: Vec<BookLevel>,
    pub asks_levels: Vec<BookLevel>,
    pub engine_seq: u64,
    pub ts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementBatch {
    pub batch_id: String,
    pub ts: u64,
    pub fills: Vec<Fill>,
    pub price_refs: String,
    pub funding_refs: String,
    pub state_root: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    NewOrder(NewOrder),
    CancelOrder(CancelOrder),
    PriceUpdate(PriceUpdate),
    FundingUpdate(FundingUpdate),
    OrderAck(OrderAck),
    Fill(Fill),
    BookDelta(BookDelta),
    SettlementBatch(SettlementBatch),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub shard_id: ShardId,
    pub engine_seq: u64,
    pub event: Event,
    pub ts: u64,
}

impl From<pb::NewOrder> for NewOrder {
    fn from(value: pb::NewOrder) -> Self {
        Self {
            request_id: value.request_id,
            market_id: value.market_id,
            subaccount_id: value.subaccount_id,
            side: match value.side.as_str() {
                "SELL" => Side::Sell,
                _ => Side::Buy,
            },
            order_type: match value.order_type.as_str() {
                "MARKET" => OrderType::Market,
                "POST_ONLY" => OrderType::PostOnly,
                "IOC" => OrderType::Ioc,
                "FOK" => OrderType::Fok,
                _ => OrderType::Limit,
            },
            tif: match value.tif.as_str() {
                "IOC" => TimeInForce::Ioc,
                "FOK" => TimeInForce::Fok,
                _ => TimeInForce::Gtc,
            },
            price_ticks: value.price_ticks,
            qty: value.qty,
            reduce_only: value.reduce_only,
            expiry_ts: value.expiry_ts,
            nonce: value.nonce,
            client_ts: value.client_ts,
        }
    }
}

impl From<pb::CancelOrder> for CancelOrder {
    fn from(value: pb::CancelOrder) -> Self {
        Self {
            request_id: value.request_id,
            market_id: value.market_id,
            subaccount_id: value.subaccount_id,
            order_id: if value.order_id == 0 { None } else { Some(value.order_id) },
            nonce_start: if value.nonce_start == 0 { None } else { Some(value.nonce_start) },
            nonce_end: if value.nonce_end == 0 { None } else { Some(value.nonce_end) },
        }
    }
}

impl From<pb::PriceUpdate> for PriceUpdate {
    fn from(value: pb::PriceUpdate) -> Self {
        Self {
            market_id: value.market_id,
            mark_price: value.mark_price,
            index_price: value.index_price,
            ts: value.ts,
        }
    }
}

impl From<pb::FundingUpdate> for FundingUpdate {
    fn from(value: pb::FundingUpdate) -> Self {
        Self {
            market_id: value.market_id,
            funding_index: value.funding_index,
            ts: value.ts,
        }
    }
}

impl From<OrderAck> for pb::OrderAck {
    fn from(value: OrderAck) -> Self {
        Self {
            request_id: value.request_id,
            status: match value.status {
                OrderStatus::Accepted => "ACCEPTED".to_string(),
                OrderStatus::Rejected => "REJECTED".to_string(),
            },
            reject_reason: value.reject_reason.unwrap_or_default(),
            assigned_order_id: value.assigned_order_id.unwrap_or_default(),
            engine_seq: value.engine_seq,
            ts: value.ts,
        }
    }
}

impl From<Fill> for pb::Fill {
    fn from(value: Fill) -> Self {
        Self {
            market_id: value.market_id,
            maker_order_id: value.maker_order_id,
            taker_order_id: value.taker_order_id,
            price_ticks: value.price_ticks,
            qty: value.qty,
            maker_fee: value.maker_fee,
            taker_fee: value.taker_fee,
            engine_seq: value.engine_seq,
            ts: value.ts,
        }
    }
}

impl From<BookDelta> for pb::BookDelta {
    fn from(value: BookDelta) -> Self {
        Self {
            market_id: value.market_id,
            bids_levels: value
                .bids_levels
                .into_iter()
                .map(|level| pb::BookLevel {
                    price_ticks: level.price_ticks,
                    qty: level.qty,
                })
                .collect(),
            asks_levels: value
                .asks_levels
                .into_iter()
                .map(|level| pb::BookLevel {
                    price_ticks: level.price_ticks,
                    qty: level.qty,
                })
                .collect(),
            engine_seq: value.engine_seq,
            ts: value.ts,
        }
    }
}

impl From<SettlementBatch> for pb::SettlementBatch {
    fn from(value: SettlementBatch) -> Self {
        Self {
            batch_id: value.batch_id,
            ts: value.ts,
            fills: value.fills.into_iter().map(Into::into).collect(),
            price_refs: value.price_refs,
            funding_refs: value.funding_refs,
            state_root: value.state_root,
        }
    }
}
