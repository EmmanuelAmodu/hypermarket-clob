use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::types::{MarketId, NewOrder, OrderId, Price, Quantity, Side, TimeInForce, UserId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fill {
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub price: Price,
    pub quantity: Quantity,
    pub maker_user_id: UserId,
    pub taker_user_id: UserId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionReport {
    FullyFilled {
        order_id: OrderId,
        fills: Vec<Fill>,
    },
    Posted {
        order_id: OrderId,
        remaining: Quantity,
        fills: Vec<Fill>,
    },
    PartiallyFilledAndPosted {
        order_id: OrderId,
        remaining: Quantity,
        fills: Vec<Fill>,
    },
    PartiallyFilledAndCanceled {
        order_id: OrderId,
        remaining: Quantity,
        fills: Vec<Fill>,
    },
    Canceled {
        order_id: OrderId,
        remaining: Quantity,
        fills: Vec<Fill>,
    },
}

impl ExecutionReport {
    pub fn order_id(&self) -> OrderId {
        match *self {
            Self::FullyFilled { order_id, .. }
            | Self::Posted { order_id, .. }
            | Self::PartiallyFilledAndPosted { order_id, .. }
            | Self::PartiallyFilledAndCanceled { order_id, .. }
            | Self::Canceled { order_id, .. } => order_id,
        }
    }

    pub fn fills(&self) -> &[Fill] {
        match self {
            Self::FullyFilled { fills, .. }
            | Self::Posted { fills, .. }
            | Self::PartiallyFilledAndPosted { fills, .. }
            | Self::PartiallyFilledAndCanceled { fills, .. }
            | Self::Canceled { fills, .. } => fills,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceOrderError {
    ZeroQuantity,
    FokNotFillable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderBookSnapshot {
    pub best_bid: Option<(Price, Quantity)>,
    pub best_ask: Option<(Price, Quantity)>,
}

#[derive(Clone, Debug)]
struct RestingOrder {
    user_id: UserId,
    side: Side,
    price: Price,
    remaining: Quantity,
}

#[derive(Default)]
struct OrderBook {
    bids: BTreeMap<Price, VecDeque<OrderId>>,
    asks: BTreeMap<Price, VecDeque<OrderId>>,
    orders: HashMap<OrderId, RestingOrder>,
}

impl OrderBook {
    fn snapshot(&self) -> OrderBookSnapshot {
        OrderBookSnapshot {
            best_bid: self.best_price_level(Side::Bid),
            best_ask: self.best_price_level(Side::Ask),
        }
    }

    fn best_price_level(&self, side: Side) -> Option<(Price, Quantity)> {
        match side {
            Side::Bid => {
                let (&price, _) = self.bids.last_key_value()?;
                Some((price, self.level_quantity(side, price)))
            }
            Side::Ask => {
                let (&price, _) = self.asks.first_key_value()?;
                Some((price, self.level_quantity(side, price)))
            }
        }
    }

    fn level_quantity(&self, side: Side, price: Price) -> Quantity {
        let queue = match side {
            Side::Bid => self.bids.get(&price),
            Side::Ask => self.asks.get(&price),
        };
        let Some(queue) = queue else { return 0 };
        queue
            .iter()
            .filter_map(|id| self.orders.get(id))
            .map(|o| o.remaining)
            .sum()
    }

    fn add_resting(&mut self, side: Side, price: Price, order_id: OrderId, order: RestingOrder) {
        self.orders.insert(order_id, order);
        let levels = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        levels.entry(price).or_default().push_back(order_id);
    }

    fn peek_best_maker(&self, maker_side: Side) -> Option<(Price, OrderId)> {
        match maker_side {
            Side::Bid => {
                let (&price, queue) = self.bids.last_key_value()?;
                let maker_id = *queue.front()?;
                Some((price, maker_id))
            }
            Side::Ask => {
                let (&price, queue) = self.asks.first_key_value()?;
                let maker_id = *queue.front()?;
                Some((price, maker_id))
            }
        }
    }

    fn pop_front_at_price(&mut self, side: Side, price: Price) -> Option<OrderId> {
        let levels = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let queue = levels.get_mut(&price)?;
        let id = queue.pop_front()?;
        if queue.is_empty() {
            levels.remove(&price);
        }
        Some(id)
    }

    fn cancel(&mut self, order_id: OrderId) -> bool {
        let Some(resting) = self.orders.remove(&order_id) else {
            return false;
        };

        let levels = match resting.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        if let Some(queue) = levels.get_mut(&resting.price) {
            queue.retain(|&id| id != order_id);
            if queue.is_empty() {
                levels.remove(&resting.price);
            }
        }
        true
    }

    fn can_fully_fill(&self, order: &NewOrder) -> bool {
        let side = order.side();
        let mut remaining = order.quantity();
        if remaining == 0 {
            return false;
        }

        match order {
            NewOrder::Market { .. } => match side {
                Side::Bid => {
                    for (&_price, queue) in &self.asks {
                        for id in queue {
                            let Some(o) = self.orders.get(id) else {
                                continue;
                            };
                            remaining = remaining.saturating_sub(o.remaining);
                            if remaining == 0 {
                                return true;
                            }
                        }
                    }
                    false
                }
                Side::Ask => {
                    for (&price, queue) in self.bids.iter().rev() {
                        let _ = price;
                        for id in queue {
                            let Some(o) = self.orders.get(id) else {
                                continue;
                            };
                            remaining = remaining.saturating_sub(o.remaining);
                            if remaining == 0 {
                                return true;
                            }
                        }
                    }
                    false
                }
            },
            NewOrder::Limit { price: limit, .. } => match side {
                Side::Bid => {
                    for (&price, queue) in &self.asks {
                        if price > *limit {
                            break;
                        }
                        for id in queue {
                            let Some(o) = self.orders.get(id) else {
                                continue;
                            };
                            remaining = remaining.saturating_sub(o.remaining);
                            if remaining == 0 {
                                return true;
                            }
                        }
                    }
                    false
                }
                Side::Ask => {
                    for (&price, queue) in self.bids.iter().rev() {
                        if price < *limit {
                            break;
                        }
                        for id in queue {
                            let Some(o) = self.orders.get(id) else {
                                continue;
                            };
                            remaining = remaining.saturating_sub(o.remaining);
                            if remaining == 0 {
                                return true;
                            }
                        }
                    }
                    false
                }
            },
        }
    }
}

#[derive(Default)]
pub struct MatchingEngine {
    books: HashMap<MarketId, OrderBook>,
    next_order_id: OrderId,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self, market_id: &str) -> OrderBookSnapshot {
        self.books
            .get(market_id)
            .map(|b| b.snapshot())
            .unwrap_or(OrderBookSnapshot {
                best_bid: None,
                best_ask: None,
            })
    }

    pub fn cancel_order(&mut self, market_id: &str, order_id: OrderId) -> bool {
        self.books
            .get_mut(market_id)
            .is_some_and(|b| b.cancel(order_id))
    }

    pub fn place_order(
        &mut self,
        market_id: impl Into<MarketId>,
        order: NewOrder,
    ) -> Result<ExecutionReport, PlaceOrderError> {
        if order.quantity() == 0 {
            return Err(PlaceOrderError::ZeroQuantity);
        }

        let market_id = market_id.into();
        let book = self.books.entry(market_id).or_default();

        if order.time_in_force() == TimeInForce::Fok && !book.can_fully_fill(&order) {
            return Err(PlaceOrderError::FokNotFillable);
        }

        let order_id = self.next_order_id;
        self.next_order_id = self.next_order_id.wrapping_add(1);

        let taker_user_id = order.user_id();
        let taker_side = order.side();
        let taker_tif = order.time_in_force();
        let mut taker_remaining = order.quantity();
        let mut fills = Vec::new();

        let (taker_is_market, taker_limit_price) = match order {
            NewOrder::Market { .. } => (true, None),
            NewOrder::Limit { price, .. } => (false, Some(price)),
        };

        while taker_remaining > 0 {
            let maker_side = taker_side.opposite();
            let Some((maker_price, maker_id)) = book.peek_best_maker(maker_side) else {
                break;
            };

            if !taker_is_market {
                let limit = taker_limit_price.expect("limit price checked above");
                let crosses = match taker_side {
                    Side::Bid => maker_price <= limit,
                    Side::Ask => maker_price >= limit,
                };
                if !crosses {
                    break;
                }
            }

            let maker_remaining = book.orders.get(&maker_id).map(|o| o.remaining).unwrap_or(0);
            if maker_remaining == 0 {
                let popped = book.pop_front_at_price(maker_side, maker_price);
                debug_assert_eq!(popped, Some(maker_id));
                book.orders.remove(&maker_id);
                continue;
            }

            let fill_qty = taker_remaining.min(maker_remaining);
            let (maker_user_id, maker_filled) = {
                let maker = book
                    .orders
                    .get_mut(&maker_id)
                    .expect("maker id from book should exist");
                maker.remaining -= fill_qty;
                (maker.user_id, maker.remaining == 0)
            };

            taker_remaining -= fill_qty;
            fills.push(Fill {
                maker_order_id: maker_id,
                taker_order_id: order_id,
                price: maker_price,
                quantity: fill_qty,
                maker_user_id,
                taker_user_id,
            });

            if maker_filled {
                let popped = book.pop_front_at_price(maker_side, maker_price);
                debug_assert_eq!(popped, Some(maker_id));
                book.orders.remove(&maker_id);
            }
        }

        let filled_qty = order.quantity() - taker_remaining;
        let should_post = match order {
            NewOrder::Limit { .. } => taker_tif == TimeInForce::Gtc,
            NewOrder::Market { .. } => false,
        };

        if taker_remaining == 0 {
            return Ok(ExecutionReport::FullyFilled { order_id, fills });
        }

        if should_post {
            let price = taker_limit_price.expect("limit order must have price");
            book.add_resting(
                taker_side,
                price,
                order_id,
                RestingOrder {
                    user_id: taker_user_id,
                    side: taker_side,
                    price,
                    remaining: taker_remaining,
                },
            );

            if filled_qty == 0 {
                Ok(ExecutionReport::Posted {
                    order_id,
                    remaining: taker_remaining,
                    fills,
                })
            } else {
                Ok(ExecutionReport::PartiallyFilledAndPosted {
                    order_id,
                    remaining: taker_remaining,
                    fills,
                })
            }
        } else if filled_qty == 0 {
            Ok(ExecutionReport::Canceled {
                order_id,
                remaining: taker_remaining,
                fills,
            })
        } else {
            Ok(ExecutionReport::PartiallyFilledAndCanceled {
                order_id,
                remaining: taker_remaining,
                fills,
            })
        }
    }
}
