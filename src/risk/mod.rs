use std::collections::HashMap;

use crate::config::MarketConfig;
use crate::models::{MarketId, OrderType, PriceTicks, Side, SubaccountId};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Position {
    pub size: i64,
    pub entry_price: PriceTicks,
    pub funding_index: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Subaccount {
    pub collateral: i64,
    pub positions: HashMap<MarketId, Position>,
    pub cross_margin: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RiskState {
    pub subaccounts: HashMap<SubaccountId, Subaccount>,
    pub mark_prices: HashMap<MarketId, PriceTicks>,
    pub funding_indices: HashMap<MarketId, i64>,
}

#[derive(Debug, Clone)]
pub struct RiskConfig {
    pub max_slippage_bps: u64,
    pub max_leverage: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RiskError {
    #[error("price band violation")]
    PriceBand,
    #[error("insufficient margin")]
    InsufficientMargin,
    #[error("reduce-only violation")]
    ReduceOnly,
    #[error("max position exceeded")]
    MaxPosition,
}

#[derive(Debug)]
pub struct RiskEngine {
    pub state: RiskState,
    pub config: RiskConfig,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            state: RiskState {
                subaccounts: HashMap::new(),
                mark_prices: HashMap::new(),
                funding_indices: HashMap::new(),
            },
            config,
        }
    }

    pub fn update_mark(&mut self, market_id: MarketId, mark: PriceTicks) {
        self.state.mark_prices.insert(market_id, mark);
    }

    pub fn update_funding(&mut self, market_id: MarketId, index: i64) {
        self.state.funding_indices.insert(market_id, index);
    }

    pub fn ensure_subaccount(&mut self, subaccount_id: SubaccountId) -> &mut Subaccount {
        self.state.subaccounts.entry(subaccount_id).or_insert(Subaccount {
            collateral: 0,
            positions: HashMap::new(),
            cross_margin: false,
        })
    }

    pub fn validate_order(
        &self,
        market: &MarketConfig,
        subaccount_id: SubaccountId,
        side: Side,
        order_type: OrderType,
        price_ticks: PriceTicks,
        qty: u64,
        reduce_only: bool,
    ) -> Result<(), RiskError> {
        let mark = self.state.mark_prices.get(&market.market_id).copied().unwrap_or(price_ticks);
        let band = market.price_band_bps;
        if order_type != OrderType::Market {
            let lower = mark.saturating_sub(mark * band / 10_000);
            let upper = mark + mark * band / 10_000;
            if price_ticks < lower || price_ticks > upper {
                return Err(RiskError::PriceBand);
            }
        }

        let subaccount = self.state.subaccounts.get(&subaccount_id);
        let position = subaccount
            .and_then(|acc| acc.positions.get(&market.market_id))
            .map(|pos| pos.size)
            .unwrap_or(0);
        let delta = match side {
            Side::Buy => qty as i64,
            Side::Sell => -(qty as i64),
        };
        let projected = position + delta;
        if reduce_only && projected.abs() > position.abs() {
            return Err(RiskError::ReduceOnly);
        }
        if projected.abs() > market.max_position {
            return Err(RiskError::MaxPosition);
        }

        let equity = self.equity(subaccount_id);
        let notional = price_ticks.saturating_mul(qty);
        let im_required = (notional as u128 * market.initial_margin_bps as u128 / 10_000) as i64;
        if equity < im_required {
            return Err(RiskError::InsufficientMargin);
        }
        Ok(())
    }

    pub fn apply_fill(
        &mut self,
        market: &MarketConfig,
        subaccount_id: SubaccountId,
        side: Side,
        price_ticks: PriceTicks,
        qty: u64,
        fee: i64,
    ) {
        let subaccount = self.ensure_subaccount(subaccount_id);
        let position = subaccount
            .positions
            .entry(market.market_id)
            .or_insert(Position {
                size: 0,
                entry_price: price_ticks,
                funding_index: 0,
            });
        let delta = match side {
            Side::Buy => qty as i64,
            Side::Sell => -(qty as i64),
        };
        let new_size = position.size + delta;
        if new_size == 0 {
            position.size = 0;
            position.entry_price = price_ticks;
        } else {
            position.entry_price = price_ticks;
            position.size = new_size;
        }
        subaccount.collateral -= fee;
    }

    pub fn equity(&self, subaccount_id: SubaccountId) -> i64 {
        let Some(account) = self.state.subaccounts.get(&subaccount_id) else {
            return 0;
        };
        let mut equity = account.collateral;
        for (market_id, position) in &account.positions {
            let mark = self.state.mark_prices.get(market_id).copied().unwrap_or(position.entry_price);
            let pnl = (position.size as i128 * (mark as i128 - position.entry_price as i128)) / 1;
            equity += pnl as i64;
        }
        equity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduce_only_blocks_increase() {
        let mut engine = RiskEngine::new(RiskConfig {
            max_slippage_bps: 50,
            max_leverage: 10,
        });
        engine.ensure_subaccount(1).positions.insert(
            1,
            Position {
                size: 10,
                entry_price: 100,
                funding_index: 0,
            },
        );
        let market = MarketConfig {
            market_id: 1,
            tick_size: 1,
            lot_size: 1,
            maker_fee_bps: 1,
            taker_fee_bps: 2,
            initial_margin_bps: 500,
            maintenance_margin_bps: 250,
            max_position: 100,
            price_band_bps: 1000,
            matching_mode: crate::config::MatchingMode::Continuous,
            batch_interval_ms: 2000,
        };
        let res = engine.validate_order(
            &market,
            1,
            Side::Buy,
            OrderType::Limit,
            100,
            5,
            true,
        );
        assert!(matches!(res, Err(RiskError::ReduceOnly)));
    }
}
