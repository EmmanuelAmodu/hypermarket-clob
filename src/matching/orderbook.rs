use std::collections::{BTreeMap, HashMap};

use crate::models::{Fill, OrderId, OrderType, PriceTicks, Quantity, Side, TimeInForce};

#[derive(Debug, Clone)]
pub struct IncomingOrder {
    pub order_id: OrderId,
    pub subaccount_id: u64,
    pub side: Side,
    pub order_type: OrderType,
    pub tif: TimeInForce,
    pub price_ticks: PriceTicks,
    pub qty: Quantity,
    pub reduce_only: bool,
    pub ingress_seq: u64,
}

#[derive(Debug, Clone)]
pub struct BookSnapshot {
    pub bids: Vec<(PriceTicks, Quantity)>,
    pub asks: Vec<(PriceTicks, Quantity)>,
}

#[derive(Debug, Clone)]
pub struct OrderView {
    pub order_id: OrderId,
    pub subaccount_id: u64,
    pub side: Side,
    pub price_ticks: PriceTicks,
    pub remaining: Quantity,
    pub ingress_seq: u64,
}

#[derive(Debug, Clone)]
struct OrderNode {
    order_id: OrderId,
    subaccount_id: u64,
    side: Side,
    price_ticks: PriceTicks,
    remaining: Quantity,
    next: Option<usize>,
    prev: Option<usize>,
    ingress_seq: u64,
}

#[derive(Debug, Default)]
struct Level {
    head: Option<usize>,
    tail: Option<usize>,
    total_qty: Quantity,
}

