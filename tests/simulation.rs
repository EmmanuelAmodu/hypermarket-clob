use std::path::PathBuf;

use hypermarket_clob::config::{MarketConfig, MatchingMode};
use hypermarket_clob::engine::shard::EngineShard;
use hypermarket_clob::models::{Event, NewOrder, OrderType, PriceUpdate, Side, TimeInForce};
use hypermarket_clob::persistence::wal::Wal;
use hypermarket_clob::risk::{RiskConfig, RiskEngine};

fn market(mode: MatchingMode) -> MarketConfig {
    MarketConfig {
        market_id: 1,
        tick_size: 1,
        lot_size: 1,
        maker_fee_bps: 1,
        taker_fee_bps: 2,
        initial_margin_bps: 1,
        maintenance_margin_bps: 1,
        max_position: 1000,
        price_band_bps: 10_000,
        max_open_orders_per_subaccount: 0,
        matching_mode: mode,
        batch_interval_ms: 2000,
    }
}

#[test]
fn oracle_price_jump() {
    let wal = Wal::open(&PathBuf::from(std::env::temp_dir().join("sim.wal"))).unwrap();
    let risk = RiskEngine::new(RiskConfig { max_slippage_bps: 50, max_leverage: 10 });
    let mut shard = EngineShard::new(0, vec![market(MatchingMode::Continuous)], wal, risk);
    let update = PriceUpdate { market_id: 1, mark_price: 200, index_price: 200, ts: 1 };
    let _ = shard.handle_event(Event::PriceUpdate(update), 1);
    let order = NewOrder {
        request_id: "req-1".to_string(),
        market_id: 1,
        subaccount_id: 1,
        side: Side::Buy,
        order_type: OrderType::Limit,
        tif: TimeInForce::Gtc,
        price_ticks: 200,
        qty: 1,
        reduce_only: false,
        expiry_ts: 0,
        nonce: 1,
        client_ts: 0,
    };
    let outputs = shard.handle_event(Event::NewOrder(order), 2).unwrap();
    assert!(!outputs.is_empty());
}
