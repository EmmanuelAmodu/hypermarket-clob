pub mod orderbook;
pub mod batch;

use crate::models::{Fill, OrderId};

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub fills: Vec<Fill>,
    pub remaining_order_id: Option<OrderId>,
}
