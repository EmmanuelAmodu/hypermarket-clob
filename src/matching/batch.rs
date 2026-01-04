use std::cmp::Ordering;

use crate::matching::orderbook::IncomingOrder;
use crate::models::{Fill, OrderType, PriceTicks, Side, TimeInForce};

#[derive(Debug, Default)]
pub struct BatchAuction {
    pub pending: Vec<IncomingOrder>,
}

#[derive(Debug, Clone, Copy)]
pub struct ClearingResult {
    pub price: PriceTicks,
    pub volume: u64,
}

impl BatchAuction {
    pub fn push(&mut self, order: IncomingOrder) {
        self.pending.push(order);
    }

    pub fn clear(&mut self, mark_price: PriceTicks) -> (ClearingResult, Vec<Fill>, Vec<IncomingOrder>) {
        let orders = std::mem::take(&mut self.pending);
        if orders.is_empty() {
            return (
                ClearingResult {
                    price: mark_price,
                    volume: 0,
                },
                Vec::new(),
                Vec::new(),
            );
        }

        let mut candidates: Vec<PriceTicks> = orders
            .iter()
            .filter(|o| o.order_type != OrderType::Market)
            .map(|o| o.price_ticks)
            .collect();
        candidates.push(mark_price);
        candidates.sort_unstable();
        candidates.dedup();

        let mut best = ClearingResult {
            price: mark_price,
            volume: 0,
        };
        let mut best_imbalance = u64::MAX;
        let mut best_distance = u64::MAX;

        for price in candidates {
            let (buy, sell) = demand_supply(&orders, price);
            let volume = buy.min(sell);
            let imbalance = buy.max(sell) - volume;
            let distance = if price > mark_price {
                price - mark_price
            } else {
                mark_price - price
            };
            let better = volume > best.volume
                || (volume == best.volume && imbalance < best_imbalance)
                || (volume == best.volume && imbalance == best_imbalance && distance < best_distance)
                || (volume == best.volume
                    && imbalance == best_imbalance
                    && distance == best_distance
                    && price < best.price);
            if better {
                best = ClearingResult { price, volume };
                best_imbalance = imbalance;
                best_distance = distance;
            }
        }

        let mut buy_orders: Vec<IncomingOrder> = orders
            .iter()
            .cloned()
            .filter(|o| matches!(o.side, Side::Buy))
            .collect();
        let mut sell_orders: Vec<IncomingOrder> = orders
            .iter()
            .cloned()
            .filter(|o| matches!(o.side, Side::Sell))
            .collect();

        buy_orders.sort_by(|a, b| a.ingress_seq.cmp(&b.ingress_seq));
        sell_orders.sort_by(|a, b| a.ingress_seq.cmp(&b.ingress_seq));

        let mut fills = Vec::new();
        let mut remaining_buys = best.volume;
        let mut remaining_sells = best.volume;

        for buy in &mut buy_orders {
            if remaining_buys == 0 {
                break;
            }
            let tradable = buy.qty.min(remaining_buys);
            remaining_buys -= tradable;
            for sell in &mut sell_orders {
                if remaining_sells == 0 {
                    break;
                }
                if tradable == 0 {
                    break;
                }
                let trade_qty = tradable.min(remaining_sells).min(sell.qty);
                if trade_qty == 0 {
                    continue;
                }
                sell.qty -= trade_qty;
                remaining_sells -= trade_qty;
                fills.push(Fill {
                    market_id: 0,
                    maker_order_id: sell.order_id,
                    taker_order_id: buy.order_id,
                    price_ticks: best.price,
                    qty: trade_qty,
                    maker_fee: 0,
                    taker_fee: 0,
                    engine_seq: 0,
                    ts: 0,
                });
            }
        }

        let mut resting = Vec::new();
        for order in orders {
            if order.tif == TimeInForce::Gtc && order.order_type != OrderType::Market {
                resting.push(order);
            }
        }

        (best, fills, resting)
    }
}

fn demand_supply(orders: &[IncomingOrder], price: PriceTicks) -> (u64, u64) {
    let mut buy = 0u64;
    let mut sell = 0u64;
    for order in orders {
        match order.side {
            Side::Buy => {
                if order.order_type == OrderType::Market || order.price_ticks >= price {
                    buy += order.qty;
                }
            }
            Side::Sell => {
                if order.order_type == OrderType::Market || order.price_ticks <= price {
                    sell += order.qty;
                }
            }
        }
    }
    (buy, sell)
}

impl PartialEq for IncomingOrder {
    fn eq(&self, other: &Self) -> bool {
        self.order_id == other.order_id
    }
}

impl Eq for IncomingOrder {}

impl Ord for IncomingOrder {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ingress_seq.cmp(&other.ingress_seq)
    }
}

impl PartialOrd for IncomingOrder {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