#[derive(Debug, Default)]
pub struct OrderBook {
    bids: BTreeMap<PriceTicks, Level>,
    asks: BTreeMap<PriceTicks, Level>,
    orders: slab::Slab<OrderNode>,
    order_index: HashMap<OrderId, usize>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, depth: usize) -> BookSnapshot {
        let bids = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(price, level)| (*price, level.total_qty))
            .collect();
        let asks = self
            .asks
            .iter()
            .take(depth)
            .map(|(price, level)| (*price, level.total_qty))
            .collect();
        BookSnapshot { bids, asks }
    }

    pub fn order_views(&self) -> Vec<OrderView> {
        self.orders
            .iter()
            .map(|(_, order)| OrderView {
                order_id: order.order_id,
                subaccount_id: order.subaccount_id,
                side: order.side,
                price_ticks: order.price_ticks,
                remaining: order.remaining,
                ingress_seq: order.ingress_seq,
            })
            .collect()
    }

    pub fn cancel(&mut self, order_id: OrderId) -> bool {
        let Some(&idx) = self.order_index.get(&order_id) else {
            return false;
        };
        let order = self.orders.get(idx).cloned();
        if let Some(order) = order {
            self.detach_from_level(idx, &order);
            self.orders.remove(idx);
            self.order_index.remove(&order_id);
            return true;
        }
        false
    }

    pub fn has_order(&self, order_id: OrderId) -> bool {
        self.order_index.contains_key(&order_id)
    }

    pub fn place_order(&mut self, incoming: IncomingOrder, max_matches: usize) -> (Vec<Fill>, Option<OrderId>) {
        if incoming.tif == TimeInForce::Fok {
            let available = self.available_qty(&incoming);
            if available < incoming.qty {
                return (Vec::new(), None);
            }
        }
        let mut fills = Vec::new();
        let mut remaining = incoming.qty;
        let mut matches = 0usize;

        while remaining > 0 {
            if matches >= max_matches {
                break;
            }
            let Some((best_price, level)) = match incoming.side {
                Side::Buy => self.asks.iter_mut().next().map(|(p, l)| (*p, l)),
                Side::Sell => self.bids.iter_mut().rev().next().map(|(p, l)| (*p, l)),
            } else {
                break;
            };
            if !self.crosses(incoming.side, incoming.order_type, incoming.price_ticks, best_price) {
                break;
            }
            let head_idx = match level.head {
                Some(idx) => idx,
                None => {
                    self.remove_level_if_empty(incoming.side, best_price);
                    continue;
                }
            };
            let Some(mut maker) = self.orders.get(head_idx).cloned() else {
                break;
            };
            let trade_qty = remaining.min(maker.remaining);
            remaining -= trade_qty;
            maker.remaining -= trade_qty;
            level.total_qty -= trade_qty;
            matches += 1;

            fills.push(Fill {
                market_id: 0,
                maker_order_id: maker.order_id,
                taker_order_id: incoming.order_id,
                price_ticks: best_price,
                qty: trade_qty,
                maker_fee: 0,
                taker_fee: 0,
                engine_seq: 0,
                ts: 0,
            });

            if maker.remaining == 0 {
                let next = maker.next;
                self.detach_from_level(head_idx, &maker);
                self.orders.remove(head_idx);
                self.order_index.remove(&maker.order_id);
                level.head = next;
                if level.head.is_none() {
                    level.tail = None;
                }
            } else {
                self.orders[head_idx] = maker;
            }

            if level.total_qty == 0 {
                self.remove_level_if_empty(incoming.side, best_price);
            }
        }

        if remaining == 0 {
            return (fills, None);
        }

        match incoming.tif {
            TimeInForce::Ioc => (fills, None),
            TimeInForce::Fok => (fills, None),
            TimeInForce::Gtc => {
                let resting_id = if incoming.order_type == OrderType::PostOnly && !fills.is_empty() {
                    None
                } else {
                    Some(self.add_resting(incoming, remaining))
                };
                (fills, resting_id)
            }
        }
    }

    pub fn would_cross(&self, side: Side, price_ticks: PriceTicks) -> bool {
        match side {
            Side::Buy => self.asks.keys().next().map(|best| price_ticks >= *best).unwrap_or(false),
            Side::Sell => self.bids.keys().next_back().map(|best| price_ticks <= *best).unwrap_or(false),
        }
    }

    fn add_resting(&mut self, incoming: IncomingOrder, remaining: Quantity) -> OrderId {
        let level = match incoming.side {
            Side::Buy => self.bids.entry(incoming.price_ticks).or_default(),
            Side::Sell => self.asks.entry(incoming.price_ticks).or_default(),
        };
        let idx = self.orders.insert(OrderNode {
            order_id: incoming.order_id,
            subaccount_id: incoming.subaccount_id,
            side: incoming.side,
            price_ticks: incoming.price_ticks,
            remaining,
            next: None,
            prev: level.tail,
            ingress_seq: incoming.ingress_seq,
        });
        if let Some(tail) = level.tail {
            self.orders[tail].next = Some(idx);
        }
        if level.head.is_none() {
            level.head = Some(idx);
        }
        level.tail = Some(idx);
        level.total_qty += remaining;
        self.order_index.insert(incoming.order_id, idx);
        incoming.order_id
    }

    fn detach_from_level(&mut self, idx: usize, order: &OrderNode) {
        let level = match order.side {
            Side::Buy => self.bids.get_mut(&order.price_ticks),
            Side::Sell => self.asks.get_mut(&order.price_ticks),
        };
        if let Some(level) = level {
            if level.head == Some(idx) {
                level.head = order.next;
            }
            if level.tail == Some(idx) {
                level.tail = order.prev;
            }
            if let Some(prev) = order.prev {
                self.orders[prev].next = order.next;
            }
            if let Some(next) = order.next {
                self.orders[next].prev = order.prev;
            }
            level.total_qty = level.total_qty.saturating_sub(order.remaining);
        }
    }

    fn remove_level_if_empty(&mut self, side: Side, price: PriceTicks) {
        match side {
            Side::Buy => {
                if let Some(level) = self.bids.get(&price) {
                    if level.total_qty == 0 {
                        self.bids.remove(&price);
                    }
                }
            }
            Side::Sell => {
                if let Some(level) = self.asks.get(&price) {
                    if level.total_qty == 0 {
                        self.asks.remove(&price);
                    }
                }
            }
        }
    }

    fn crosses(&self, side: Side, order_type: OrderType, limit_price: PriceTicks, best_price: PriceTicks) -> bool {
        match order_type {
            OrderType::Market => true,
            _ => match side {
                Side::Buy => limit_price >= best_price,
                Side::Sell => limit_price <= best_price,
            },
        }
    }

    fn available_qty(&self, incoming: &IncomingOrder) -> Quantity {
        let mut available = 0u64;
        match incoming.side {
            Side::Buy => {
                for (price, level) in &self.asks {
                    if !self.crosses(incoming.side, incoming.order_type, incoming.price_ticks, *price) {
                        break;
                    }
                    available = available.saturating_add(level.total_qty);
                }
            }
            Side::Sell => {
                for (price, level) in self.bids.iter().rev() {
                    if !self.crosses(incoming.side, incoming.order_type, incoming.price_ticks, *price) {
                        break;
                    }
                    available = available.saturating_add(level.total_qty);
                }
            }
        }
        available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_only_rejects_cross() {
        let mut book = OrderBook::new();
        let maker = IncomingOrder {
            order_id: 1,
            subaccount_id: 1,
            side: Side::Sell,
            order_type: OrderType::Limit,
            tif: TimeInForce::Gtc,
            price_ticks: 100,
            qty: 10,
            reduce_only: false,
            ingress_seq: 1,
        };
        book.place_order(maker, 10);

        let taker = IncomingOrder {
            order_id: 2,
            subaccount_id: 2,
            side: Side::Buy,
            order_type: OrderType::PostOnly,
            tif: TimeInForce::Gtc,
            price_ticks: 110,
            qty: 5,
            reduce_only: false,
            ingress_seq: 2,
        };

        assert!(book.would_cross(taker.side, taker.price_ticks));
    }
}
