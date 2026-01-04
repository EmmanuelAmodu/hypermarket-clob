use std::collections::{HashMap, VecDeque};

use lru::LruCache;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::config::{MarketConfig, MatchingMode};
use crate::matching::batch::BatchAuction;
use crate::matching::orderbook::{IncomingOrder, OrderBook};
use crate::models::{
    BookDelta, BookLevel, CancelOrder, Event, EventEnvelope, Fill, MarketId, NewOrder, OrderAck,
    OrderId, OrderStatus, PriceTicks, Side, TimeInForce,
};
use crate::persistence::wal::Wal;
use crate::risk::{RiskEngine, RiskError, RiskState};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OrderSnapshot {
    pub order_id: OrderId,
    pub subaccount_id: u64,
    pub side: Side,
    pub price_ticks: PriceTicks,
    pub remaining: u64,
    pub ingress_seq: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineState {
    pub shard_id: usize,
    pub engine_seq: u64,
    pub next_order_id: u64,
    pub orderbooks: HashMap<MarketId, Vec<OrderSnapshot>>,
    pub risk_state: RiskState,
}

struct MarketState {
    config: MarketConfig,
    book: OrderBook,
    batch: BatchAuction,
    pending: VecDeque<IncomingOrder>,
}

pub struct EngineShard {
    pub shard_id: usize,
    pub engine_seq: u64,
    pub next_order_id: u64,
    pub markets: HashMap<MarketId, MarketState>,
    pub risk: RiskEngine,
    pub wal: Wal,
    pub dedupe: LruCache<String, ()>,
    pub order_owners: HashMap<OrderId, (u64, Side)>,
}

impl EngineShard {
    pub fn new(shard_id: usize, markets: Vec<MarketConfig>, wal: Wal, mut risk: RiskEngine) -> Self {
        let mut market_state = HashMap::new();
        for market in markets {
            risk.update_mark(market.market_id, market.tick_size);
            market_state.insert(
                market.market_id,
                MarketState {
                    config: market,
                    book: OrderBook::new(),
                    batch: BatchAuction::default(),
                    pending: VecDeque::new(),
                },
            );
        }
        Self {
            shard_id,
            engine_seq: 0,
            next_order_id: 1,
            markets: market_state,
            risk,
            wal,
            dedupe: LruCache::new(std::num::NonZeroUsize::new(10_000).unwrap_or_else(|| std::num::NonZeroUsize::new(1).unwrap())),
            order_owners: HashMap::new(),
        }
    }

    pub fn snapshot(&self) -> EngineState {
        let mut orderbooks = HashMap::new();
        for (market_id, state) in &self.markets {
            let orders = state
                .book
                .order_views()
                .into_iter()
                .map(|order| OrderSnapshot {
                    order_id: order.order_id,
                    subaccount_id: order.subaccount_id,
                    side: order.side,
                    price_ticks: order.price_ticks,
                    remaining: order.remaining,
                    ingress_seq: order.ingress_seq,
                })
                .collect();
            orderbooks.insert(*market_id, orders);
        }
        EngineState {
            shard_id: self.shard_id,
            engine_seq: self.engine_seq,
            next_order_id: self.next_order_id,
            orderbooks,
            risk_state: self.risk.state.clone(),
        }
    }

    pub fn restore(state: EngineState, markets: Vec<MarketConfig>, wal: Wal, mut risk: RiskEngine) -> Self {
        let mut shard = EngineShard::new(state.shard_id, markets, wal, risk.clone());
        shard.engine_seq = state.engine_seq;
        shard.next_order_id = state.next_order_id;
        shard.risk.state = state.risk_state;
        for (market_id, orders) in state.orderbooks {
            if let Some(market_state) = shard.markets.get_mut(&market_id) {
                for order in orders {
                    let incoming = IncomingOrder {
                        order_id: order.order_id,
                        subaccount_id: order.subaccount_id,
                        side: order.side,
                        order_type: crate::models::OrderType::Limit,
                        tif: TimeInForce::Gtc,
                        price_ticks: order.price_ticks,
                        qty: order.remaining,
                        reduce_only: false,
                        ingress_seq: order.ingress_seq,
                    };
                    market_state.book.place_order(incoming, 0);
                    shard.order_owners.insert(order.order_id, (order.subaccount_id, order.side));
                }
            }
        }
        shard
    }

    #[instrument(skip(self))]
    pub fn handle_event(&mut self, event: Event, ts: u64) -> anyhow::Result<Vec<EventEnvelope>> {
        self.engine_seq += 1;
        let input = EventEnvelope {
            shard_id: self.shard_id,
            engine_seq: self.engine_seq,
            event: event.clone(),
            ts,
        };
        self.wal.append(&input)?;
        let outputs = match event {
            Event::NewOrder(order) => self.on_new_order(order, ts),
            Event::CancelOrder(cancel) => self.on_cancel(cancel, ts),
            Event::PriceUpdate(update) => {
                self.risk.update_mark(update.market_id, update.mark_price);
                Vec::new()
            }
            Event::FundingUpdate(update) => {
                self.risk.update_funding(update.market_id, update.funding_index);
                Vec::new()
            }
            _ => Vec::new(),
        };
        for output in &outputs {
            self.wal.append(output)?;
        }
        Ok(outputs)
    }

    fn on_new_order(&mut self, order: NewOrder, ts: u64) -> Vec<EventEnvelope> {
        if self.dedupe.contains(&order.request_id) {
            return Vec::new();
        }
        self.dedupe.put(order.request_id.clone(), ());
        let Some(market) = self.markets.get_mut(&order.market_id) else {
            return vec![self.reject(order.request_id, "unknown market", ts)];
        };
        if let Err(reason) = self.validate_order(&order, market) {
            return vec![self.reject(order.request_id, reason, ts)];
        }

        let order_id = self.next_order_id;
        self.next_order_id += 1;
        self.order_owners.insert(order_id, (order.subaccount_id, order.side));
        let incoming = IncomingOrder {
            order_id,
            subaccount_id: order.subaccount_id,
            side: order.side,
            order_type: order.order_type,
            tif: order.tif,
            price_ticks: order.price_ticks,
            qty: order.qty,
            reduce_only: order.reduce_only,
            ingress_seq: self.engine_seq,
        };

        let mut events = Vec::new();
        events.push(EventEnvelope {
            shard_id: self.shard_id,
            engine_seq: self.engine_seq,
            event: Event::OrderAck(OrderAck {
                request_id: order.request_id,
                status: OrderStatus::Accepted,
                reject_reason: None,
                assigned_order_id: Some(order_id),
                engine_seq: self.engine_seq,
                ts,
            }),
            ts,
        });

        match market.config.matching_mode {
            MatchingMode::Continuous => {
                let (fills, _) = market.book.place_order(incoming, 1024);
                events.extend(self.emit_fills(fills, &market.config, market, ts));
                events.push(self.emit_book_delta(order.market_id, market, ts));
            }
            MatchingMode::Batch => {
                market.batch.push(incoming);
            }
        }

        events
    }

    fn on_cancel(&mut self, cancel: CancelOrder, ts: u64) -> Vec<EventEnvelope> {
        if let Some(market) = self.markets.get_mut(&cancel.market_id) {
            if let Some(order_id) = cancel.order_id {
                if market.book.cancel(order_id) {
                    self.order_owners.remove(&order_id);
                    return vec![self.emit_book_delta(cancel.market_id, market, ts)];
                }
            }
        }
        Vec::new()
    }

    fn validate_order(&self, order: &NewOrder, market: &MarketState) -> Result<(), &'static str> {
        if order.order_type == crate::models::OrderType::PostOnly && market.book.would_cross(order.side, order.price_ticks) {
            return Err("post-only would cross");
        }
        self.risk
            .validate_order(
                &market.config,
                order.subaccount_id,
                order.side,
                order.order_type,
                order.price_ticks,
                order.qty,
                order.reduce_only,
            )
            .map_err(|err| match err {
                RiskError::PriceBand => "price band",
                RiskError::InsufficientMargin => "insufficient margin",
                RiskError::ReduceOnly => "reduce-only",
                RiskError::MaxPosition => "max position",
            })
    }

    fn reject(&self, request_id: String, reason: &str, ts: u64) -> EventEnvelope {
        EventEnvelope {
            shard_id: self.shard_id,
            engine_seq: self.engine_seq,
            event: Event::OrderAck(OrderAck {
                request_id,
                status: OrderStatus::Rejected,
                reject_reason: Some(reason.to_string()),
                assigned_order_id: None,
                engine_seq: self.engine_seq,
                ts,
            }),
            ts,
        }
    }

    fn emit_fills(&mut self, fills: Vec<Fill>, market: &MarketConfig, market_state: &MarketState, ts: u64) -> Vec<EventEnvelope> {
        fills
            .into_iter()
            .map(|mut fill| {
                fill.market_id = market.market_id;
                fill.engine_seq = self.engine_seq;
                fill.ts = ts;
                let maker_fee = fee_for(fill.qty, fill.price_ticks, market.maker_fee_bps);
                let taker_fee = fee_for(fill.qty, fill.price_ticks, market.taker_fee_bps);
                fill.maker_fee = maker_fee;
                fill.taker_fee = taker_fee;
                if let Some((maker_sub, maker_side)) = self.order_owners.get(&fill.maker_order_id).copied() {
                    self.risk.apply_fill(market, maker_sub, maker_side, fill.price_ticks, fill.qty, maker_fee);
                }
                if let Some((taker_sub, taker_side)) = self.order_owners.get(&fill.taker_order_id).copied() {
                    self.risk.apply_fill(market, taker_sub, taker_side, fill.price_ticks, fill.qty, taker_fee);
                }
                if !market_state.book.has_order(fill.maker_order_id) {
                    self.order_owners.remove(&fill.maker_order_id);
                }
                if !market_state.book.has_order(fill.taker_order_id) {
                    self.order_owners.remove(&fill.taker_order_id);
                }
                EventEnvelope {
                    shard_id: self.shard_id,
                    engine_seq: self.engine_seq,
                    event: Event::Fill(fill),
                    ts,
                }
            })
            .collect()
    }

    fn emit_book_delta(&self, market_id: MarketId, market: &MarketState, ts: u64) -> EventEnvelope {
        let snapshot = market.book.snapshot(10);
        let bids_levels = snapshot
            .bids
            .into_iter()
            .map(|(price, qty)| BookLevel {
                price_ticks: price,
                qty,
            })
            .collect();
        let asks_levels = snapshot
            .asks
            .into_iter()
            .map(|(price, qty)| BookLevel {
                price_ticks: price,
                qty,
            })
            .collect();
        EventEnvelope {
            shard_id: self.shard_id,
            engine_seq: self.engine_seq,
            event: Event::BookDelta(BookDelta {
                market_id,
                bids_levels,
                asks_levels,
                engine_seq: self.engine_seq,
                ts,
            }),
            ts,
        }
    }
}

fn fee_for(qty: u64, price_ticks: u64, fee_bps: i64) -> i64 {
    let notional = qty.saturating_mul(price_ticks) as i64;
    notional.saturating_mul(fee_bps) / 10_000
}
